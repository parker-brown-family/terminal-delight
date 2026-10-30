//! CLI boundary: unsafe/unknown operations never fall through to a window.
use std::process::Command;

#[test]
fn uninstall_dispatch_needs_no_home_display_or_running_host() {
    for (args, expected) in [
        (vec!["uninstall", "help"], 0),
        (vec!["uninstall", "remove"], 2),
        (vec!["uninstall", "preview", "--apply"], 2),
        (vec!["uninstall", "preview", "--json"], 2),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_terminal-delight"))
            .args(args)
            .env_clear()
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(expected));
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(combined.contains("uninstall"));
        assert!(!combined.contains("panicked"));
    }
}
