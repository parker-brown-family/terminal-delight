use super::*;

/// A folder of the test's own, gone when the test is.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "td-addproject-{tag}-{}-{}",
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

/// Parker's own `~/.ssh/config`, shape for shape: four git remotes, a machine
/// with two aliases, and one more machine.
const PARKERS: &str = "\
Host github.com
  IdentityFile ~/.ssh/id_bfs
Host github-bfs
  HostName github.com
  User git
  IdentityFile ~/.ssh/id_bfs
Host github-intellimass
  HostName github.com
Host github-nord
  HostName=github.com
Host omarchy-bfs bfs
  HostName 192.168.1.40
  User parker
Host onn
  HostName onn.lan
";

fn no_includes(_: &str) -> Vec<String> {
    Vec::new()
}

#[test]
fn the_git_remotes_in_an_ssh_config_are_counted_and_the_machines_offered() {
    let hosts = parse_ssh_config(PARKERS, &mut no_includes);
    let aliases: Vec<&str> = hosts.hosts.iter().map(SshHost::alias).collect();
    assert_eq!(aliases, ["omarchy-bfs", "onn"]);
    assert_eq!(hosts.git_hosts, 4, "github.com and its three aliases");
    let bfs = &hosts.hosts[0];
    assert_eq!(bfs.aliases, ["omarchy-bfs", "bfs"]);
    assert_eq!(bfs.hostname.as_deref(), Some("192.168.1.40"));
    assert_eq!(bfs.user.as_deref(), Some("parker"));
}

#[test]
fn patterns_match_blocks_and_repeats_name_no_machine() {
    let text = "\
Host *
  ServerAliveInterval 30
Host *.lan !secret
  User me
Host box \"quoted box\" bad,comma
  HostName box.lan
Match host box
  User nobody
Host box
  User second
Host -oProxyCommand=evil
";
    let hosts = parse_ssh_config(text, &mut no_includes);
    assert_eq!(hosts.hosts.len(), 1, "{:?}", hosts.hosts);
    let b = &hosts.hosts[0];
    // A quoted alias with a space is not a destination this window will
    // type; a pattern list is not one machine; an option is not a host.
    assert_eq!(b.aliases, ["box"]);
    assert_eq!(b.hostname.as_deref(), Some("box.lan"));
    // The Match block's User belongs to no alias, and the second `Host box`
    // block is ssh's second choice, not its first.
    assert_eq!(b.user, None);
    assert_eq!(hosts.git_hosts, 0);
}

#[test]
fn an_include_is_followed_where_it_stands_and_a_loop_ends() {
    let mut asked: Vec<String> = Vec::new();
    let mut read = |pattern: &str| {
        asked.push(pattern.to_string());
        match pattern {
            "config.d/*" => vec!["Host lab\n  HostName lab.lan\nInclude loop\n".to_string()],
            "loop" => vec!["Include loop\nHost deep\n".to_string()],
            _ => Vec::new(),
        }
    };
    let hosts = parse_ssh_config("Include config.d/*\nHost after\n", &mut read);
    let aliases: Vec<&str> = hosts.hosts.iter().map(SshHost::alias).collect();
    assert_eq!(aliases, ["lab", "deep", "after"]);
    assert!(
        asked.len() < 20,
        "the include loop stopped: {}",
        asked.len()
    );
}

#[test]
fn include_files_read_from_dot_ssh_and_match_a_star() {
    let home = Scratch::new("t1");
    let d = home.path().join(".ssh/config.d");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("a.conf"), "Host a\n").unwrap();
    std::fs::write(d.join("b.conf"), "Host b\n").unwrap();
    std::fs::write(d.join("notes.txt"), "Host c\n").unwrap();
    std::fs::write(
        home.path().join(".ssh/config"),
        "Include config.d/*.conf\nHost github-x\n  User git\n",
    )
    .unwrap();
    let hosts = read_ssh_hosts(home.path()).unwrap();
    let aliases: Vec<&str> = hosts.hosts.iter().map(SshHost::alias).collect();
    assert_eq!(aliases, ["a", "b"]);
    assert_eq!(hosts.git_hosts, 1);
    // No config at all is no hosts, and says nothing is wrong.
    let empty = Scratch::new("t2");
    assert_eq!(read_ssh_hosts(empty.path()), Ok(SshHosts::default()));
}

#[test]
fn a_host_already_in_machines_toml_is_not_offered_again() {
    let hosts = parse_ssh_config(PARKERS, &mut no_includes).hosts;
    let machines =
        entry::parse_machines("[[machine]]\nname = \"farbox\"\nssh = \"omarchy-bfs\"\n").unwrap();
    let left: Vec<String> = hosts_not_added(&hosts, &machines)
        .iter()
        .map(|h| h.alias().to_string())
        .collect();
    assert_eq!(left, ["onn"]);
    // Through its second alias, and with a user in front, it is the same host.
    let by_alias = Machine {
        name: "x".into(),
        ssh: Some("me@bfs".into()),
    };
    assert!(hosts[0].is_machine(&by_alias));
    assert!(!hosts[1].is_machine(&by_alias));
}

#[test]
fn every_ssh_call_is_batch_mode_and_ends_its_options_before_the_host() {
    let args = ssh_args("omarchy-bfs", Some(Path::new("/run/user/1000")));
    let at = args.iter().position(|a| a == "--").unwrap();
    assert_eq!(&args[at + 1..], ["omarchy-bfs"]);
    let opts = args[..at].join(" ");
    assert!(opts.contains("BatchMode=yes"));
    assert!(opts.contains("ControlPath=/run/user/1000/td-ssh-%C"));
    assert!(
        !opts.contains("StrictHostKeyChecking"),
        "no key is accepted for anybody"
    );
    assert!(!ssh_args("h", None).join(" ").contains("Control"));
}

#[test]
fn ssh_trouble_names_an_unknown_key_and_a_changed_one_apart() {
    assert_eq!(
        ssh_trouble("No ED25519 host key is known for onn and you have requested strict checking.\r\nHost key verification failed.\r\n"),
        SshTrouble::KeyUnknown
    );
    assert!(matches!(
        ssh_trouble("@@@@\n@    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @\n@@@@\nHost key verification failed.\n"),
        SshTrouble::KeyChanged(_)
    ));
    assert_eq!(
        ssh_trouble("Warning: Permanently added 'x' (ED25519) to the list of known hosts.\nme@x: Permission denied (publickey).\n"),
        SshTrouble::Other("me@x: Permission denied (publickey).".into())
    );
    assert_eq!(
        ssh_trouble("ssh: connect to host onn.lan port 22: Connection timed out\n"),
        SshTrouble::Other("ssh: connect to host onn.lan port 22: Connection timed out".into())
    );
}

#[test]
fn the_path_field_splits_into_the_folder_listed_and_the_name_typed() {
    assert_eq!(split_field("~/Work/td-"), ("~/Work/".into(), "td-".into()));
    assert_eq!(split_field("~/Work/"), ("~/Work/".into(), String::new()));
    assert_eq!(split_field("~"), ("~/".into(), String::new()));
    assert_eq!(split_field("/"), ("/".into(), String::new()));
    assert_eq!(split_field("/srv"), ("/".into(), "srv".into()));
    // Neither `/` nor `~` in front: read from home.
    assert_eq!(split_field("Work/t"), ("~/Work/".into(), "t".into()));
    assert_eq!(field_target("~/Work/td-"), "~/Work/td-");
    assert_eq!(field_target("~/Work/"), "~/Work");
    assert_eq!(field_target("~/"), "~");
    assert_eq!(field_target("/"), "/");
    let home = Path::new("/home/p");
    assert_eq!(expand_home("~/Work", home), Path::new("/home/p/Work"));
    assert_eq!(expand_home("~", home), Path::new("/home/p"));
    assert_eq!(expand_home("/srv/x", home), Path::new("/srv/x"));
    assert_eq!(parent_field("/home/p/Work"), Some("/home/p/".into()));
    assert_eq!(parent_field("/home/"), Some("/".into()));
    assert_eq!(parent_field("/"), None);
}

#[test]
fn a_listing_reads_back_with_checkouts_marked_and_dotted_folders_last() {
    let out = "at /home/mia/TF\nd .state\ng repo\nd docs\nd Zed\nbogus\ng .hidden-repo\n";
    let Listing::At { path, folders } = parse_listing(out).unwrap() else {
        panic!("a listing");
    };
    assert_eq!(path, "/home/mia/TF");
    let names: Vec<(&str, bool)> = folders.iter().map(|f| (f.name.as_str(), f.git)).collect();
    assert_eq!(
        names,
        [
            ("docs", false),
            ("repo", true),
            ("Zed", false),
            (".hidden-repo", true),
            (".state", false)
        ]
    );
    assert_eq!(parse_listing("missing\n"), Ok(Listing::Missing));
    assert!(parse_listing("sh: 1: cd: can't cd\n").is_err());
    let m = matching(&folders, "D");
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].name, "docs");
}

/// Run a far script the way a far machine would: its login shell handed the
/// one command string ssh carries. Spaces, quotes and a `$` in a folder's
/// name are what the quoting has to survive.
fn run_far(script: &str, args: &[&str], home: &Path) -> (bool, String, String) {
    let _fork = crate::testsync::forks_and_locks();
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(remote_command(script, args))
        .env("HOME", home)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn the_far_listing_script_reads_a_real_folder_through_a_shell_line() {
    let home = Scratch::new("t3");
    let work = home.path().join("Work");
    std::fs::create_dir_all(work.join("it's a $HOME folder")).unwrap();
    std::fs::create_dir_all(work.join("checkout/.git")).unwrap();
    std::fs::create_dir_all(work.join(".cache")).unwrap();
    std::fs::write(work.join("a file"), "").unwrap();
    let (ok, out, err) = run_far(&list_script(), &["~/Work/"], home.path());
    assert!(ok, "{err}");
    let listing = parse_listing(&out).unwrap();
    assert_eq!(
        listing,
        list_local(&work).unwrap(),
        "the far script and this machine's own read agree"
    );
    let Listing::At { path, folders } = listing else {
        panic!("a listing")
    };
    assert_eq!(Path::new(&path), work);
    let names: Vec<(&str, bool)> = folders.iter().map(|f| (f.name.as_str(), f.git)).collect();
    assert_eq!(
        names,
        [
            ("checkout", true),
            ("it's a $HOME folder", false),
            (".cache", false)
        ]
    );
    let (ok, out, _) = run_far(&list_script(), &["~/Nope/"], home.path());
    assert!(ok);
    assert_eq!(parse_listing(&out), Ok(Listing::Missing));
    // A file is not a folder to list: the script fails, and says why.
    let (ok, _, err) = run_far(&list_script(), &["~/Work/a file"], home.path());
    assert!(!ok);
    assert!(!err.trim().is_empty());
}

#[test]
fn the_far_scripts_make_a_folder_start_a_repository_and_clone_one() {
    let home = Scratch::new("t4");
    // Make, twice: the second finds it there.
    for _ in 0..2 {
        let (ok, out, err) = run_far(&make_script(), &["~/Work/new one/deep"], home.path());
        assert!(ok, "{err}");
        assert_eq!(
            out.lines().next(),
            Some(format!("at {}", home.path().join("Work/new one/deep").display()).as_str())
        );
    }
    // Start empty: a repository, and a folder holding anything is refused.
    let (ok, out, err) = run_far(&init_script(), &["~/Work/fresh"], home.path());
    assert!(ok, "{err}");
    assert!(out.ends_with("git\n"));
    assert!(home.path().join("Work/fresh/.git").is_dir());
    std::fs::write(home.path().join("Work/new one/deep/x"), "").unwrap();
    let (ok, _, err) = run_far(&init_script(), &["~/Work/new one/deep"], home.path());
    assert!(!ok);
    assert!(err.contains("not empty"), "{err}");
    // Clone: from a repository on disk, which git clones like any URL.
    let src = home.path().join("Work/fresh").to_string_lossy().to_string();
    let (ok, out, err) = run_far(&clone_script(), &[src.as_str(), "~/Work/copy"], home.path());
    assert!(ok, "{err}");
    assert!(out.starts_with(&format!("at {}", home.path().join("Work/copy").display())));
    assert!(home.path().join("Work/copy/.git").is_dir());
    // A clone onto a folder that is there fails, and git says so.
    let (ok, _, err) = run_far(&clone_script(), &[src.as_str(), "~/Work/copy"], home.path());
    assert!(!ok);
    assert!(err.contains("already exists"), "{err}");
}

#[test]
fn the_clone_field_reads_urls_shorthand_and_names() {
    let clone = |url: &str, name: &str| {
        Ok(NewProject::Clone {
            url: url.into(),
            name: name.into(),
        })
    };
    assert_eq!(
        new_project("https://github.com/tcballard/build-omarchy-plugins"),
        clone(
            "https://github.com/tcballard/build-omarchy-plugins",
            "build-omarchy-plugins"
        )
    );
    // A page inside a GitHub repository clones the repository.
    assert_eq!(
        new_project("https://github.com/MiaAI-Lab/t3code/tree/main/apps?tab=x#y"),
        clone("https://github.com/MiaAI-Lab/t3code", "t3code")
    );
    assert_eq!(
        new_project("git@github-bfs:parker-brown-family/terminal-delight.git"),
        clone(
            "git@github-bfs:parker-brown-family/terminal-delight.git",
            "terminal-delight"
        )
    );
    assert_eq!(
        new_project("ssh://git@gitlab.com:2222/group/sub/proj.git/"),
        clone("ssh://git@gitlab.com:2222/group/sub/proj.git/", "proj")
    );
    assert_eq!(
        new_project("owner/repo.git"),
        clone("https://github.com/owner/repo", "repo")
    );
    assert_eq!(
        new_project("  my-thing "),
        Ok(NewProject::Empty {
            name: "my-thing".into()
        })
    );
    assert_eq!(
        new_project("two words"),
        Ok(NewProject::Empty {
            name: "two words".into()
        })
    );
    assert_eq!(new_project("   "), Err(NewTrouble::Empty));
    for bad in [
        ".hidden",
        "-rf",
        "a/b/c",
        "https://github.com/onlyowner",
        "ftp://x/y",
        "javascript://alert",
    ] {
        assert_eq!(new_project(bad), Err(NewTrouble::BadName), "{bad}");
    }
}

#[test]
fn a_repository_in_the_sentence_is_offered_and_a_plain_link_is_not() {
    // Parker's sentence, from the screenshot he pasted.
    let sentence = "https://github.com/tcballard/build-omarchy-plugins\n\nWe want to absolutely hammer this with insighgt and burn tokens thinking about if it can be improved!";
    assert_eq!(
        first_repository(sentence).as_deref(),
        Some("https://github.com/tcballard/build-omarchy-plugins")
    );
    assert_eq!(
        first_repository("see (git@github-bfs:o/r.git), thanks").as_deref(),
        Some("git@github-bfs:o/r.git")
    );
    assert_eq!(
        first_repository("read https://x.com/MiaAI_lab/status/2119208270304883032 first"),
        None
    );
    assert_eq!(first_repository("https://github.com/ alone"), None);
    assert_eq!(first_repository("nothing here: at all"), None);
}

#[test]
fn projects_toml_remembers_a_project_once_and_keeps_what_was_written() {
    let config = Scratch::new("t5");
    assert_eq!(load_projects(config.path()), Ok(Vec::new()));
    let far = Added {
        path: "/home/mia/TF/it's \"quoted\"".into(),
        machine: Some("farbox".into()),
        git: Some(true),
    };
    let here = Added {
        path: "/home/p/deep/down/proj".into(),
        machine: None,
        git: None,
    };
    assert_eq!(remember_project(config.path(), &far), Ok(true));
    assert_eq!(remember_project(config.path(), &here), Ok(true));
    assert_eq!(
        remember_project(config.path(), &far),
        Ok(false),
        "not twice"
    );
    let text = std::fs::read_to_string(projects_path(config.path())).unwrap();
    assert!(text.starts_with("# Projects added from the composer"));
    assert_eq!(
        load_projects(config.path()),
        Ok(vec![far.clone(), here.clone()])
    );
    // A comment the person wrote survives the next append.
    std::fs::write(projects_path(config.path()), format!("{text}# mine\n")).unwrap();
    let third = Added {
        path: "/srv/x".into(),
        machine: None,
        git: Some(false),
    };
    assert_eq!(remember_project(config.path(), &third), Ok(true));
    let text = std::fs::read_to_string(projects_path(config.path())).unwrap();
    assert!(text.contains("# mine\n"));
    assert_eq!(load_projects(config.path()).unwrap().len(), 3);
    assert!(parse_projects("[[project]]\npath = \"relative\"\n").is_err());
    assert!(parse_projects("[[project]]\npath = \"/x\"\nport = 1\n").is_err());
}

#[test]
fn a_new_machine_is_appended_to_machines_toml_and_read_back() {
    let config = Scratch::new("t6");
    let m = add_machine(config.path(), "onn").unwrap();
    assert_eq!(m.name, "onn");
    assert_eq!(m.ssh.as_deref(), Some("onn"));
    // Twice is once.
    add_machine(config.path(), "onn").unwrap();
    let text = std::fs::read_to_string(entry::machines_path(config.path())).unwrap();
    assert!(text.starts_with("# Machines the composer can start an agent on"));
    assert_eq!(entry::load_machines(config.path()).unwrap(), vec![m]);
    // Beside a file somebody wrote by hand, under a name already taken by
    // another destination, it refuses rather than doubling the name.
    std::fs::write(
        entry::machines_path(config.path()),
        "[[machine]]\nname = \"box\"\nssh = \"elsewhere\"\n",
    )
    .unwrap();
    assert!(add_machine(config.path(), "box").is_err());
    assert!(add_machine(config.path(), "-oProxyCommand=x").is_err());
    let m2 = add_machine(config.path(), "lab").unwrap();
    assert_eq!(
        entry::load_machines(config.path()).unwrap(),
        vec![
            Machine {
                name: "box".into(),
                ssh: Some("elsewhere".into())
            },
            m2
        ]
    );
}

#[test]
fn added_projects_lead_the_list_and_a_lost_machine_is_named() {
    let home = Scratch::new("t7");
    let deep = home.path().join("deep/proj");
    std::fs::create_dir_all(deep.join(".git")).unwrap();
    let scanned_dir = home.path().join("Work/scanned");
    std::fs::create_dir_all(&scanned_dir).unwrap();
    let scanned = vec![launcher::Project {
        path: scanned_dir.clone(),
        name: "scanned".into(),
        repo: false,
        machine: None,
    }];
    let machines = vec![
        Machine::here(Some("legion"), "this machine"),
        Machine {
            name: "farbox".into(),
            ssh: Some("omarchy-bfs".into()),
        },
    ];
    let added = vec![
        Added {
            path: deep.to_string_lossy().into(),
            machine: None,
            git: None,
        },
        Added {
            path: "/gone/folder".into(),
            machine: None,
            git: None,
        },
        Added {
            path: scanned_dir.to_string_lossy().into(),
            machine: None,
            git: None,
        },
        Added {
            path: "/home/parker/TF/x".into(),
            machine: Some("farbox".into()),
            git: Some(true),
        },
        Added {
            path: "/srv/y".into(),
            machine: Some("oldbox".into()),
            git: None,
        },
    ];
    let (projects, lost) = with_added(scanned, &added, &machines, home.path());
    let rows: Vec<(&str, Option<&str>, bool)> = projects
        .iter()
        .map(|p| (p.name.as_str(), p.machine.as_deref(), p.repo))
        .collect();
    assert_eq!(
        rows,
        [
            ("x", Some("farbox"), true),
            ("proj", None, true),
            ("scanned", None, false)
        ],
        "newest added first, a folder gone from here dropped, the scan's own kept once"
    );
    assert_eq!(lost, ["oldbox"]);
}

#[test]
fn the_scan_sees_one_level_inside_a_root_and_nothing_hidden() {
    let roots = launcher::default_roots(Path::new("/home/p"));
    assert!(scan_sees(Path::new("/home/p/Work/td"), &roots));
    assert!(!scan_sees(Path::new("/home/p/Work/td/app"), &roots));
    assert!(!scan_sees(Path::new("/home/p/Work/.hidden"), &roots));
    assert!(!scan_sees(Path::new("/srv/x"), &roots));
}

#[test]
fn folder_names_refuse_what_would_hide_or_read_as_an_option() {
    for ok in ["proj", "My Project", "a.b_c-d", "épée"] {
        assert!(folder_name_ok(ok), "{ok}");
    }
    for bad in [
        "",
        ".x",
        "-x",
        "a/b",
        " pad",
        "tab\there",
        "..",
        &"x".repeat(101),
    ] {
        assert!(!folder_name_ok(bad), "{bad:?}");
    }
}
