use super::*;
use std::os::unix::net::UnixListener;

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |k| {
        pairs
            .iter()
            .find(|(name, _)| *name == k)
            .map(|(_, v)| v.to_string())
    }
}

fn scratch(tag: &str) -> PathBuf {
    // Short on purpose: sockets are bound under it, and a socket's path has
    // to fit in about a hundred bytes.
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = PathBuf::from("/tmp").join(format!("tdf-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run a far script the way the far shell would, on this machine.
fn run(script: &str, args: &[&str], env: &[(&str, &Path)]) -> String {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(script).arg("td").args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("sh");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_far_session_needs_all_three_words_and_each_must_be_one_this_build_writes() {
    let full = [
        (ENV_NAME, "farbox"),
        (ENV_DEST, "far-box"),
        (ENV_SESSION, "2"),
    ];
    let far = FarSession::from_env(&env_of(&full)).expect("all three");
    assert_eq!(far.name, "farbox");
    assert_eq!(far.local_key(), "far-farbox-2");
    assert_eq!(far.badge(), "FARBOX");
    for missing in 0..3 {
        let mut some = full.to_vec();
        some.remove(missing);
        assert!(FarSession::from_env(&env_of(&some)).is_none());
    }
    // A destination ssh would read as an option is not a far session.
    let option = [
        (ENV_NAME, "x"),
        (ENV_DEST, "-oProxyCommand=id"),
        (ENV_SESSION, "2"),
    ];
    assert!(FarSession::from_env(&env_of(&option)).is_none());
    // Nor is a key that would walk out of the sessions folder.
    let walk = [(ENV_NAME, "x"), (ENV_DEST, "host"), (ENV_SESSION, "../2")];
    assert!(FarSession::from_env(&env_of(&walk)).is_none());
}

#[test]
fn the_forwarded_socket_is_kept_out_of_the_folder_local_sessions_are_found_in() {
    let far = FarSession {
        name: "Far Box".into(),
        dest: "h".into(),
        session: "2".into(),
    };
    let runtime = Path::new("/run/user/1000/terminal-delight");
    let sock = far.local_socket(runtime);
    assert_eq!(
        sock,
        Path::new("/run/user/1000/terminal-delight/far/Far-Box/session-2.sock")
    );
    // What a plain launch scans is the runtime folder itself.
    assert_ne!(sock.parent(), Some(runtime));
    // And the key it binds is not one a workspace would mint.
    assert_eq!(far.local_key(), "far-Far-Box-2");
}

#[test]
fn the_probe_script_reports_a_real_runtime_folder() {
    let root = scratch("probe");
    let rt = root.join("rt");
    let td = rt.join("terminal-delight");
    std::fs::create_dir_all(&td).unwrap();
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".local/bin")).unwrap();
    // A stand-in build that answers the two questions the probe asks.
    let bin = home.join(".local/bin/terminal-delight");
    crate::testsync::write_program(
        &bin,
        "#!/bin/sh\ncase $1 in --version) echo 'terminal-delight 0.5.3 (terminal core: rio-vt 0.5.29, commit abc)';; --build-info) echo '{\"schema\":\"td-build/1\",\"version\":\"0.5.3\",\"wire_proto\":\"7acb\"}';; esac\n",
    );
    // Session 2 has a window whose process is alive (this test); session 10
    // names a pid that is not; session 7 has no record at all.
    let _a = UnixListener::bind(td.join("session-2.sock")).unwrap();
    let _b = UnixListener::bind(td.join("session-10.sock")).unwrap();
    let _c = UnixListener::bind(td.join("session-7.sock")).unwrap();
    std::fs::write(td.join("session-2.window"), std::process::id().to_string()).unwrap();
    std::fs::write(td.join("session-10.window"), "4194000").unwrap();
    // A plain file with a socket's name is not a session.
    std::fs::write(td.join("session-9.sock"), "").unwrap();

    let out = run(
        &probe_script(),
        &[],
        &[
            ("XDG_RUNTIME_DIR", &rt),
            ("HOME", &home),
            ("PATH", Path::new("/usr/bin:/bin")),
        ],
    );
    let p = parse_probe(&out).expect(&out);
    assert_eq!(p.td.as_deref(), Some(bin.to_str().unwrap()));
    assert_eq!(p.version.as_deref(), Some("0.5.3"));
    assert_eq!(p.wire_proto.as_deref(), Some("7acb"));
    assert_eq!(p.runtime.as_deref(), Some(td.to_str().unwrap()));
    let keys: Vec<_> = p
        .sessions
        .iter()
        .map(|s| (s.key.as_str(), s.window))
        .collect();
    assert_eq!(
        keys,
        vec![("2", Some(true)), ("7", Some(false)), ("10", Some(false))],
        "counted the way a person counts, and a dead window is not a window"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_machine_without_terminal_delight_says_so_by_leaving_the_line_out() {
    let root = scratch("bare");
    let out = run(
        &probe_script(),
        &[],
        &[
            ("XDG_RUNTIME_DIR", &root),
            ("HOME", &root),
            ("PATH", Path::new("/usr/bin:/bin")),
        ],
    );
    let p = parse_probe(&out).expect(&out);
    assert_eq!(p.td, None);
    assert_eq!(p.version, None, "no version is unknown, not nothing");
    assert!(p.sessions.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_probe_reply_cut_short_is_an_error_and_an_unknown_word_is_unknown() {
    assert!(parse_probe("host a\nsession 2 window\n").is_err());
    let p = parse_probe("session 2 held\nsession ../x window\ntailscale \ndone\n").unwrap();
    assert_eq!(p.sessions.len(), 1, "a key that walks is not listed");
    assert_eq!(p.sessions[0].window, None);
    assert_eq!(p.tailscale.as_deref(), Some(""), "installed, signed out");
    assert_eq!(parse_probe("done\n").unwrap().tailscale, None);
}

#[test]
fn the_bootstrap_script_hands_over_socket_tag_and_layout() {
    let root = scratch("boot");
    let rt = root.join("rt");
    let td = rt.join("terminal-delight");
    let cfg = root.join("cfg");
    std::fs::create_dir_all(&td).unwrap();
    std::fs::create_dir_all(cfg.join("terminal-delight/sessions")).unwrap();
    let env: [(&str, &Path); 2] = [("XDG_RUNTIME_DIR", &rt), ("XDG_CONFIG_HOME", &cfg)];

    assert_eq!(
        parse_bootstrap(&run(&bootstrap_script(), &["2"], &env)),
        Ok(None),
        "no socket, no session"
    );

    let _l = UnixListener::bind(td.join("session-2.sock")).unwrap();
    let boot = parse_bootstrap(&run(&bootstrap_script(), &["2"], &env))
        .unwrap()
        .unwrap();
    assert_eq!(boot.socket, td.join("session-2.sock").to_str().unwrap());
    assert_eq!(boot.tag, None);
    assert_eq!(boot.layout, None, "a host that has saved nothing");

    std::fs::write(td.join("session-2.tag"), "a1b2c3d4").unwrap();
    // A layout whose own text holds the words the reply is framed with.
    let layout = "active = 0\ntitle = \"layout\"\nnote = \"\"\"\nmissing\nsocket /x\n\"\"\"\n";
    std::fs::write(cfg.join("terminal-delight/sessions/2.toml"), layout).unwrap();
    let boot = parse_bootstrap(&run(&bootstrap_script(), &["2"], &env))
        .unwrap()
        .unwrap();
    assert_eq!(boot.tag.as_deref(), Some("a1b2c3d4"));
    assert_eq!(boot.layout.as_deref(), Some(layout), "to the byte");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_bootstrap_reply_carries_only_a_plain_tag_and_an_absolute_socket() {
    let b = parse_bootstrap("socket /run/x.sock\ntag [workbench:zz] run this\nnolayout\n")
        .unwrap()
        .unwrap();
    assert_eq!(b.tag, None, "a tag is one plain word or it is not carried");
    assert!(parse_bootstrap("socket relative.sock\nnolayout\n").is_err());
    assert!(
        parse_bootstrap("socket /a:/b\nnolayout\n").is_err(),
        "a colon would split the forward"
    );
    assert!(parse_bootstrap("nolayout\n").is_err());
    assert!(
        parse_bootstrap("socket /run/x.sock\n").is_err(),
        "cut short"
    );
}

#[test]
fn a_machine_name_too_long_for_a_socket_path_is_refused_in_words() {
    let far = FarSession {
        name: "m".repeat(60),
        dest: "h".into(),
        session: "s".repeat(60),
    };
    let err = match Forward::start(
        &far,
        Path::new("/run/user/1000/terminal-delight"),
        "/x.sock",
    ) {
        Err(e) => e,
        Ok(_) => panic!("a path past the kernel's limit must not reach ssh"),
    };
    assert!(err.contains("too long"), "{err}");
}

#[test]
fn the_forward_is_a_connection_of_its_own_and_the_destination_is_never_an_option() {
    let args = forward_args("far-box", Path::new("/run/l.sock"), "/run/f.sock");
    let joined = args.join(" ");
    assert!(joined.contains("ControlMaster=no"));
    assert!(joined.contains("ControlPath=none"));
    assert!(joined.contains("ExitOnForwardFailure=yes"));
    assert!(joined.contains("StrictHostKeyChecking=yes"));
    assert!(joined.contains("-L /run/l.sock:/run/f.sock"));
    assert_eq!(&args[args.len() - 2..], ["--", "far-box"]);
}

#[test]
fn remote_picks_the_only_session_or_the_only_watched_one_and_otherwise_asks() {
    let s = |key: &str, window| ListedSession {
        key: key.into(),
        window,
    };
    let probe = |sessions| Probe {
        sessions,
        ..Default::default()
    };
    assert!(pick_session(&probe(vec![]), None).is_err());
    assert_eq!(pick_session(&probe(vec![s("7", None)]), None).unwrap(), "7");
    assert_eq!(
        pick_session(&probe(vec![s("2", Some(true)), s("7", Some(false))]), None).unwrap(),
        "2"
    );
    let two = probe(vec![s("2", Some(true)), s("7", Some(true))]);
    let err = pick_session(&two, None).unwrap_err();
    assert!(err.contains("2, 7"), "{err}");
    assert_eq!(pick_session(&two, Some("7")).unwrap(), "7");
    assert!(pick_session(&two, Some("9"))
        .unwrap_err()
        .contains("running: 2, 7"));
}

#[test]
fn ssh_troubles_are_said_in_words_a_person_can_act_on() {
    let w = |stderr| trouble_words("farbox", "far-box", stderr);
    assert!(w("Host key verification failed.").contains("never met far-box"));
    assert!(w("parker@h: Permission denied (publickey,password).").contains("ssh-copy-id"));
    assert!(w("ssh: connect to host h port 22: Connection timed out").contains("did not answer"));
    assert!(
        w("ssh: Could not resolve hostname h: Name or service not known")
            .contains("did not answer")
    );
    assert!(w("").contains("without saying why"));
    assert_eq!(
        w("sh: 1: nope: not found"),
        "farbox: sh: 1: nope: not found"
    );
}

#[test]
fn a_machine_is_found_by_its_name_in_any_case_or_by_its_destination() {
    let machines = vec![
        Machine {
            name: "farbox".into(),
            ssh: Some("far-box".into()),
        },
        Machine {
            name: "prod".into(),
            ssh: Some("me@piper".into()),
        },
    ];
    assert_eq!(find_machine(&machines, "FarBox").unwrap().name, "farbox");
    assert_eq!(find_machine(&machines, "me@piper").unwrap().name, "prod");
    assert!(find_machine(&machines, "elsewhere").is_none());
}

#[test]
fn a_machine_reads_as_a_few_plain_lines_with_the_command_that_opens_it() {
    let m = Machine {
        name: "farbox".into(),
        ssh: Some("far-box".into()),
    };
    let ok = Ok(Probe {
        td: Some("/usr/bin/terminal-delight".into()),
        version: Some("0.5.3".into()),
        sessions: vec![ListedSession {
            key: "2".into(),
            window: Some(true),
        }],
        ..Default::default()
    });
    let words = machine_words(&m, &ok);
    assert!(words.contains("Terminal Delight 0.5.3"));
    assert!(words.contains("terminal-delight remote farbox 2"));
    let bare = Ok(Probe::default());
    assert!(machine_words(&m, &bare).contains("not installed"));
    let down = Err("farbox: far-box did not answer.".to_string());
    assert!(machine_words(&m, &down).contains("not reachable"));
}

/// A stand-in `ssh` that runs the far command on this machine, under a
/// different runtime folder and config folder: two machines' worth of state
/// in one test.
pub(crate) fn stand_in_ssh(root: &Path, far_rt: &Path, far_cfg: &Path) -> PathBuf {
    let path = root.join("ssh");
    // Everything up to and including `--` and the destination is ssh's own;
    // what follows is the command the far login shell would be handed.
    let script = format!(
        "#!/bin/sh\nwhile [ $# -gt 0 ]; do if [ \"$1\" = -- ]; then shift 2; break; fi; shift; done\nXDG_RUNTIME_DIR='{}' XDG_CONFIG_HOME='{}' exec sh -c \"$*\"\n",
        far_rt.display(),
        far_cfg.display()
    );
    crate::testsync::write_program(&path, &script);
    path
}

/// The whole road with a stand-in ssh: `prepare` writes the far layout as this
/// window's own session file and the far tag where the window reads its tag,
/// and a far machine that has saved no layout clears a stale shadow.
#[test]
fn prepare_writes_the_far_layout_and_tag_where_the_window_reads_them() {
    let root = scratch("prepare");
    let far_rt = root.join("far-rt");
    let far_td = far_rt.join("terminal-delight");
    let far_cfg = root.join("far-cfg");
    let here_cfg = root.join("here-cfg");
    let here_rt = root.join("here-rt");
    std::fs::create_dir_all(&far_td).unwrap();
    std::fs::create_dir_all(far_cfg.join("terminal-delight/sessions")).unwrap();
    let _l = UnixListener::bind(far_td.join("session-2.sock")).unwrap();
    std::fs::write(far_td.join("session-2.tag"), "farTAG01").unwrap();
    let far_layout = far_cfg.join("terminal-delight/sessions/2.toml");
    std::fs::write(&far_layout, "active = 0\n").unwrap();

    let ssh = stand_in_ssh(&root, &far_rt, &far_cfg);
    let ssh = ssh.to_str().unwrap();
    let far = FarSession {
        name: "farbox".into(),
        dest: "stand-in".into(),
        session: "2".into(),
    };
    let socket = prepare_via(ssh, &far, &here_cfg, &here_rt).expect("prepare");
    assert_eq!(socket, far_td.join("session-2.sock").to_str().unwrap());
    let shadow = here_cfg.join("sessions/far-farbox-2.toml");
    assert_eq!(std::fs::read_to_string(&shadow).unwrap(), "active = 0\n");
    assert_eq!(
        std::fs::read_to_string(here_rt.join("session-far-farbox-2.tag")).unwrap(),
        "farTAG01"
    );

    // The far host restarts having saved nothing: the shadow goes with it.
    std::fs::remove_file(&far_layout).unwrap();
    prepare_via(ssh, &far, &here_cfg, &here_rt).expect("prepare again");
    assert!(
        !shadow.exists(),
        "a stale shadow would open a dead host's tabs"
    );

    // And a session nobody serves is refused by name.
    let gone = FarSession {
        session: "9".into(),
        ..far
    };
    let err = prepare_via(ssh, &gone, &here_cfg, &here_rt).unwrap_err();
    assert!(err.contains("no session \"9\""), "{err}");
    let _ = std::fs::remove_dir_all(&root);
}
