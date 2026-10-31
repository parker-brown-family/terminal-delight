//! A whole removal on a fixture home: a TD install with hooks beside someone
//! else's hook, the key sheet beside a person's own bindings, an MCP entry,
//! builds behind a launcher, runtime leftovers and a companion product's files
//! inside TD's own directory. Every effect outside the filesystem is a fake
//! that records what it was asked to do.
use super::apply::{apply, Effects, ExeKind, Proc, Status};
use super::guard::{self, Machine};
use super::known;
use super::plan::{plan, Inputs, Op, Retention};
use super::roots::Roots;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

struct Fake {
    account: bool,
    procs: Vec<Proc>,
    ran: Vec<String>,
    signalled: Vec<(u32, i32)>,
    closed: Vec<PathBuf>,
    thawed: Vec<String>,
    dt_off: usize,
}

impl Effects for Fake {
    fn account_home(&self) -> bool {
        self.account
    }
    fn self_pid(&self) -> u32 {
        1
    }
    fn processes(&mut self) -> Vec<Proc> {
        self.procs.clone()
    }
    fn signal(&mut self, pid: u32, sig: i32, exe: &Path) -> bool {
        if !self.procs.iter().any(|p| p.pid == pid && p.exe == exe) {
            return false;
        }
        self.signalled.push((pid, sig));
        self.procs.retain(|p| p.pid != pid);
        true
    }
    fn close_host(
        &mut self,
        s: &Path,
        _allow: &dyn Fn(&Path, ExeKind) -> bool,
    ) -> Result<(), String> {
        self.closed.push(s.to_path_buf());
        let _ = std::fs::remove_file(s);
        Ok(())
    }
    fn thaw(&mut self, session: &str) -> usize {
        self.thawed.push(session.into());
        1
    }
    fn run(&mut self, argv: &[String], _cwd: Option<&Path>) -> Result<String, String> {
        self.ran.push(argv.join(" "));
        Ok(String::new())
    }
    fn default_terminal_off(&mut self, _list: &Path, journal: &Path) -> Result<String, String> {
        self.dt_off += 1;
        let _ = std::fs::remove_file(journal);
        Ok("restored".into())
    }
    fn export(&mut self, dest: &Path) -> Result<String, String> {
        std::fs::create_dir_all(dest)
            .map(|_| "exported".into())
            .map_err(|e| e.to_string())
    }
    fn sleep(&mut self, _: Duration) {}
}

fn fake(account: bool) -> Fake {
    Fake {
        account,
        procs: vec![],
        ran: vec![],
        signalled: vec![],
        closed: vec![],
        thawed: vec![],
        dt_off: 0,
    }
}

const MACHINE: &str = "e2e-machine-id-0001";

fn machine() -> Machine {
    Machine {
        id: Some(MACHINE.into()),
        hostname: Some("fixture".into()),
    }
}

fn w(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn elf(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, b"\x7fELF fake build").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

struct Fx {
    root: PathBuf,
    env: Vec<(String, String)>,
}

impl Fx {
    fn new(tag: &str) -> Fx {
        let root = std::env::temp_dir().join(format!("td-e2e-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let h = |s: &str| root.join(s);
        let bin = h(".local/bin");
        // TD's own files.
        w(&h(".config/terminal-delight/theme.toml"), "theme = \"x\"\n");
        w(&h(".config/terminal-delight/sessions/1.toml"), "[s]\n");
        w(&h(".config/terminal-delight/mobile/token"), "secret\n");
        w(
            &h(".local/state/terminal-delight/default-terminal.journal"),
            "journal\n",
        );
        w(
            &h(".local/share/terminal-delight/skills/decision-brief/SKILL.md"),
            "kit\n",
        );
        w(
            &h(".local/share/terminal-delight/palettes/army/colors"),
            "sibling\n",
        );
        w(&h(".cache/terminal-delight/pages/x"), "cache\n");
        w(&h("run/terminal-delight-glyph-cat.svg"), "<svg/>");
        // TD's receipt for what it unpacked, as `art::record_asset` writes it.
        w(
            &h("run/terminal-delight-assets.manifest"),
            &format!(
                "{}\tterminal-delight-glyph-cat.svg\n",
                known::sha256_bytes(b"<svg/>")
            ),
        );
        w(
            &h("run/terminal-delight-scene-storyboard.webp"),
            "another tool's picture",
        );
        w(&h("run/other.txt"), "not ours\n");
        std::fs::create_dir_all(h("run/terminal-delight")).unwrap();
        // A real socket, as a session host leaves one.
        drop(
            std::os::unix::net::UnixListener::bind(h("run/terminal-delight/session-1.sock"))
                .unwrap(),
        );
        w(&h("run/terminal-delight/session-1.log"), "log\n");
        w(&h("tmp/td-demo-9-1.toml"), "demo\n");
        w(
            &h("tmp/terminal-delight-assets.manifest"),
            &format!("{}\ttd-demo-9-1.toml\n", known::sha256_bytes(b"demo\n")),
        );
        // Helper with its receipt, settings that run it beside someone else's hook.
        let helper = bin.join("td-agent-hooks");
        w(&helper, "#!/bin/sh\n# td-agent-hooks fixture\n");
        let sha = known::sha256_file(&helper).unwrap();
        let settings = h(".claude/settings.json");
        w(
            &h(".local/state/terminal-delight/receipts/agent-hooks.receipt"),
            &format!(
                "file\t{sha}\t{}\nhook\t{}\t{}\n",
                helper.display(),
                settings.display(),
                helper.display()
            ),
        );
        w(
            &settings,
            &format!(
                "{{\n  \"theme\": \"dark\",\n  \"hooks\": {{\n    \"Stop\": [\n      {{\n        \"hooks\": [\n          {{\n            \"type\": \"command\",\n            \"command\": \"{}\"\n          }},\n          {{\n            \"type\": \"command\",\n            \"command\": \"someone-elses-hook\"\n          }}\n        ]\n      }}\n    ]\n  }}\n}}\n",
                helper.display()
            ),
        );
        // Key sheet beside a person's own bindings, and a line TD's README asked for.
        let block = format!(
            "{}\n-- SUPER+ALT+T shows every Terminal Delight key in Omarchy's searchable key\n-- menu, the way SUPER+K shows Omarchy's own and SUPER+CTRL+K shows Herdr's.\n-- The list is the docs site's Keys page, copied beside the script by\n-- scripts/install-keys-hotkey.sh in the terminal-delight repo. The seat was\n-- td-send's, whose hotkey is retired; T reads for Terminal Delight, and the K\n-- family on SUPER, SUPER+ALT and SUPER+CTRL is all Omarchy's already.\no.bind(\"SUPER + ALT + T\", \"Terminal Delight keybindings\", \"{}/td-keybindings\")\n{}\n",
            super::blocks::OPEN,
            bin.display(),
            super::blocks::SHUT
        );
        w(
            &h(".config/hypr/bindings.lua"),
            &format!("-- mine\n\n{block}-- also mine\n"),
        );
        w(
            &h(".config/hypr/hyprland.lua"),
            "o.window(\"terminal-delight\", { tag = \"+terminal\" })\n",
        );
        // A legacy bare-command registration; new setup uses an absolute launcher.
        w(&h(".claude.json"), "{\"mcpServers\":{\"terminal-delight\":{\"type\":\"stdio\",\"command\":\"terminal-delight\",\"args\":[\"mcp\"]}}}");
        // Builds behind a launcher, and something else in the builds dir.
        elf(&h(".local/lib/terminal-delight/td-aaa-current"));
        elf(&h(".local/lib/terminal-delight/td-bbb-old"));
        w(&h(".local/lib/terminal-delight/notes.txt"), "mine\n");
        std::os::unix::fs::symlink(
            h(".local/lib/terminal-delight/td-aaa-current"),
            bin.join("terminal-delight"),
        )
        .unwrap();
        // A companion product's service.
        w(&h(".config/systemd/user/td-tubes.service"), "[Unit]\n");
        let env = vec![
            ("HOME".to_string(), root.display().to_string()),
            (
                "XDG_RUNTIME_DIR".to_string(),
                h("run").display().to_string(),
            ),
            ("TMPDIR".to_string(), h("tmp").display().to_string()),
            ("PATH".to_string(), bin.display().to_string()),
        ];
        Fx { root, env }
    }

    fn env(&self) -> impl Fn(&str) -> Option<OsString> + '_ {
        move |k| {
            self.env
                .iter()
                .find(|(a, _)| a == k)
                .map(|(_, v)| OsString::from(v))
        }
    }

    fn roots(&self) -> Roots {
        Roots::from_env(&self.env()).unwrap()
    }

    fn plan(&self, retention: Retention) -> super::plan::Plan {
        let r = self.roots();
        let env = self.env();
        plan(&Inputs {
            roots: &r,
            env: &env,
            machine: &machine(),
            retention,
            stamp: "20261030-120000",
        })
    }
}

/// The removal directory the caller creates before running the worker.
fn out(fx: &Fx) -> PathBuf {
    let o = fx.root.join("removal");
    std::fs::create_dir_all(&o).unwrap();
    o
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn keep_records_removes_the_application_and_reports_the_rest() {
    let fx = Fx::new("keep");
    let p = fx.plan(Retention::Keep);
    assert!(p.protected.is_none());
    let ops: Vec<&Op> = p.steps.iter().map(|s| &s.op).collect();
    assert!(ops.iter().any(|o| matches!(o, Op::EditSettings { .. })));
    assert!(ops.iter().any(|o| matches!(o, Op::StripKeyBlock { .. })));
    assert!(ops.iter().any(|o| matches!(o, Op::McpRemove { .. })));
    assert!(ops
        .iter()
        .any(|o| matches!(o, Op::DefaultTerminalOff { .. })));
    assert!(p.reports.iter().any(|r| r.what.contains("palettes")));
    assert!(p.reports.iter().any(|r| r.what.contains("hyprland.lua")));
    assert!(p
        .reports
        .iter()
        .any(|r| r.what.contains("td-tubes.service")));
    assert!(p.reports.iter().any(|r| r.what.contains("phone")));
    assert!(p.reports.iter().any(|r| r.what.contains("notes.txt")));

    let out = out(&fx);
    let mut f = fake(false);
    f.procs.push(Proc {
        pid: 4242,
        exe: fx.root.join(".local/lib/terminal-delight/td-aaa-current"),
        argv: vec![],
        comm: String::new(),
        kind: ExeKind::Build,
    });
    let fresh = fx.plan(Retention::Keep);
    let rc = apply(&p, &fresh, &fx.roots(), &out, &mut f);
    assert!(rc.refused.is_none(), "{:?}", rc.refused);
    let failed: Vec<_> = rc
        .outcomes
        .iter()
        .filter(|o| matches!(o.status, Status::Failed | Status::Blocked))
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
    // The fixture is not the account home: other programs' CLIs and the
    // systemd bus were not touched, and the receipt does not claim complete.
    assert!(f.ran.iter().all(|r| !r.contains("claude")), "{:?}", f.ran);
    assert!(f.thawed.is_empty());
    assert!(!rc.complete);
    assert_eq!(f.signalled, vec![(4242, libc::SIGTERM)]);
    assert_eq!(f.closed.len(), 1);
    assert_eq!(f.dt_off, 1);

    let h = |s: &str| fx.root.join(s);
    let settings = std::fs::read_to_string(h(".claude/settings.json")).unwrap();
    assert!(settings.contains("someone-elses-hook"));
    assert!(!settings.contains("td-agent-hooks"));
    assert!(settings.starts_with("{\n  \"theme\": \"dark\""));
    assert_eq!(
        std::fs::read_to_string(h(".config/hypr/bindings.lua")).unwrap(),
        "-- mine\n\n-- also mine\n"
    );
    for gone in [
        ".local/bin/td-agent-hooks",
        ".local/bin/terminal-delight",
        ".local/lib/terminal-delight/td-bbb-old",
        ".cache/terminal-delight",
        ".local/share/terminal-delight/skills",
        "run/terminal-delight",
        "run/terminal-delight-glyph-cat.svg",
        "tmp/td-demo-9-1.toml",
        ".local/state/terminal-delight/receipts/agent-hooks.receipt",
    ] {
        assert!(
            std::fs::symlink_metadata(h(gone)).is_err(),
            "{gone} should be gone"
        );
    }
    for kept in [
        ".config/terminal-delight/theme.toml",
        ".config/terminal-delight/mobile/token",
        ".local/share/terminal-delight/palettes/army/colors",
        "run/other.txt",
        "run/terminal-delight-scene-storyboard.webp",
        ".local/lib/terminal-delight/notes.txt",
        ".config/hypr/hyprland.lua",
        ".claude.json",
    ] {
        assert!(h(kept).exists(), "{kept} should stay");
    }
    // No backup beside the person's settings; the copy is in the removal dir.
    let beside: Vec<_> = std::fs::read_dir(h(".claude"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(beside.len(), 1, "{beside:?}");
    assert!(std::fs::read_dir(out.join("before")).unwrap().count() >= 2);
    assert!(out.join("receipt.md").exists() && out.join("receipt.json").exists());
}

#[test]
fn delete_and_export_remove_records_only_after_the_export() {
    let fx = Fx::new("export");
    let p = fx.plan(Retention::Export);
    let at = |pred: &dyn Fn(&Op) -> bool| p.steps.iter().position(|s| pred(&s.op)).unwrap();
    let export = at(&|o| matches!(o, Op::Export { .. }));
    // The export runs after TD has stopped and saved, and records go after it.
    assert!(at(&|o| matches!(o, Op::StopProcesses { .. })) < export);
    assert!(export < at(&|o| matches!(o, Op::RemoveRecords { .. })));
    assert!(p.steps[at(&|o| matches!(o, Op::RemoveRecords { .. }))]
        .after
        .contains(&export));
    let mut f = fake(true);
    let fresh = fx.plan(Retention::Export);
    let rc = apply(&p, &fresh, &fx.roots(), &out(&fx), &mut f);
    assert!(rc.refused.is_none());
    assert!(fx
        .root
        .join("terminal-delight-export-20261030-120000")
        .is_dir());
    assert!(!fx.root.join(".config/terminal-delight").exists());
    assert!(!fx.root.join(".local/state/terminal-delight").exists());
    // On the account home the clients' own CLIs are used, and frozen panes thawed.
    assert!(
        f.ran
            .iter()
            .any(|r| r
                == "/usr/bin/env -u CLAUDE_CONFIG_DIR claude mcp remove -s user terminal-delight"),
        "{:?}",
        f.ran
    );
    assert_eq!(f.thawed, vec!["1".to_string()]);
}

#[test]
fn a_failed_export_keeps_the_records() {
    struct NoExport(Fake);
    impl Effects for NoExport {
        fn account_home(&self) -> bool {
            false
        }
        fn self_pid(&self) -> u32 {
            1
        }
        fn processes(&mut self) -> Vec<Proc> {
            vec![]
        }
        fn signal(&mut self, _: u32, _: i32, _: &Path) -> bool {
            false
        }
        fn close_host(
            &mut self,
            _: &Path,
            _: &dyn Fn(&Path, ExeKind) -> bool,
        ) -> Result<(), String> {
            Ok(())
        }
        fn thaw(&mut self, _: &str) -> usize {
            0
        }
        fn run(&mut self, a: &[String], c: Option<&Path>) -> Result<String, String> {
            self.0.run(a, c)
        }
        fn default_terminal_off(&mut self, l: &Path, j: &Path) -> Result<String, String> {
            self.0.default_terminal_off(l, j)
        }
        fn export(&mut self, _: &Path) -> Result<String, String> {
            Err("disk full".into())
        }
        fn sleep(&mut self, _: Duration) {}
    }
    let fx = Fx::new("noexport");
    let p = fx.plan(Retention::Export);
    let fresh = fx.plan(Retention::Export);
    let rc = apply(
        &p,
        &fresh,
        &fx.roots(),
        &out(&fx),
        &mut NoExport(fake(false)),
    );
    assert!(fx.root.join(".config/terminal-delight/theme.toml").exists());
    assert!(fx.root.join(".local/state/terminal-delight").exists());
    assert!(fx.root.join(".claude/settings.json").exists());
    assert!(rc.outcomes.iter().any(|o| o.status == Status::Failed));
    assert!(!rc.complete);
}

#[test]
fn a_protected_machine_refuses_and_changes_nothing() {
    let fx = Fx::new("protected");
    guard::protect(&fx.roots(), &machine(), "test", "2026-10-30").unwrap();
    let before = std::fs::read(fx.root.join(".claude/settings.json")).unwrap();
    let p = fx.plan(Retention::Delete);
    assert!(p.protected.is_some());
    let rc = apply(
        &p,
        &fx.plan(Retention::Delete),
        &fx.roots(),
        &out(&fx),
        &mut fake(true),
    );
    assert!(rc.refused.is_some());
    assert!(rc.outcomes.is_empty());
    assert_eq!(
        std::fs::read(fx.root.join(".claude/settings.json")).unwrap(),
        before
    );
    assert!(fx.root.join(".local/bin/terminal-delight").exists());
}

#[test]
fn a_change_after_the_plan_refuses() {
    let fx = Fx::new("changed");
    let p = fx.plan(Retention::Keep);
    w(
        &fx.root.join(".claude/settings.json"),
        "{\"changed\": true}\n",
    );
    let rc = apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut fake(false),
    );
    assert!(rc
        .refused
        .as_deref()
        .unwrap_or("")
        .contains("changed since"));
    assert!(fx.root.join(".local/bin/td-agent-hooks").exists());
}

#[test]
fn a_running_build_that_will_not_stop_blocks_the_file_removals() {
    struct Stubborn(Fake);
    impl Effects for Stubborn {
        fn account_home(&self) -> bool {
            false
        }
        fn self_pid(&self) -> u32 {
            1
        }
        fn processes(&mut self) -> Vec<Proc> {
            self.0.procs.clone()
        }
        fn signal(&mut self, p: u32, s: i32, _: &Path) -> bool {
            self.0.signalled.push((p, s));
            true
        }
        fn close_host(
            &mut self,
            _: &Path,
            _: &dyn Fn(&Path, ExeKind) -> bool,
        ) -> Result<(), String> {
            Ok(())
        }
        fn thaw(&mut self, _: &str) -> usize {
            0
        }
        fn run(&mut self, _: &[String], _: Option<&Path>) -> Result<String, String> {
            Ok(String::new())
        }
        fn default_terminal_off(&mut self, l: &Path, j: &Path) -> Result<String, String> {
            self.0.default_terminal_off(l, j)
        }
        fn export(&mut self, _: &Path) -> Result<String, String> {
            Ok(String::new())
        }
        fn sleep(&mut self, _: Duration) {}
    }
    let fx = Fx::new("stubborn");
    let mut f = fake(false);
    f.procs.push(Proc {
        pid: 77,
        exe: fx.root.join(".local/lib/terminal-delight/td-bbb-old"),
        argv: vec![],
        comm: String::new(),
        kind: ExeKind::Build,
    });
    let p = fx.plan(Retention::Keep);
    let rc = apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut Stubborn(f),
    );
    assert!(rc
        .outcomes
        .iter()
        .any(|o| o.status == Status::Failed && o.op.contains("stop processes")));
    assert!(fx
        .root
        .join(".local/lib/terminal-delight/td-bbb-old")
        .exists());
    assert!(fx.root.join(".local/bin/td-agent-hooks").exists());
    assert!(rc.outcomes.iter().any(|o| o.status == Status::Blocked));
}

#[test]
fn a_fixture_whose_directories_reach_outside_its_home_is_refused() {
    let fx = Fx::new("confined");
    let mut env = fx.env.clone();
    // A runtime dir outside the fixture home, as an inherited real one would be.
    let outside = std::env::temp_dir().join(format!("td-e2e-outside-{}", std::process::id()));
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("terminal-delight-mark.png"), b"x").unwrap();
    env.retain(|(k, _)| k != "XDG_RUNTIME_DIR");
    env.push(("XDG_RUNTIME_DIR".into(), outside.display().to_string()));
    let lookup = |k: &str| {
        env.iter()
            .find(|(a, _)| a == k)
            .map(|(_, v)| OsString::from(v))
    };
    let r = Roots::from_env(&lookup).unwrap();
    let mk = || {
        plan(&Inputs {
            roots: &r,
            env: &lookup,
            machine: &machine(),
            retention: Retention::Keep,
            stamp: "20261030-120000",
        })
    };
    let p = mk();
    assert!(p.blocked.is_some());
    let rc = apply(&p, &mk(), &r, &out(&fx), &mut fake(false));
    assert!(rc.refused.is_some());
    assert!(outside.join("terminal-delight-mark.png").exists());
    assert!(fx.root.join(".local/bin/td-agent-hooks").exists());
    std::fs::remove_dir_all(outside).unwrap();
}

#[test]
fn overlapping_roots_are_refused() {
    let fx = Fx::new("overlap");
    let mut env = fx.env.clone();
    let cfg = fx.root.join(".config").display().to_string();
    env.push(("XDG_CACHE_HOME".into(), cfg));
    let lookup = |k: &str| {
        env.iter()
            .find(|(a, _)| a == k)
            .map(|(_, v)| OsString::from(v))
    };
    let r = Roots::from_env(&lookup).unwrap();
    let p = plan(&Inputs {
        roots: &r,
        env: &lookup,
        machine: &machine(),
        retention: Retention::Keep,
        stamp: "20261030-120000",
    });
    assert!(
        p.blocked.as_deref().unwrap_or("").contains("overlap"),
        "{:?}",
        p.blocked
    );
}

#[test]
fn an_export_that_cannot_be_written_keeps_the_records() {
    let fx = Fx::new("exportlink");
    let outside =
        std::env::temp_dir().join(format!("td-e2e-outside-export-{}", std::process::id()));
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(
        &outside,
        fx.root.join("terminal-delight-export-20261030-120000"),
    )
    .unwrap();
    let p = fx.plan(Retention::Export);
    let rc = apply(
        &p,
        &fx.plan(Retention::Export),
        &fx.roots(),
        &out(&fx),
        &mut fake(false),
    );
    assert!(rc.refused.is_none());
    assert!(fx.root.join(".config/terminal-delight/theme.toml").exists());
    assert!(fx.root.join(".local/state/terminal-delight").exists());
    assert!(rc.records.contains("still in place"), "{}", rc.records);
    std::fs::remove_dir_all(outside).unwrap();
}

#[test]
fn a_linked_socket_is_never_asked_to_close() {
    let fx = Fx::new("socklink");
    let elsewhere = fx.root.join("elsewhere.sock");
    drop(std::os::unix::net::UnixListener::bind(&elsewhere).unwrap());
    std::os::unix::fs::symlink(
        &elsewhere,
        fx.root.join("run/terminal-delight/session-live.sock"),
    )
    .unwrap();
    let mut f = fake(false);
    let p = fx.plan(Retention::Keep);
    apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut f,
    );
    assert_eq!(
        f.closed,
        vec![fx.root.join("run/terminal-delight/session-1.sock")]
    );
}

#[test]
fn writing_the_plan_for_review_does_not_change_the_plan() {
    let fx = Fx::new("selfstable");
    std::fs::remove_dir_all(fx.root.join(".cache/terminal-delight")).unwrap();
    let before = fx.plan(Retention::Keep);
    // The review copy lands in TD's cache, which did not exist a moment ago.
    std::fs::create_dir_all(fx.root.join(".cache/terminal-delight/admin")).unwrap();
    std::fs::write(
        fx.root
            .join(".cache/terminal-delight/admin/uninstall-plan.json"),
        "{}",
    )
    .unwrap();
    std::fs::write(fx.root.join("run/terminal-delight-glyph-new.svg"), "<svg/>").unwrap();
    assert_eq!(before.steps, fx.plan(Retention::Keep).steps);
}

#[test]
fn a_fixture_launcher_cannot_make_a_live_build_a_target() {
    let fx = Fx::new("livebuild");
    // A "live" build outside the fixture home, and the fixture's launcher
    // pointing straight at it.
    let live = std::env::temp_dir().join(format!("td-e2e-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&live);
    elf(&live.join("td-live-build"));
    let launcher = fx.root.join(".local/bin/terminal-delight");
    std::fs::remove_file(&launcher).unwrap();
    std::os::unix::fs::symlink(live.join("td-live-build"), &launcher).unwrap();
    let p = fx.plan(Retention::Keep);
    for s in &p.steps {
        if let Op::StopProcesses { exes, .. } = &s.op {
            assert!(exes.iter().all(|e| !e.starts_with(&live)), "{exes:?}");
        }
    }
    let mut f = fake(false);
    f.procs.push(Proc {
        pid: 999,
        exe: live.join("td-live-build"),
        argv: vec![],
        comm: String::new(),
        kind: ExeKind::Build,
    });
    let rc = apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut f,
    );
    assert!(rc.refused.is_none());
    assert!(f.signalled.is_empty(), "{:?}", f.signalled);
    assert!(live.join("td-live-build").exists());
    std::fs::remove_dir_all(live).unwrap();
}

#[test]
fn td_still_the_default_terminal_keeps_the_program() {
    let fx = Fx::new("stilldefault");
    // No journal, and TD first in the list: nothing to restore, so the
    // program and its desktop entry must stay until the person picks another.
    std::fs::remove_file(
        fx.root
            .join(".local/state/terminal-delight/default-terminal.journal"),
    )
    .unwrap();
    w(
        &fx.root.join(".config/xdg-terminals.list"),
        "terminal-delight.desktop\nfoot.desktop\n",
    );
    let p = fx.plan(Retention::Keep);
    let rc = apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut fake(false),
    );
    assert!(rc
        .outcomes
        .iter()
        .any(|o| o.status == Status::Failed && o.op.contains("default terminal")));
    assert!(fx
        .root
        .join(".local/lib/terminal-delight/td-aaa-current")
        .exists());
    assert!(fx.root.join(".local/bin/terminal-delight").exists());
}

#[test]
fn only_the_names_td_gives_its_loose_files_are_removed() {
    use super::apply::loose_asset;
    for yes in [
        "terminal-delight-ping.mp3",
        "terminal-delight-glyph-cat.svg",
        "terminal-delight-prop-tile.png",
        "terminal-delight-scene-map.webp",
        "terminal-delight-robot.png",
        "td-demo-123-456.toml",
    ] {
        assert!(loose_asset(yes), "{yes}");
    }
    for no in [
        "terminal-delight-logo-draft.png",
        "terminal-delight-glyph-../x.svg",
        "terminal-delight-notes.md",
        "td-demo-x.toml",
        "terminal-delight-ping.mp3.bak",
    ] {
        assert!(!loose_asset(no), "{no}");
    }
}

#[test]
fn a_bad_stamp_or_a_non_empty_journal_dir_refuses_before_writing() {
    let fx = Fx::new("stamp");
    let mut p = fx.plan(Retention::Keep);
    p.stamp = "../../evil".into();
    let rc = apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut fake(false),
    );
    assert!(rc.refused.is_some());
    assert!(fx.root.join(".local/bin/td-agent-hooks").exists());
}

#[test]
fn a_registration_replaced_during_the_run_is_left_alone() {
    struct Swapper(Fake, PathBuf);
    impl Effects for Swapper {
        fn account_home(&self) -> bool {
            true
        }
        fn self_pid(&self) -> u32 {
            1
        }
        fn processes(&mut self) -> Vec<Proc> {
            vec![]
        }
        fn signal(&mut self, _: u32, _: i32, _: &Path) -> bool {
            false
        }
        fn close_host(
            &mut self,
            _: &Path,
            _: &dyn Fn(&Path, ExeKind) -> bool,
        ) -> Result<(), String> {
            Ok(())
        }
        fn thaw(&mut self, _: &str) -> usize {
            0
        }
        fn run(&mut self, a: &[String], c: Option<&Path>) -> Result<String, String> {
            self.0.run(a, c)
        }
        fn default_terminal_off(&mut self, l: &Path, j: &Path) -> Result<String, String> {
            // Mid-run, something else takes the name for its own server.
            std::fs::write(
                &self.1,
                r#"{"mcpServers":{"terminal-delight":{"command":"/opt/other/server","args":[]}}}"#,
            )
            .unwrap();
            self.0.default_terminal_off(l, j)
        }
        fn export(&mut self, _: &Path) -> Result<String, String> {
            Ok(String::new())
        }
        fn sleep(&mut self, _: Duration) {}
    }
    let fx = Fx::new("mcpswap");
    let p = fx.plan(Retention::Keep);
    let mut sw = Swapper(fake(true), fx.root.join(".claude.json"));
    let rc = apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut sw,
    );
    assert!(rc
        .outcomes
        .iter()
        .any(|o| o.status == Status::Failed && o.detail.contains("changed since the plan")));
    assert!(
        sw.0.ran.iter().all(|r| !r.contains("claude mcp remove")),
        "{:?}",
        sw.0.ran
    );
}

#[test]
fn a_runtime_dir_that_is_the_settings_dir_is_refused() {
    let fx = Fx::new("rtalias");
    let mut env = fx.env.clone();
    env.retain(|(k, _)| k != "XDG_RUNTIME_DIR");
    env.push((
        "XDG_RUNTIME_DIR".into(),
        fx.root.join(".config").display().to_string(),
    ));
    let lookup = |k: &str| {
        env.iter()
            .find(|(a, _)| a == k)
            .map(|(_, v)| OsString::from(v))
    };
    let r = Roots::from_env(&lookup).unwrap();
    let p = plan(&Inputs {
        roots: &r,
        env: &lookup,
        machine: &machine(),
        retention: Retention::Keep,
        stamp: "20261030-120000",
    });
    assert!(
        p.blocked.as_deref().unwrap_or("").contains("overlap"),
        "{:?}",
        p.blocked
    );
}

#[test]
fn a_runtime_dir_on_top_of_older_records_is_refused() {
    let fx = Fx::new("legacyrt");
    // Current settings live elsewhere; an older copy stays at the default.
    std::fs::create_dir_all(fx.root.join("custom-config/terminal-delight")).unwrap();
    let mut env = fx.env.clone();
    env.retain(|(k, _)| k != "XDG_RUNTIME_DIR");
    env.push((
        "XDG_CONFIG_HOME".into(),
        fx.root.join("custom-config").display().to_string(),
    ));
    env.push((
        "XDG_RUNTIME_DIR".into(),
        fx.root.join(".config").display().to_string(),
    ));
    let lookup = |k: &str| {
        env.iter()
            .find(|(a, _)| a == k)
            .map(|(_, v)| OsString::from(v))
    };
    let r = Roots::from_env(&lookup).unwrap();
    let p = plan(&Inputs {
        roots: &r,
        env: &lookup,
        machine: &machine(),
        retention: Retention::Keep,
        stamp: "20261030-120000",
    });
    assert!(
        p.blocked
            .as_deref()
            .unwrap_or("")
            .contains("older settings"),
        "{:?}",
        p.blocked
    );
}

#[test]
fn a_receipt_others_can_write_proves_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let fx = Fx::new("openmanifest");
    let manifest = fx.root.join("run/terminal-delight-assets.manifest");
    std::fs::set_permissions(&manifest, std::fs::Permissions::from_mode(0o666)).unwrap();
    let p = fx.plan(Retention::Keep);
    apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut fake(false),
    );
    assert!(fx.root.join("run/terminal-delight-glyph-cat.svg").exists());
}

#[test]
fn an_unreadable_registration_is_a_failure_not_already_gone() {
    struct Breaker(Fake, PathBuf);
    impl Effects for Breaker {
        fn account_home(&self) -> bool {
            true
        }
        fn self_pid(&self) -> u32 {
            1
        }
        fn processes(&mut self) -> Vec<Proc> {
            vec![]
        }
        fn signal(&mut self, _: u32, _: i32, _: &Path) -> bool {
            false
        }
        fn close_host(
            &mut self,
            _: &Path,
            _: &dyn Fn(&Path, ExeKind) -> bool,
        ) -> Result<(), String> {
            Ok(())
        }
        fn thaw(&mut self, _: &str) -> usize {
            0
        }
        fn run(&mut self, a: &[String], c: Option<&Path>) -> Result<String, String> {
            self.0.run(a, c)
        }
        fn default_terminal_off(&mut self, l: &Path, j: &Path) -> Result<String, String> {
            // Mid-run, the client is halfway through rewriting its config.
            std::fs::write(&self.1, "{\"mcpServers\": {").unwrap();
            self.0.default_terminal_off(l, j)
        }
        fn export(&mut self, _: &Path) -> Result<String, String> {
            Ok(String::new())
        }
        fn sleep(&mut self, _: Duration) {}
    }
    let fx = Fx::new("mcpbroken");
    let p = fx.plan(Retention::Keep);
    let mut b = Breaker(fake(true), fx.root.join(".claude.json"));
    let rc = apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut b,
    );
    assert!(rc
        .outcomes
        .iter()
        .any(|o| o.status == Status::Failed && o.detail.contains("cannot read it now")));
}

#[test]
fn an_appimage_or_a_wrapper_at_the_launcher_stays_and_a_true_copy_goes() {
    let fx = Fx::new("reglauncher");
    let launcher = fx.root.join(".local/bin/terminal-delight");
    let removes_launcher = |p: &super::plan::Plan| {
        p.steps
            .iter()
            .any(|s| matches!(&s.op, Op::RemoveFile { path, .. } | Op::RemoveLink { path, .. } if *path == launcher))
    };
    // An AppImage moved into the launcher's place, as the install page allows.
    std::fs::remove_file(&launcher).unwrap();
    let mut image = b"\x7fELF\x02\x01\x01\x00AI\x02".to_vec();
    image.extend_from_slice(&[0u8; 64]);
    std::fs::write(&launcher, &image).unwrap();
    let p = fx.plan(Retention::Keep);
    assert!(!removes_launcher(&p));
    assert!(p.reports.iter().any(|r| r.what.contains("AppImage")));
    // A compiled wrapper of the person's own, which the documented MCP form
    // now resolves to: neither removed nor taken for TD.
    std::fs::write(&launcher, b"\x7fELF someone's wrapper").unwrap();
    let p = fx.plan(Retention::Keep);
    assert!(!removes_launcher(&p));
    assert!(p
        .steps
        .iter()
        .all(|s| !matches!(s.op, Op::McpRemove { .. })));
    assert!(p.reports.iter().any(|r| r.what.contains("not this TD")));
    // This very program in the launcher's place is TD's, and goes.
    std::fs::remove_file(&launcher).unwrap();
    if std::fs::hard_link(std::env::current_exe().unwrap(), &launcher).is_ok() {
        let p = fx.plan(Retention::Keep);
        assert!(removes_launcher(&p));
    }
}

#[test]
fn a_registration_with_its_own_path_to_a_wrapper_is_not_td() {
    let fx = Fx::new("mcppath");
    // The person's wrapper, first on the PATH the entry itself sets.
    let wrapper = fx.root.join("custom-mcp/bin/terminal-delight");
    elf(&wrapper);
    std::fs::write(&wrapper, b"\x7fELF my wrapper").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    w(
        &fx.root.join(".claude.json"),
        &format!(
            "{{\"mcpServers\":{{\"terminal-delight\":{{\"command\":\"terminal-delight\",\"args\":[\"mcp\"],\"env\":{{\"PATH\":\"{}\"}}}}}}}}",
            fx.root.join("custom-mcp/bin").display()
        ),
    );
    let p = fx.plan(Retention::Keep);
    assert!(p
        .steps
        .iter()
        .all(|s| !matches!(s.op, Op::McpRemove { .. })));
    assert!(p.reports.iter().any(|r| r.what.contains("MCP entry")));
}

#[test]
fn an_export_cannot_be_written_where_a_removal_would_delete_it() {
    let fx = Fx::new("exportdest");
    let r = fx.roots();
    let uid = unsafe { libc::getuid() };
    for dest in [
        r.temp.join(format!("terminal-delight-{uid}/my-export")),
        fx.root.join(".local/state/terminal-delight/my-export"),
    ] {
        assert!(r.is_inside_td(&dest), "{}", dest.display());
    }
}

/// The three AppImages there are: type 2, type 1 with its marker, and an
/// older type 1 that carries only the ISO 9660 volume it is built around.
fn appimages() -> Vec<(&'static str, Vec<u8>)> {
    let mut two = b"\x7fELF\x02\x01\x01\x00AI\x02".to_vec();
    two.extend_from_slice(&[0u8; 32]);
    let mut one = b"\x7fELF\x01\x01\x01\x00AI\x01".to_vec();
    one.extend_from_slice(&[0u8; 32]);
    let mut iso = b"\x7fELF\x01\x01\x01\x00\x00\x00\x00".to_vec();
    iso.resize(32769, 0);
    iso.extend_from_slice(b"CD001\x01");
    vec![("type 2", two), ("type 1", one), ("type 1, ISO only", iso)]
}

#[test]
fn an_appimage_in_the_builds_directory_is_never_a_build() {
    for (kind, bytes) in appimages() {
        let fx = Fx::new("buildsimage");
        let image = fx
            .root
            .join(".local/lib/terminal-delight/td-release.AppImage");
        std::fs::write(&image, &bytes).unwrap();
        let p = fx.plan(Retention::Keep);
        assert!(
            p.steps
                .iter()
                .all(|s| !matches!(&s.op, Op::RemoveFile { path, .. } | Op::RemoveBuild { path } if *path == image)),
            "{kind}"
        );
        for s in &p.steps {
            if let Op::StopProcesses { exes, .. } = &s.op {
                assert!(!exes.contains(&image), "{kind}");
            }
        }
        // Old-build pruning never counts it either.
        let listing = super::builds::list(&fx.roots(), &Vec::new);
        assert!(listing.builds.iter().all(|b| b.path != image), "{kind}");
        assert!(
            listing
                .other
                .iter()
                .any(|o| o.name == "td-release.AppImage"),
            "{kind}"
        );
        // And a process running it is never a build.
        let kind_read = ExeKind::of_reader(&mut std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(kind_read, ExeKind::AppImage, "{kind}");
    }
}

#[test]
fn a_registration_whose_path_starts_relative_or_with_its_own_cwd_is_reported() {
    let fx = Fx::new("mcprel");
    let bin = fx.root.join(".local/bin");
    for entry in [
        format!(
            r#"{{"command":"terminal-delight","args":["mcp"],"env":{{"PATH":".:{}"}}}}"#,
            bin.display()
        ),
        format!(
            r#"{{"command":"terminal-delight","args":["mcp"],"env":{{"PATH":":{}"}}}}"#,
            bin.display()
        ),
        r#"{"command":"terminal-delight","args":["mcp"],"cwd":"/somewhere"}"#.to_string(),
    ] {
        w(
            &fx.root.join(".claude.json"),
            &format!(r#"{{"mcpServers":{{"terminal-delight":{entry}}}}}"#),
        );
        let p = fx.plan(Retention::Keep);
        assert!(
            p.steps
                .iter()
                .all(|s| !matches!(s.op, Op::McpRemove { .. })),
            "{entry}"
        );
    }
}

#[test]
fn absolute_launcher_registrations_ignore_shadowing_path_for_every_client() {
    let mut fx = Fx::new("mcpabsolute");
    let launcher = fx.root.join(".local/bin/terminal-delight");
    let entry = serde_json::json!({"command": launcher, "args": ["mcp"]});
    let settings = serde_json::json!({"mcpServers": {"terminal-delight": entry}});
    w(&fx.root.join(".claude.json"), &settings.to_string());
    w(
        &fx.root.join(".gemini/settings.json"),
        &settings.to_string(),
    );
    w(
        &fx.root.join(".codex/config.toml"),
        &format!(
            "[mcp_servers.terminal-delight]\ncommand = {}\nargs = [\"mcp\"]\n",
            serde_json::to_string(&launcher).unwrap()
        ),
    );
    let shadow = fx.root.join("shadow");
    w(&shadow.join("terminal-delight"), "#!/bin/sh\nexit 1\n");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        shadow.join("terminal-delight"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fx.env.retain(|(k, _)| k != "PATH");
    fx.env.push(("PATH".into(), shadow.display().to_string()));
    let p = fx.plan(Retention::Keep);
    let entries: Vec<_> = p
        .steps
        .iter()
        .filter(|s| matches!(s.op, Op::McpRemove { .. }))
        .collect();
    assert_eq!(entries.len(), 3);
    for client in ["claude", "codex", "gemini"] {
        assert!(entries.iter().any(|s| matches!(&s.op,
            Op::McpRemove { client: c, command, .. }
            if c == client && Path::new(command) == launcher)));
    }
    assert!(entries.iter().all(|s| !s.why.contains("PATH")));
    let mut effects = fake(true);
    let rc = apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut effects,
    );
    assert!(rc.refused.is_none());
    assert_eq!(
        rc.outcomes
            .iter()
            .filter(|o| o.op.contains(" mcp remove ") && o.status == Status::Done)
            .count(),
        3
    );
    for client in ["claude", "codex", "gemini"] {
        assert!(effects
            .ran
            .iter()
            .any(|a| a.contains(&format!("{client} mcp remove"))));
    }
}

#[test]
fn a_running_appimage_in_the_builds_directory_is_never_signalled() {
    let fx = Fx::new("runningimage");
    let image = fx
        .root
        .join(".local/lib/terminal-delight/td-other.AppImage");
    let mut bytes = b"\x7fELF\x02\x01\x01\x00AI\x02".to_vec();
    bytes.extend_from_slice(&[0u8; 32]);
    std::fs::write(&image, &bytes).unwrap();
    let mut f = fake(false);
    f.procs.push(Proc {
        pid: 31,
        exe: image.clone(),
        argv: vec![],
        comm: String::new(),
        kind: ExeKind::AppImage,
    });
    let p = fx.plan(Retention::Keep);
    apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut f,
    );
    assert!(
        f.signalled.iter().all(|(pid, _)| *pid != 31),
        "{:?}",
        f.signalled
    );
}

#[test]
fn an_unreadable_tree_has_no_size_rather_than_zero() {
    use std::os::unix::fs::PermissionsExt;
    let fx = Fx::new("unreadable");
    let state = fx.root.join(".local/state/terminal-delight/locked");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (bytes, files) = super::plan::dir_size(&fx.root.join(".local/state/terminal-delight"));
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Running as root reads anything; then there is nothing to prove here.
    if unsafe { libc::geteuid() } != 0 {
        assert_eq!((bytes, files), (None, None));
    }
}

#[test]
fn a_program_in_the_builds_directory_that_cannot_be_read_blocks_instead_of_being_signalled() {
    // An AppImage launched from TD's builds directory and then deleted: the
    // path still names a td- file there, and nothing can show what it was.
    let fx = Fx::new("deletedimage");
    let gone = fx
        .root
        .join(".local/lib/terminal-delight/td-other.AppImage");
    let mut f = fake(false);
    f.procs.push(Proc {
        pid: 32,
        exe: gone,
        argv: vec![],
        comm: String::new(),
        kind: ExeKind::Unknown,
    });
    let p = fx.plan(Retention::Keep);
    let rc = apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut f,
    );
    assert!(
        f.signalled.iter().all(|(pid, _)| *pid != 32),
        "{:?}",
        f.signalled
    );
    assert!(rc
        .outcomes
        .iter()
        .any(|o| o.status == Status::Failed && o.detail.contains("left alone")));
    assert!(fx
        .root
        .join(".local/lib/terminal-delight/td-bbb-old")
        .exists());
}

#[test]
fn only_the_running_programs_own_bytes_make_it_a_build() {
    let kind = |b: &[u8]| ExeKind::of_reader(&mut std::io::Cursor::new(b)).unwrap();
    assert_eq!(kind(b"#!/bin/sh\n"), ExeKind::Other);
    assert_eq!(kind(b""), ExeKind::Other);
    assert_eq!(kind(b"\x7fELF\x02\x01\x01\x00\x00\x00\x00"), ExeKind::Build);
    for (what, bytes) in appimages() {
        assert_eq!(kind(&bytes), ExeKind::AppImage, "{what}");
    }
}

#[test]
fn a_records_folder_that_cannot_be_looked_at_is_unknown_not_absent() {
    use std::os::unix::fs::PermissionsExt;
    let fx = Fx::new("noaccess");
    let base = fx.root.join(".local/state");
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o000)).unwrap();
    let p = fx.plan(Retention::Keep);
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o755)).unwrap();
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let records = p.sizes.iter().find(|z| z.what == "records").unwrap();
    assert_eq!((records.bytes, records.present), (None, None));
    let gone = p.sizes.iter().find(|z| z.what == "data").unwrap();
    if gone.bytes.is_none() {
        assert_eq!(gone.present, Some(false));
    }
}

#[test]
fn a_bare_mcp_command_says_which_path_found_it() {
    let fx = Fx::new("mcpbare");
    w(
        &fx.root.join(".claude.json"),
        r#"{"mcpServers":{"terminal-delight":{"command":"terminal-delight","args":["mcp"]}}}"#,
    );
    let p = fx.plan(Retention::Keep);
    let steps: Vec<_> = p
        .steps
        .iter()
        .filter(|s| matches!(s.op, Op::McpRemove { .. }))
        .collect();
    assert!(!steps.is_empty());
    for s in steps {
        assert!(s.why.contains("as found on the PATH"), "{}", s.why);
    }
}

#[test]
fn a_launcher_pointing_at_a_persons_own_program_in_the_builds_directory_keeps_both() {
    let fx = Fx::new("ownwrapper");
    let lib = fx.root.join(".local/lib/terminal-delight");
    let wrapper = lib.join("custom-wrapper");
    elf(&wrapper);
    let launcher = fx.root.join(".local/bin/terminal-delight");
    std::fs::remove_file(&launcher).unwrap();
    std::os::unix::fs::symlink(&wrapper, &launcher).unwrap();
    let p = fx.plan(Retention::Keep);
    assert!(p
        .steps
        .iter()
        .all(|s| !matches!(&s.op, Op::RemoveLink { path, .. } if *path == launcher)));
    for s in &p.steps {
        if let Op::StopProcesses { exes, .. } = &s.op {
            assert!(!exes.contains(&wrapper), "{exes:?}");
        }
    }
    let mut f = fake(false);
    f.procs.push(Proc {
        pid: 33,
        exe: wrapper.clone(),
        argv: vec![],
        comm: String::new(),
        kind: ExeKind::Build,
    });
    apply(
        &p,
        &fx.plan(Retention::Keep),
        &fx.roots(),
        &out(&fx),
        &mut f,
    );
    assert!(
        f.signalled.iter().all(|(pid, _)| *pid != 33),
        "{:?}",
        f.signalled
    );
    assert!(std::fs::symlink_metadata(&launcher).is_ok());
    assert!(wrapper.exists());
}

/// A reader whose first bytes are an ELF header and whose ISO descriptor
/// read fails, as a damaged block would.
struct FailsAtTheVolume(std::io::Cursor<Vec<u8>>);
impl std::io::Read for FailsAtTheVolume {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        if self.0.position() >= 32769 {
            return Err(std::io::Error::other("damaged block"));
        }
        self.0.read(b)
    }
}
impl std::io::Seek for FailsAtTheVolume {
    fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.seek(to)
    }
}

#[test]
fn a_classifier_that_cannot_finish_reading_proves_nothing() {
    let mut bytes = b"\x7fELF\x01\x01\x01\x00\x00\x00\x00".to_vec();
    bytes.resize(40000, 0);
    let err = ExeKind::of_reader(&mut FailsAtTheVolume(std::io::Cursor::new(bytes)));
    assert!(err.is_err());
    // And the planner's own reading of a file it cannot open is no kind at all.
    use std::os::unix::fs::PermissionsExt;
    let fx = Fx::new("unrdbld");
    let f = fx.root.join(".local/lib/terminal-delight/td-locked");
    elf(&f);
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o000)).unwrap();
    let kind = super::plan::exe_kind_of(&f);
    let p = fx.plan(Retention::Keep);
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    assert_eq!(kind, None);
    assert!(p.steps.iter().all(
        |s| !matches!(&s.op, Op::RemoveFile { path, .. } | Op::RemoveBuild { path } if *path == f)
    ));
}

#[test]
fn the_worker_removes_a_build_only_while_its_bytes_still_say_build() {
    let fx = Fx::new("rebldck");
    let b = fx.root.join(".local/lib/terminal-delight/td-bbb-old");
    let p = fx.plan(Retention::Keep);
    assert!(p
        .steps
        .iter()
        .any(|s| matches!(&s.op, Op::RemoveBuild { path } if *path == b)));
    // Between the review and the run it became an AppImage.
    let mut image = b"\x7fELF\x02\x01\x01\x00AI\x02".to_vec();
    image.extend_from_slice(&[0u8; 32]);
    std::fs::write(&b, &image).unwrap();
    let mut f = fake(false);
    apply(&p, &p.clone(), &fx.roots(), &out(&fx), &mut f);
    assert!(b.exists());
}
