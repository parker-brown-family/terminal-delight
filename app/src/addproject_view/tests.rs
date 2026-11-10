use super::*;
use std::process::Command;

/// A folder of the test's own, gone when the test is.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        use std::sync::atomic::AtomicUsize;
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "td-addview-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A stand-in ssh: it skips its options and the host, records the host, and
/// runs the line it was handed in a shell whose home is `far_home`, the way a
/// far machine's login shell would. Two hosts misbehave on purpose.
fn fake_ssh(dir: &Path, far_home: &Path) -> PathBuf {
    let script = dir.join("ssh");
    let _fork = crate::testsync::forks_and_locks();
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\n[ \"$1\" = probe ] && exit 0\n\
             while [ \"$1\" != \"--\" ]; do shift; done\nshift\nhost=$1\nshift\n\
             echo \"$host\" >> '{log}'\n\
             case $host in\n  \
             untrusted) printf 'No ED25519 host key is known for untrusted and you have requested strict checking.\\r\\nHost key verification failed.\\r\\n' >&2; exit 255;;\n  \
             gone) echo 'ssh: connect to host gone port 22: Connection timed out' >&2; exit 255;;\n\
             esac\n\
             HOME='{home}' exec sh -c \"$*\"\n",
            log = dir.join("hosts").display(),
            home = far_home.display(),
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    drop(_fork);
    // A file just written cannot be run while a child forked on another
    // thread still holds it open for writing (ETXTBSY): wait until it can.
    assert!(
        crate::testsync::soon(|| Command::new(&script).arg("probe").status().is_ok()),
        "the stand-in ssh never became runnable"
    );
    script
}

fn window(
    cx: &mut gpui::TestAppContext,
) -> (gpui::Entity<Workspace>, &mut gpui::VisualTestContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        theme::init_embedded(cx, theme::house_outer());
        skin::init_embedded(cx);
    });
    let seed = session::PaneRestore {
        program: Some(("/bin/sh".into(), vec!["-c".into(), "exec /bin/cat".into()])),
        ..Default::default()
    };
    let window = {
        let _fork = crate::testsync::forks_and_locks();
        cx.open_window(size(px(1400.), px(900.)), |w, cx| {
            Workspace::build(true, true, Some(seed), None, w, cx)
        })
    };
    let ws = window.root(cx).unwrap();
    let cx = gpui::VisualTestContext::from_window(window.into(), cx).into_mut();
    let stops: Vec<_> = ws
        .read_with(cx, |ws, _| ws.every_view())
        .into_iter()
        .map(|p| p.read_with(cx, |v, _| v.fixture_shutdown()))
        .collect();
    cx.on_quit(move || stops.into_iter().for_each(|s| s()));
    (ws, cx)
}

fn redraw(cx: &mut gpui::VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear();
        });
    }
    cx.run_until_parked();
}

/// Wait for background work the test dispatcher hands to real processes.
fn settle(
    cx: &mut gpui::VisualTestContext,
    done: impl Fn(&Workspace) -> bool,
    ws: &gpui::Entity<Workspace>,
) {
    for _ in 0..200 {
        redraw(cx);
        if ws.read_with(cx, |ws, _| done(ws)) {
            return;
        }
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the dialog never settled");
}

/// Open the composer with a known list of machines, then the dialog with
/// this test's own config, home and ssh — never the person's.
fn open(
    ws: &gpui::Entity<Workspace>,
    cx: &mut gpui::VisualTestContext,
    machines: Vec<entry::Machine>,
    sentence: &str,
    config: &Path,
    home: &Path,
    ssh: &Path,
) {
    let (config, home, ssh) = (config.to_path_buf(), home.to_path_buf(), ssh.to_path_buf());
    let sentence = sentence.to_string();
    cx.update(|w, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_composer(w, cx);
            let c = ws.composer.as_mut().unwrap();
            c.machines = machines;
            c.machine = 0;
            c.text = EditBuffer::at_end(&sentence);
            ws.add_project_open_with(config, home, ssh.to_string_lossy().into(), cx);
        })
    });
    redraw(cx);
}

fn here() -> entry::Machine {
    entry::Machine::here(Some("legion"), "this machine")
}

fn farbox() -> entry::Machine {
    entry::Machine {
        name: "farbox".into(),
        ssh: Some("farbox".into()),
    }
}

fn adding(ws: &Workspace) -> Option<&AddProject> {
    ws.composer.as_ref()?.adding.as_ref()
}

fn browse(ws: &Workspace) -> Option<&Browse> {
    match &adding(ws)?.step {
        Step::Folder(b) => Some(b),
        _ => None,
    }
}

fn chosen(ws: &Workspace) -> (Option<launcher::Project>, String, Option<String>) {
    let c = ws.composer.as_ref().unwrap();
    (
        c.project.and_then(|i| c.projects.get(i)).cloned(),
        c.machines[c.machine].name.clone(),
        c.note.clone(),
    )
}

/// Ctrl+O on this machine: the folders of `~/Work` are listed with the
/// checkouts marked, typing narrows them, Tab completes, and Enter on a folder
/// that is there adds it — landing back on the composer as its PROJECT. A
/// folder the scan already sees is not written to projects.toml.
#[gpui::test]
fn a_folder_picked_here_becomes_the_composers_project(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    let (home, config, bin) = (
        Scratch::new("home"),
        Scratch::new("config"),
        Scratch::new("bin"),
    );
    std::fs::create_dir_all(home.path().join("Work/alpha/.git")).unwrap();
    std::fs::create_dir_all(home.path().join("Work/beta")).unwrap();
    let ssh = fake_ssh(bin.path(), home.path());
    open(&ws, cx, vec![here()], "", config.path(), home.path(), &ssh);
    // Only this machine and no ssh hosts: straight to the folders.
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.rows().len() == 3),
        &ws,
    );
    ws.read_with(cx, |ws, _| {
        let rows: Vec<(String, bool)> = browse(ws)
            .unwrap()
            .rows()
            .iter()
            .map(|r| (r.label.clone(), r.git))
            .collect();
        assert_eq!(
            rows,
            [
                ("..".into(), false),
                ("alpha".into(), true),
                ("beta".into(), false)
            ]
        );
    });
    assert!(
        cx.debug_bounds("add-project-folder-1").is_some(),
        "the rows are drawn"
    );
    cx.simulate_keystrokes("a l");
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        let b = browse(ws).unwrap();
        assert_eq!(b.field.text(), "~/Work/al");
        assert_eq!(b.rows().len(), 2, "the way up and alpha");
    });
    cx.simulate_keystrokes("tab");
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.exists() == Some(true)),
        &ws,
    );
    cx.simulate_keystrokes("enter");
    settle(cx, |ws| adding(ws).is_none(), &ws);
    ws.read_with(cx, |ws, _| {
        let (project, machine, note) = chosen(ws);
        let project = project.expect("a project is chosen");
        assert_eq!(project.path, home.path().join("Work/alpha"));
        assert!(project.repo);
        assert_eq!(project.machine, None);
        assert_eq!(machine, "legion");
        assert_eq!(note.as_deref(), Some("Added alpha."));
    });
    assert!(
        !addproject::projects_path(config.path()).exists(),
        "the scan sees ~/Work/alpha, so nothing is written"
    );
}

/// A folder that is not there is never made by a plain Enter, only by
/// Ctrl+Enter; made deep under home, out of the scan's sight, it is written to
/// projects.toml.
#[gpui::test]
fn a_missing_folder_is_made_only_when_asked_and_then_remembered(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    let (home, config, bin) = (
        Scratch::new("home"),
        Scratch::new("config"),
        Scratch::new("bin"),
    );
    std::fs::create_dir_all(home.path().join("Work")).unwrap();
    let ssh = fake_ssh(bin.path(), home.path());
    open(&ws, cx, vec![here()], "", config.path(), home.path(), &ssh);
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.exists() == Some(true)),
        &ws,
    );
    // Select the whole field and type over it.
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("~/deep/new/thing");
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.exists() == Some(false)),
        &ws,
    );
    cx.simulate_keystrokes("enter");
    redraw(cx);
    let made = home.path().join("deep/new/thing");
    assert!(!made.exists(), "Enter makes nothing");
    ws.read_with(cx, |ws, _| {
        let note = browse(ws).unwrap().note.clone().unwrap_or_default();
        assert!(note.contains("does not exist yet"), "{note}");
    });
    cx.simulate_keystrokes("ctrl-enter");
    settle(cx, |ws| adding(ws).is_none(), &ws);
    assert!(made.is_dir(), "Ctrl+Enter makes it");
    ws.read_with(cx, |ws, _| {
        let (project, _, _) = chosen(ws);
        assert_eq!(project.unwrap().path, made);
    });
    assert_eq!(
        addproject::load_projects(config.path()).unwrap(),
        vec![addproject::Added {
            path: made.to_string_lossy().into(),
            machine: None,
            git: None,
        }]
    );
}

/// On a far machine everything goes over ssh: the machine is checked when the
/// dialog opens, its folder listed and made by the far script, and the
/// project comes back carrying its machine. Sent with another machine chosen,
/// the composer refuses with a note instead of starting in a missing folder.
#[gpui::test]
fn a_far_folder_is_listed_and_added_over_ssh_and_keeps_its_machine(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    let (home, far, config, bin) = (
        Scratch::new("home"),
        Scratch::new("far"),
        Scratch::new("config"),
        Scratch::new("bin"),
    );
    std::fs::create_dir_all(far.path().join("Work/proj")).unwrap();
    let ssh = fake_ssh(bin.path(), far.path());
    open(
        &ws,
        cx,
        vec![here(), farbox()],
        "",
        config.path(),
        home.path(),
        &ssh,
    );
    settle(
        cx,
        |ws| {
            adding(ws)
                .is_some_and(|a| matches!(a.places.get(1), Some(Place::Far(_, Seen::Reachable))))
        },
        &ws,
    );
    assert!(cx.debug_bounds("add-project-machine-1").is_some());
    cx.simulate_keystrokes("down enter");
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.rows().len() == 2),
        &ws,
    );
    // Down to `proj`, open it, add it.
    cx.simulate_keystrokes("down down enter");
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.exists() == Some(true)),
        &ws,
    );
    ws.read_with(cx, |ws, _| {
        assert_eq!(browse(ws).unwrap().field.text(), "~/Work/proj/");
    });
    cx.simulate_keystrokes("enter");
    settle(cx, |ws| adding(ws).is_none(), &ws);
    let path = far.path().join("Work/proj");
    ws.read_with(cx, |ws, _| {
        let (project, machine, note) = chosen(ws);
        let project = project.unwrap();
        assert_eq!(project.path, path);
        assert_eq!(project.machine.as_deref(), Some("farbox"));
        assert_eq!(machine, "farbox", "MACHINE follows the project");
        assert_eq!(note.as_deref(), Some("Added proj on farbox."));
    });
    assert_eq!(
        addproject::load_projects(config.path()).unwrap(),
        vec![addproject::Added {
            path: path.to_string_lossy().into(),
            machine: Some("farbox".into()),
            git: Some(false),
        }]
    );
    let hosts = std::fs::read_to_string(bin.path().join("hosts")).unwrap();
    assert!(hosts.lines().all(|h| h == "farbox"), "{hosts}");
    // Choose this machine instead and send: refused, nothing started.
    let tabs = ws.read_with(cx, |ws, _| ws.tabs.len());
    cx.update(|w, cx| {
        ws.update(cx, |ws, cx| {
            let c = ws.composer.as_mut().unwrap();
            c.machine = 0;
            c.text = EditBuffer::at_end("fix it");
            ws.composer_send(entry::Send::Open, w, cx);
        })
    });
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.tabs.len(), tabs, "no tab was opened");
        let note = ws
            .composer
            .as_ref()
            .unwrap()
            .note
            .clone()
            .unwrap_or_default();
        assert!(note.contains("proj is on farbox"), "{note}");
    });
    // Found again by typing on the PROJECT chip, the far project brings its
    // machine back with it.
    cx.update(|_, cx| {
        ws.update(cx, |ws, _| {
            ws.composer.as_mut().unwrap().focus = entry::Focus::Project;
        })
    });
    cx.simulate_keystrokes("p r o j");
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        let (project, machine, _) = chosen(ws);
        assert_eq!(project.unwrap().machine.as_deref(), Some("farbox"));
        assert_eq!(
            machine, "farbox",
            "MACHINE follows a far project chosen by name"
        );
    });
}

/// A host from ~/.ssh/config is checked before it is added: an untrusted one
/// is refused with the one command that fixes it and written nowhere; a good
/// one is appended to machines.toml and its folders open. Git remotes in the
/// config are never offered.
#[gpui::test]
fn a_new_ssh_host_is_checked_before_it_is_written_to_machines_toml(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    let (home, far, config, bin) = (
        Scratch::new("home"),
        Scratch::new("far"),
        Scratch::new("config"),
        Scratch::new("bin"),
    );
    std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
    std::fs::write(
        home.path().join(".ssh/config"),
        "Host github-bfs\n  HostName github.com\n  User git\nHost untrusted\nHost lab\n",
    )
    .unwrap();
    std::fs::create_dir_all(far.path().join("Work")).unwrap();
    let ssh = fake_ssh(bin.path(), far.path());
    open(&ws, cx, vec![here()], "", config.path(), home.path(), &ssh);
    ws.read_with(cx, |ws, _| {
        let a = adding(ws).unwrap();
        let names: Vec<String> = a
            .places
            .iter()
            .map(|p| match p {
                Place::Here(m) | Place::Far(m, _) => m.name.clone(),
                Place::Host(h) => h.alias().to_string(),
            })
            .collect();
        assert_eq!(names, ["legion", "untrusted", "lab"]);
        assert_eq!(a.git_hosts, 1);
    });
    cx.simulate_keystrokes("down enter");
    settle(
        cx,
        |ws| adding(ws).is_some_and(|a| matches!(a.step, Step::Machines) && a.note.is_some()),
        &ws,
    );
    ws.read_with(cx, |ws, _| {
        let note = adding(ws).unwrap().note.clone().unwrap();
        assert!(
            note.contains("Run ssh untrusted once in a terminal"),
            "{note}"
        );
    });
    assert!(
        !entry::machines_path(config.path()).exists(),
        "nothing written"
    );
    cx.simulate_keystrokes("down enter");
    settle(cx, |ws| browse(ws).is_some(), &ws);
    ws.read_with(cx, |ws, _| {
        assert_eq!(browse(ws).unwrap().machine.name, "lab");
        let c = ws.composer.as_ref().unwrap();
        assert!(
            c.machines.iter().any(|m| m.name == "lab"),
            "the chip has it now"
        );
    });
    assert_eq!(
        entry::load_machines(config.path()).unwrap(),
        vec![entry::Machine {
            name: "lab".into(),
            ssh: Some("lab".into()),
        }]
    );
}

/// Clone or start new lands in the folder being shown: a repository URL
/// named in the composer's sentence waits in the field, a clone from a local
/// repository becomes the project, and a plain name starts an empty one.
#[gpui::test]
fn a_clone_or_a_new_repository_lands_in_the_folder_shown(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    let (home, config, bin, src) = (
        Scratch::new("home"),
        Scratch::new("config"),
        Scratch::new("bin"),
        Scratch::new("src"),
    );
    std::fs::create_dir_all(home.path().join("Work")).unwrap();
    {
        let _fork = crate::testsync::forks_and_locks();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .arg(src.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()
            .unwrap()
            .success());
    }
    let ssh = fake_ssh(bin.path(), home.path());
    open(
        &ws,
        cx,
        vec![here()],
        "https://github.com/tcballard/build-omarchy-plugins\n\nhammer this",
        config.path(),
        home.path(),
        &ssh,
    );
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.exists() == Some(true)),
        &ws,
    );
    cx.simulate_keystrokes("ctrl-n");
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        let field = browse(ws).unwrap().new.as_ref().unwrap().text();
        assert_eq!(field, "https://github.com/tcballard/build-omarchy-plugins");
    });
    assert!(cx.debug_bounds("add-project-new-field").is_some());
    // Over it, a clone from a repository on this disk.
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input(&format!("file://{}", src.path().display()));
    cx.simulate_keystrokes("enter");
    settle(cx, |ws| adding(ws).is_none(), &ws);
    let cloned = home
        .path()
        .join("Work")
        .join(src.path().file_name().unwrap());
    assert!(cloned.join(".git").is_dir(), "cloned into ~/Work");
    ws.read_with(cx, |ws, _| {
        let (project, _, note) = chosen(ws);
        assert_eq!(project.unwrap().path, cloned);
        assert!(note.unwrap().starts_with("Added "));
    });
    // A plain name starts an empty repository in the same place.
    let (config, home2, ssh2) = (
        config.path().to_path_buf(),
        home.path().to_path_buf(),
        ssh.clone(),
    );
    cx.update(|_, cx| {
        ws.update(cx, |ws, cx| {
            ws.add_project_open_with(config, home2, ssh2.to_string_lossy().into(), cx)
        })
    });
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.exists() == Some(true)),
        &ws,
    );
    cx.simulate_keystrokes("ctrl-n ctrl-a");
    cx.simulate_input("fresh idea");
    cx.simulate_keystrokes("enter");
    settle(cx, |ws| adding(ws).is_none(), &ws);
    assert!(home.path().join("Work/fresh idea/.git").is_dir());
}

/// Esc goes back one layer at a time and never past the composer: the
/// highlight, then the folders, then the machines; Ctrl+O and the ＋ beside
/// PROJECT both open the dialog.
#[gpui::test]
fn escape_walks_back_one_step_and_both_doors_open_the_dialog(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    let (home, config, bin) = (
        Scratch::new("home"),
        Scratch::new("config"),
        Scratch::new("bin"),
    );
    std::fs::create_dir_all(home.path().join("Work/a")).unwrap();
    std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
    std::fs::write(home.path().join(".ssh/config"), "Host lab\n").unwrap();
    let ssh = fake_ssh(bin.path(), home.path());
    open(&ws, cx, vec![here()], "", config.path(), home.path(), &ssh);
    cx.simulate_keystrokes("enter");
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.rows().len() == 2),
        &ws,
    );
    cx.simulate_keystrokes("down");
    redraw(cx);
    let step = |ws: &Workspace| match ws
        .composer
        .as_ref()
        .map(|c| c.adding.as_ref().map(|a| &a.step))
    {
        None => "closed",
        Some(None) => "composer",
        Some(Some(Step::Machines)) => "machines",
        Some(Some(Step::Connecting { .. })) => "connecting",
        Some(Some(Step::Folder(b))) if b.at.is_some() => "highlight",
        Some(Some(Step::Folder(_))) => "folder",
    };
    let mut walked = vec![ws.read_with(cx, |ws, _| step(ws))];
    for _ in 0..4 {
        cx.simulate_keystrokes("escape");
        redraw(cx);
        walked.push(ws.read_with(cx, |ws, _| step(ws)));
    }
    assert_eq!(
        walked,
        ["highlight", "folder", "machines", "composer", "closed"]
    );
    // The two doors, with no far machine to call.
    cx.update(|w, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_composer(w, cx);
            let c = ws.composer.as_mut().unwrap();
            c.machines = vec![here()];
        })
    });
    redraw(cx);
    cx.simulate_keystrokes("ctrl-o");
    redraw(cx);
    assert!(
        ws.read_with(cx, |ws, _| adding(ws).is_some()),
        "Ctrl+O opens it"
    );
    assert!(cx.debug_bounds("add-project").is_some());
    cx.update(|_, cx| ws.update(cx, |ws, _| ws.composer.as_mut().unwrap().adding = None));
    redraw(cx);
    let plus = cx
        .debug_bounds("composer-add-project")
        .expect("the ＋ beside PROJECT is drawn");
    cx.simulate_click(plus.center(), gpui::Modifiers::none());
    redraw(cx);
    assert!(
        ws.read_with(cx, |ws, _| adding(ws).is_some()),
        "the ＋ opens it"
    );
}

/// A plain Enter adds only what is there at the moment of adding: a folder
/// deleted after its listing was read is not made again behind the person's
/// back (Codex's review, round one).
#[gpui::test]
fn a_plain_enter_never_remakes_a_folder_gone_since_its_listing(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    let (home, config, bin) = (
        Scratch::new("home"),
        Scratch::new("config"),
        Scratch::new("bin"),
    );
    std::fs::create_dir_all(home.path().join("Work/beta")).unwrap();
    let ssh = fake_ssh(bin.path(), home.path());
    open(&ws, cx, vec![here()], "", config.path(), home.path(), &ssh);
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.rows().len() == 2),
        &ws,
    );
    cx.simulate_keystrokes("b e t a");
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.exists() == Some(true)),
        &ws,
    );
    std::fs::remove_dir(home.path().join("Work/beta")).unwrap();
    cx.simulate_keystrokes("enter");
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.work.is_none() && b.note.is_some()),
        &ws,
    );
    assert!(!home.path().join("Work/beta").exists(), "not made again");
    ws.read_with(cx, |ws, _| {
        let note = browse(ws).unwrap().note.clone().unwrap();
        assert!(note.contains("does not exist yet"), "{note}");
        assert!(adding(ws).is_some(), "the dialog stays, nothing chosen");
    });
}

/// A long ssh config scrolls: the highlighted machine is always drawn, and
/// what is below the window is counted (Codex's review, round one).
#[gpui::test]
fn a_long_machine_list_keeps_the_highlight_in_view(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    let (home, config, bin) = (
        Scratch::new("home"),
        Scratch::new("config"),
        Scratch::new("bin"),
    );
    std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
    let hosts: String = (1..=12).map(|n| format!("Host box{n}\n")).collect();
    std::fs::write(home.path().join(".ssh/config"), hosts).unwrap();
    let ssh = fake_ssh(bin.path(), home.path());
    open(&ws, cx, vec![here()], "", config.path(), home.path(), &ssh);
    assert!(cx.debug_bounds("add-project-machine-0").is_some());
    assert!(
        cx.debug_bounds("add-project-machine-8").is_none(),
        "eight at a time"
    );
    for _ in 0..12 {
        cx.simulate_keystrokes("down");
    }
    redraw(cx);
    assert!(
        cx.debug_bounds("add-project-machine-12").is_some(),
        "the last machine, highlighted, is drawn"
    );
    assert!(cx.debug_bounds("add-project-machine-0").is_none());
    ws.read_with(cx, |ws, _| assert_eq!(adding(ws).unwrap().at, 12));
}
