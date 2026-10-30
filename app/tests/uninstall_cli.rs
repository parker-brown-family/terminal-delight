//! CLI boundary: unsafe/unknown operations never fall through to a window.
use std::process::Command;

#[test]
fn uninstall_and_admin_dispatch_need_no_home_display_or_running_host() {
    for (args, expected) in [
        (vec!["uninstall", "help"], 0),
        (vec!["uninstall", "remove"], 2),
        (vec!["uninstall", "preview", "--apply"], 2),
        (vec!["uninstall", "preview", "--json"], 2),
        (vec!["uninstall", "run"], 2),
        (
            vec![
                "uninstall",
                "run",
                "--plan",
                "/nonexistent/uninstall-plan.json",
            ],
            2,
        ),
        (vec!["admin", "help"], 0),
        (vec!["admin", "uninstall"], 2),
        (vec!["admin", "protect", "--force"], 2),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_terminal-delight"))
            .args(&args)
            .env_clear()
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(expected), "{args:?}");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(combined.contains(args[0]), "{args:?}: {combined}");
        assert!(!combined.contains("panicked"), "{args:?}: {combined}");
    }
}
