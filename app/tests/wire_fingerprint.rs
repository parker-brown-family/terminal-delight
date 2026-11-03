//! The updater's wire fingerprint (`build.rs`) has to read the code it claims
//! to cover. It once stopped at a file's first `#[cfg(test)]`, which in
//! `vt/mod.rs` is an attribute on `mod lab;` at line 34, so a build whose
//! `Term::advance` misread every byte still reported the same wire and would
//! have been offered as "Restart window" (issue 1267). These tests read the
//! build script's own `normalise` over the real sources.
#[allow(dead_code)]
#[path = "../build.rs"]
mod build_script;

use build_script::normalise;

fn source(rel: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Code after an early or indented `#[cfg(test)]` is still code.
#[test]
fn the_fingerprint_reads_past_a_cfg_test_that_is_not_the_test_module() {
    let core = normalise(&source("vt/mod.rs"));
    assert!(
        core.contains("pub fn advance(&mut self, bytes: &[u8]) {"),
        "Term::advance is outside the wire fingerprint"
    );
    let types = normalise(&source("vt/types.rs"));
    assert!(
        types.contains("pub fn id(&self) -> &str {"),
        "the second half of vt/types.rs is outside the wire fingerprint"
    );
}

/// A new test, or a reworded comment, does not move the wire.
#[test]
fn a_test_module_and_comments_leave_the_fingerprint_alone() {
    let text = source("vt/pump.rs");
    let with_tests = format!("{text}\n#[cfg(test)]\nmod tests {{\n    fn added() {{}}\n}}\n");
    assert_eq!(normalise(&with_tests), normalise(&text));
    let commented = text.replacen('\n', "\n// a new comment\n", 1);
    assert_eq!(normalise(&commented), normalise(&text));
}
