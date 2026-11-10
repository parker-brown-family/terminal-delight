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
        let (note, failed) = browse(ws).unwrap().note.clone().unwrap_or_default();
        assert!(note.contains("does not exist yet"), "{note}");
        assert!(!failed, "guidance, not a failure");
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
        let (note, _) = browse(ws).unwrap().note.clone().unwrap();
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

/// One key to an agent with no project: Backspace on the PROJECT chip, with
/// nothing typed, takes the project away; with letters typed it only takes
/// a letter. The none row says where such an agent works.
#[gpui::test]
fn backspace_on_the_project_chip_starts_from_no_project(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    cx.update(|w, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_composer(w, cx);
            let c = ws.composer.as_mut().unwrap();
            c.machines = vec![here()];
            c.projects.insert(
                0,
                launcher::Project {
                    path: PathBuf::from("/srv/somewhere"),
                    name: "somewhere".into(),
                    repo: true,
                    machine: None,
                },
            );
            c.order = launcher::filter(&c.projects, "");
            c.project = Some(0);
            c.focus = entry::Focus::Project;
            c.query = "so".into();
        })
    });
    redraw(cx);
    cx.simulate_keystrokes("backspace");
    ws.read_with(cx, |ws, _| {
        let c = ws.composer.as_ref().unwrap();
        assert_eq!(c.query, "s", "a letter first");
        assert!(c.project.is_some());
    });
    cx.simulate_keystrokes("backspace backspace");
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        let c = ws.composer.as_ref().unwrap();
        assert!(c.query.is_empty());
        assert_eq!(c.project, None, "then the project itself");
    });
    // The list's none row is drawn, and says where the agent will work.
    assert!(cx.debug_bounds("composer-project-none").is_some());
}

/// The project list draws eight rows and keeps the chosen one among them: a
/// list that always opened at its top hid the project the chip named whenever
/// it was the ninth or later. What is below is counted. No project leads
/// whatever the window shows: as the window's own first row it scrolled out
/// of sight, and Parker, looking for T3's "No project", did not find it.
#[gpui::test]
fn the_project_list_keeps_the_chosen_project_in_view(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    cx.update(|w, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_composer(w, cx);
            let c = ws.composer.as_mut().unwrap();
            c.machines = vec![here()];
            c.projects = (0..14)
                .map(|n| launcher::Project {
                    path: PathBuf::from(format!("/srv/p{n:02}")),
                    name: format!("p{n:02}"),
                    repo: true,
                    machine: None,
                })
                .collect();
            c.order = launcher::filter(&c.projects, "");
            c.project = Some(11);
            c.focus = entry::Focus::Project;
        })
    });
    redraw(cx);
    // Row 0 is none, so project 11 is row 12 of 15.
    assert!(
        cx.debug_bounds("composer-project-12").is_some(),
        "the chosen project is drawn"
    );
    assert!(
        cx.debug_bounds("composer-project-none").is_some(),
        "none leads whatever the window shows"
    );
    assert!(
        cx.debug_bounds("composer-project-1").is_none(),
        "the window moved down to it"
    );
    // Back to the top: none and the first seven.
    cx.update(|_, cx| ws.update(cx, |ws, _| ws.composer.as_mut().unwrap().project = None));
    redraw(cx);
    assert!(cx.debug_bounds("composer-project-none").is_some());
    assert!(cx.debug_bounds("composer-project-7").is_some());
    assert!(
        cx.debug_bounds("composer-project-8").is_none(),
        "eight at a time"
    );
}

/// The INBOX face's header ends in the same + the PROJECTS face's does, and
/// it opens the composer.
#[gpui::test]
fn the_inbox_faces_plus_opens_the_composer(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    // A test window is a scratch window, which draws no spine unless asked.
    cx.update(|_, cx| {
        ws.update(cx, |ws, cx| {
            ws.spine_in_scratch = true;
            ws.set_spine_inbox(true, cx);
        })
    });
    redraw(cx);
    let plus = cx
        .debug_bounds("spine-new-chat")
        .expect("the INBOX face draws its +");
    cx.simulate_click(plus.center(), gpui::Modifiers::none());
    redraw(cx);
    assert!(
        ws.read_with(cx, |ws, _| ws.composer.is_some()),
        "the + opens the composer"
    );
}

/// Back from the clone field goes to where it was opened: to the machines
/// when it was opened from them, which the tour found landing on a folder
/// list the person had never seen, and to the folder list when it was opened
/// there.
#[gpui::test]
fn back_from_the_clone_field_returns_to_where_it_was_opened(cx: &mut gpui::TestAppContext) {
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
    let step = |ws: &Workspace| match adding(ws).map(|a| &a.step) {
        Some(Step::Machines) => "machines",
        Some(Step::Folder(b)) if b.new.is_some() => "field",
        Some(Step::Folder(_)) => "folders",
        Some(Step::Connecting { .. }) => "connecting",
        None => "closed",
    };
    assert_eq!(ws.read_with(cx, |ws, _| step(ws)), "machines");
    // From the machines, on this computer; its Back button goes back to them.
    cx.simulate_keystrokes("n");
    settle(cx, |ws| step(ws) == "field", &ws);
    let back = cx
        .debug_bounds("add-project-back")
        .expect("the field's Back is drawn");
    cx.simulate_click(back.center(), gpui::Modifiers::none());
    redraw(cx);
    assert_eq!(ws.read_with(cx, |ws, _| step(ws)), "machines");
    // From a folder, the field closes back onto that folder.
    cx.simulate_keystrokes("enter");
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.exists() == Some(true)),
        &ws,
    );
    cx.simulate_keystrokes("ctrl-n");
    redraw(cx);
    assert_eq!(ws.read_with(cx, |ws, _| step(ws)), "field");
    cx.simulate_keystrokes("escape");
    redraw(cx);
    assert_eq!(ws.read_with(cx, |ws, _| step(ws)), "folders");
}

/// The folder list keeps its height whatever the folder holds. The card is
/// centred, so a short folder moved the path field and every row, and the
/// next click landed on a row nobody was pointing at.
#[gpui::test]
fn a_short_folder_leaves_the_dialog_where_it_was(cx: &mut gpui::TestAppContext) {
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
    for n in 0..12 {
        std::fs::create_dir_all(home.path().join(format!("Work/f{n:02}"))).unwrap();
    }
    std::fs::create_dir_all(home.path().join("Work/f00/only")).unwrap();
    let ssh = fake_ssh(bin.path(), home.path());
    open(&ws, cx, vec![here()], "", config.path(), home.path(), &ssh);
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.rows().len() == 13),
        &ws,
    );
    let field = cx.debug_bounds("add-project-field").expect("the field");
    let card = cx.debug_bounds("add-project").expect("the card");
    cx.update(|_, cx| {
        ws.update(cx, |ws, cx| {
            if let Some(a) = ws.adding() {
                if let Step::Folder(b) = &mut a.step {
                    b.set_field("~/Work/f00/");
                }
            }
            ws.add_project_read(cx);
        })
    });
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.rows().len() == 2),
        &ws,
    );
    assert_eq!(
        cx.debug_bounds("add-project-field"),
        Some(field),
        "the field stays put"
    );
    assert_eq!(
        cx.debug_bounds("add-project"),
        Some(card),
        "and so does the card"
    );
}

/// The composer stays where it is while a filter narrows its project list:
/// the card is centred, and a list that shrank moved the sentence box up the
/// screen with each letter typed.
#[gpui::test]
fn the_composer_stays_put_while_a_filter_narrows_its_list(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    cx.update(|w, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_composer(w, cx);
            let c = ws.composer.as_mut().unwrap();
            c.machines = vec![here()];
            c.projects = (0..14)
                .map(|n| launcher::Project {
                    path: PathBuf::from(format!("/srv/p{n:02}")),
                    name: format!("p{n:02}"),
                    repo: true,
                    machine: None,
                })
                .collect();
            c.order = launcher::filter(&c.projects, "");
            c.project = None;
            c.focus = entry::Focus::Project;
        })
    });
    redraw(cx);
    let open = cx.debug_bounds("composer-text").expect("the sentence box");
    cx.simulate_keystrokes("p 1 3");
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.composer.as_ref().unwrap().order.len(), 1, "one match")
    });
    assert_eq!(cx.debug_bounds("composer-text"), Some(open), "one match");
    cx.simulate_keystrokes("q");
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        assert!(ws.composer.as_ref().unwrap().order.is_empty(), "no match")
    });
    assert_eq!(cx.debug_bounds("composer-text"), Some(open), "no match");
}

/// A failure in the dialog is marked as one, so it is drawn in the skin's
/// danger ink rather than in the accent, which a green design reads as
/// success; guidance (the folder is not there yet) is not.
#[gpui::test]
fn a_failure_in_the_dialog_is_marked_as_one(cx: &mut gpui::TestAppContext) {
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
    std::fs::create_dir_all(home.path().join("Work/taken")).unwrap();
    let ssh = fake_ssh(bin.path(), home.path());
    open(&ws, cx, vec![here()], "", config.path(), home.path(), &ssh);
    settle(
        cx,
        |ws| browse(ws).is_some_and(|b| b.exists() == Some(true)),
        &ws,
    );
    cx.simulate_keystrokes("ctrl-n");
    cx.simulate_input("taken");
    cx.simulate_keystrokes("enter");
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        let (note, failed) = browse(ws).unwrap().note.clone().expect("a note");
        assert!(note.contains("taken"), "{note}");
        assert!(failed, "a name already taken is a failure");
    });
}

/// The composer up on this machine with `n` stand-in projects, p00 to p(n-1),
/// none of them the person's.
fn composer_with_projects(
    ws: &gpui::Entity<Workspace>,
    cx: &mut gpui::VisualTestContext,
    n: usize,
    chosen: Option<usize>,
    focus: entry::Focus,
) {
    cx.update(|w, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_composer(w, cx);
            let c = ws.composer.as_mut().unwrap();
            c.machines = vec![here()];
            c.projects = (0..n)
                .map(|n| launcher::Project {
                    path: PathBuf::from(format!("/srv/p{n:02}")),
                    name: format!("p{n:02}"),
                    repo: true,
                    machine: None,
                })
                .collect();
            c.order = launcher::filter(&c.projects, "");
            c.project = chosen;
            // Not the person's launch defaults: a list's length must not
            // depend on what they last chose.
            c.harness = launcher::Harness::Claude;
            c.model = None;
            c.effort = None;
            c.focus = focus;
        })
    });
    redraw(cx);
}

/// No project answers to its words while the PROJECT chip is filtered, as
/// T3's row answers to "no project" and "none"; and Enter on a chip takes what
/// is lit and goes back to the sentence, as T3's pickers do, rather than
/// starting anything. The sentence is left empty, so a mistaken start could
/// only ever ask for one.
#[gpui::test]
fn typing_no_finds_no_project_and_enter_goes_back_to_the_sentence(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    composer_with_projects(&ws, cx, 14, Some(3), entry::Focus::Project);
    cx.simulate_keystrokes("p 0 9");
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.composer.as_ref().unwrap().project, Some(9), "by name")
    });
    assert!(
        cx.debug_bounds("composer-project-none").is_none(),
        "a name typed is not a request for none"
    );
    cx.simulate_keystrokes("backspace backspace backspace n o");
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        let c = ws.composer.as_ref().unwrap();
        assert_eq!(c.query, "no");
        assert_eq!(c.project, None, "its words take it");
    });
    assert!(cx.debug_bounds("composer-project-none").is_some());
    cx.simulate_keystrokes("enter");
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        let c = ws
            .composer
            .as_ref()
            .expect("Enter on a chip closes nothing");
        assert_eq!(c.focus, entry::Focus::Text, "back to the sentence");
        assert!(c.query.is_empty());
        assert_eq!(c.project, None, "with what was lit");
        assert_eq!(c.note, None, "and no start was asked for");
    });
    assert!(
        cx.debug_bounds("composer-project-none").is_none(),
        "the list is put away"
    );
}

/// Every chip opens a list in the same place, and every list stands as tall
/// as PROJECT's, so the card, which is centred, holds still while Tab walks
/// the chips. A press on the chip that has the keyboard puts its list away;
/// a press on another opens that one.
#[gpui::test]
fn every_chip_opens_its_list_and_the_card_holds_still(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    composer_with_projects(&ws, cx, 3, Some(0), entry::Focus::Project);
    // A far machine, so MACHINE joins the ring.
    cx.update(|_, cx| {
        ws.update(cx, |ws, _| {
            ws.composer.as_mut().unwrap().machines = vec![here(), farbox()]
        })
    });
    redraw(cx);
    let at = cx.debug_bounds("composer-text").expect("the sentence box");
    for chip in ["Harness", "Model", "Effort", "Reach", "Machine"] {
        cx.simulate_keystrokes("tab");
        redraw(cx);
        ws.read_with(cx, |ws, _| {
            let focus = ws.composer.as_ref().unwrap().focus;
            assert_eq!(format!("{focus:?}"), chip)
        });
        assert!(
            cx.debug_bounds("composer-choice-0").is_some(),
            "{chip} lists its options"
        );
        assert_eq!(
            cx.debug_bounds("composer-text"),
            Some(at),
            "the card held still on {chip}"
        );
    }
    // MACHINE lists this machine and the far one.
    assert!(cx.debug_bounds("composer-choice-1").is_some());
    let machine = cx
        .debug_bounds("composer-chip-Machine")
        .expect("the MACHINE chip");
    cx.simulate_click(machine.center(), gpui::Modifiers::none());
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.composer.as_ref().unwrap().focus, entry::Focus::Text)
    });
    assert!(
        cx.debug_bounds("composer-choice-0").is_none(),
        "pressed again, the chip puts its list away"
    );
    let reach = cx
        .debug_bounds("composer-chip-Reach")
        .expect("the REACH chip");
    cx.simulate_click(reach.center(), gpui::Modifiers::none());
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.composer.as_ref().unwrap().focus, entry::Focus::Reach)
    });
    assert!(
        cx.debug_bounds("composer-choice-2").is_some(),
        "this repo, this machine, anywhere"
    );
}

/// MODEL lists every model the harness offers at once; typed letters take
/// the first match. EFFORT then lists that model's own levels, and a press
/// takes one and goes back to the sentence. The catalog is seeded, so the
/// test does not depend on which agents this machine has installed.
#[gpui::test]
fn typed_letters_pick_a_model_and_a_press_takes_an_effort(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    let home = PathBuf::from(format!("/srv/td-pickers-{}", std::process::id()));
    let model = |id: &str, label: &str| model_catalog::Model {
        id: Some(id.into()),
        label: label.into(),
        resolved_id: None,
        efforts: Some(vec![
            "low".into(),
            "medium".into(),
            "high".into(),
            "xhigh".into(),
        ]),
        default_effort: Some("medium".into()),
        caption: model_catalog::Caption::Name,
    };
    model_catalog::seed(
        launcher::Harness::Codex,
        &home,
        vec![
            model("gpt-6-sol", "sol 6"),
            model("gpt-6-astra", "astra 6"),
            model("gpt-6-luna", "luna 6"),
            model("gpt-5.6-terra", "terra 5.6"),
            model("gpt-5.6-sol", "sol 5.6"),
        ],
    );
    let project = home.clone();
    cx.update(|w, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_composer(w, cx);
            let c = ws.composer.as_mut().unwrap();
            c.machines = vec![here()];
            c.projects = vec![launcher::Project {
                path: project,
                name: "pickers".into(),
                repo: true,
                machine: None,
            }];
            c.order = launcher::filter(&c.projects, "");
            c.project = Some(0);
            c.harness = launcher::Harness::Codex;
            c.model = None;
            c.effort = None;
            c.focus = entry::Focus::Model;
        })
    });
    redraw(cx);
    assert!(
        cx.debug_bounds("composer-choice-5").is_some(),
        "the harness's default and its five models"
    );
    assert!(cx.debug_bounds("composer-choice-6").is_none());
    cx.simulate_keystrokes("l u n");
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.composer.as_ref().unwrap().model.as_deref(),
            Some("gpt-6-luna")
        )
    });
    assert!(
        cx.debug_bounds("composer-choice-1").is_none(),
        "only luna is left"
    );
    cx.simulate_keystrokes("enter");
    redraw(cx);
    let effort = cx
        .debug_bounds("composer-chip-Effort")
        .expect("the EFFORT chip");
    cx.simulate_click(effort.center(), gpui::Modifiers::none());
    redraw(cx);
    assert!(
        cx.debug_bounds("composer-choice-4").is_some(),
        "the default and luna's four levels"
    );
    assert!(cx.debug_bounds("composer-choice-5").is_none());
    let high = cx.debug_bounds("composer-choice-3").expect("high");
    cx.simulate_click(high.center(), gpui::Modifiers::none());
    redraw(cx);
    ws.read_with(cx, |ws, _| {
        let c = ws.composer.as_ref().unwrap();
        assert_eq!(c.effort.as_deref(), Some("high"));
        assert_eq!(c.model.as_deref(), Some("gpt-6-luna"), "the model stays");
        assert_eq!(c.focus, entry::Focus::Text, "a press goes back");
    });
}

/// A project name long enough to wrap the chips onto a second line, and a
/// short one that does not, leave the card where it was while the list is
/// open: the chips stand two lines tall there. Before, each step of the
/// arrows between the two moved the centred card a line (the sweep caught it
/// in the hacker design with a far machine listed). The chips keep their own
/// height in the taller row: stretched to it, a lit chip's bar ran two lines
/// (the next sweep caught that).
#[gpui::test]
fn the_card_holds_still_while_the_chips_wrap_and_unwrap(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    composer_with_projects(&ws, cx, 2, Some(0), entry::Focus::Text);
    let chip = |cx: &mut gpui::VisualTestContext| {
        cx.debug_bounds("composer-chip-Harness")
            .expect("HARNESS")
            .size
            .height
    };
    let at_rest = chip(cx);
    cx.update(|_, cx| {
        ws.update(cx, |ws, _| {
            ws.composer.as_mut().unwrap().focus = entry::Focus::Project
        })
    });
    redraw(cx);
    assert_eq!(
        chip(cx),
        at_rest,
        "a chip keeps its height in the taller row"
    );
    cx.update(|_, cx| {
        ws.update(cx, |ws, _| {
            ws.composer.as_mut().unwrap().projects[1].name =
                "a-project-whose-name-runs-long-enough-to-wrap-the-row-of-chips-onto-a-second-line"
                    .into()
        })
    });
    redraw(cx);
    let wrapped = |cx: &mut gpui::VisualTestContext| {
        let first = cx.debug_bounds("composer-chip-Project").expect("PROJECT");
        let last = cx.debug_bounds("composer-chip-Reach").expect("REACH");
        last.origin.y > first.origin.y
    };
    assert!(!wrapped(cx), "a short name keeps the chips on one line");
    let at = cx.debug_bounds("composer-text").expect("the sentence box");
    cx.update(|_, cx| ws.update(cx, |ws, _| ws.composer.as_mut().unwrap().project = Some(1)));
    redraw(cx);
    assert!(wrapped(cx), "the long one wraps them");
    assert_eq!(
        cx.debug_bounds("composer-text"),
        Some(at),
        "and the card held still"
    );
}

/// A long paste into the composer goes in as one token, not as its text;
/// Backspace takes the token whole; a second paste gets a token of its own;
/// and the draft put away keeps what each token stands for. Parker,
/// 2026-11-10: *"if i paste a giant block - it gets fully printed out"*.
///
/// Mutation-tested: inserting the text instead of the token fails "the box
/// holds the token"; skipping `delete_back_run` fails "one press takes it".
#[gpui::test]
fn a_long_paste_stands_in_the_composer_as_one_token(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    cx.update(|w, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_composer(w, cx);
            let c = ws.composer.as_mut().unwrap();
            c.machines = vec![here()];
            c.text = EditBuffer::at_end("do this: ");
        })
    });
    redraw(cx);
    let plan: String = (1..=40)
        .map(|i| format!("{i}. a step of the plan\n"))
        .collect();
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(plan.clone())));
    cx.simulate_keystrokes("ctrl-v");
    redraw(cx);
    let text = |cx: &mut gpui::VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.composer.as_ref().unwrap().text.text())
    };
    assert_eq!(
        text(cx),
        "do this: [pasted text #1 · 40 lines]",
        "the box holds the token"
    );
    let unfolded = ws.read_with(cx, |ws, _| {
        let c = ws.composer.as_ref().unwrap();
        entry::unfold(&c.text.text(), &c.pastes)
    });
    assert_eq!(
        unfolded,
        format!("do this: {plan}"),
        "the send has the text"
    );
    // A short paste is typed text, as before.
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(" now".into())));
    cx.simulate_keystrokes("ctrl-v");
    redraw(cx);
    assert!(text(cx).ends_with(" · 40 lines] now"));
    cx.simulate_keystrokes("backspace backspace backspace backspace");
    redraw(cx);
    cx.simulate_keystrokes("backspace");
    redraw(cx);
    assert_eq!(text(cx), "do this: ", "one press takes it");
    assert!(ws.read_with(cx, |ws, _| ws.composer.as_ref().unwrap().pastes.is_empty()));
    // Pasted again: a token of its own, kept with the draft put away.
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(plan.clone())));
    cx.simulate_keystrokes("ctrl-v");
    redraw(cx);
    ws.update(cx, |ws, _| ws.composer_put_away());
    let kept = ws.read_with(cx, |ws, _| {
        ws.composer_stash
            .as_ref()
            .map(|(t, _, p)| entry::unfold(t, p))
    });
    assert_eq!(kept, Some(format!("do this: {plan}")));
}
