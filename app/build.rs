//! What a build knows about itself, for `--build-info` and the UPDATES panel.
//!
//! Before this existed a binary knew only `CARGO_PKG_VERSION`, which reads the
//! same in a window three days of dev commits later, so the only record of
//! which commit was running was the name a person had given the file. Four
//! facts are embedded here, and each one is allowed to be unknown:
//!
//! - `TD_BUILD_SHA`: the commit. `TD_BUILD_SHA` in the environment wins, so a
//!   build from a tarball can still name its commit; otherwise
//!   `git rev-parse HEAD`. Absent when neither answers.
//! - `TD_BUILD_DATE`: that commit's committer date, ISO 8601. Absent likewise.
//! - `TD_BUILD_DIRTY`: `1` when `app/` had uncommitted changes, `0` when git
//!   said it had none. Absent when git could not be asked.
//! - `TD_BUILD_WIRE`: a fingerprint of everything a window and a session host
//!   must agree on to share terminals (see [`wire_fingerprint`]). Always set.
//! - `TD_BUILD_WIRE_PROTO`: the part of that fingerprint a replay cannot test,
//!   the protocol modules alone (see [`proto_fingerprint`]). Always set.
//!
//! An unset fact is left unset rather than written as an empty string or a
//! zero, so the binary can say "unknown" instead of inventing an answer.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:rerun-if-env-changed=TD_BUILD_SHA");
    println!("cargo:rerun-if-env-changed=TD_BUILD_DATE");
    println!("cargo:rerun-if-env-changed=TD_BUILD_DIRTY");

    let git = |args: &[&str]| -> Option<String> {
        let out = Command::new("git")
            .args(args)
            .current_dir(&manifest)
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    // Rerun when HEAD moves, so a commit is never reported by a binary built
    // before it, and when the app's own sources or manifest change, so the
    // dirty flag is never left saying "clean" about an edited tree. Changing
    // a source recompiles the crate anyway, so the second costs nothing. The
    // git paths come from git itself, because in a worktree `.git` is a file.
    println!("cargo:rerun-if-changed={}", manifest.join("src").display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("Cargo.toml").display()
    );
    if let Some(p) = git(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={}", manifest.join(p).display());
    }
    if let Some(head_ref) = git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(p) = git(&["rev-parse", "--git-path", &head_ref]) {
            println!("cargo:rerun-if-changed={}", manifest.join(p).display());
        }
    }

    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let sha = env("TD_BUILD_SHA").or_else(|| git(&["rev-parse", "HEAD"]));
    let date = env("TD_BUILD_DATE").or_else(|| git(&["log", "-1", "--format=%cI", "HEAD"]));
    // Dirty means the app's sources differ from the commit: an edited README
    // elsewhere in the repository does not change the binary.
    let dirty = env("TD_BUILD_DIRTY").or_else(|| {
        git(&["status", "--porcelain", "--untracked-files=no", "--", "."])
            .map(|s| if s.is_empty() { "0" } else { "1" }.to_string())
    });
    if let Some(sha) = sha.filter(|s| s.len() >= 7 && s.bytes().all(|b| b.is_ascii_hexdigit())) {
        println!("cargo:rustc-env=TD_BUILD_SHA={sha}");
    }
    if let Some(date) = date {
        println!("cargo:rustc-env=TD_BUILD_DATE={date}");
    }
    if let Some(dirty) = dirty.filter(|d| d == "0" || d == "1") {
        println!("cargo:rustc-env=TD_BUILD_DIRTY={dirty}");
    }

    let core = if std::env::var_os("CARGO_FEATURE_CORE_ALACRITTY").is_some() {
        "alacritty"
    } else {
        "rio"
    };
    println!(
        "cargo:rustc-env=TD_BUILD_WIRE={}",
        wire_fingerprint(&manifest, core)
    );
    println!(
        "cargo:rustc-env=TD_BUILD_WIRE_PROTO={}",
        proto_fingerprint(&manifest)
    );
}

/// The files the window and host exchange messages through, rather than the
/// terminal core both of them parse a byte stream with: the control protocol,
/// the snapshot encoding and its grid hash, pictures, and the close report the
/// pump sends.
const PROTO_FILES: [&str; 4] = [
    "hostproto.rs",
    "gridwire.rs",
    "picturewire.rs",
    "vt/pump.rs",
];

/// A short hash of [`PROTO_FILES`], normalised as [`wire_fingerprint`] is.
///
/// A replay of a real host's capture (`scripts/wire-attest.sh`) proves that a
/// new core reads an old host's bytes into the same screen. It says nothing
/// about a changed control verb or a changed snapshot format, so CI attests a
/// build only against one whose protocol fingerprint is equal to its own.
fn proto_fingerprint(manifest: &Path) -> String {
    let src = manifest.join("src");
    let files: Vec<PathBuf> = PROTO_FILES.iter().map(|f| src.join(f)).collect();
    let h = hash_sources(&src, &files);
    let digest = h.finalize();
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Each file's path under `src` and its normalised text, in the given order.
fn hash_sources(src: &Path, files: &[PathBuf]) -> Sha256 {
    let mut h = Sha256::new();
    for f in files {
        println!("cargo:rerun-if-changed={}", f.display());
        let name = f
            .strip_prefix(src)
            .unwrap_or(f)
            .to_string_lossy()
            .to_string();
        h.update(name.as_bytes());
        h.update([0]);
        let text = std::fs::read_to_string(f).unwrap_or_default();
        h.update(normalise(&text).as_bytes());
        h.update([0]);
    }
    h
}

/// A short hash of what decides whether a new window can attach to an old
/// session host without either of them misreading the other.
///
/// The protocol version alone is not enough. It has stayed at 1 while the
/// grid encoding and the terminal core moved underneath it: rio-vt 0.5.29
/// changed how a synchronized update is parsed, and a window and host that
/// parse one byte stream differently disagree about the screen while both
/// speak protocol 1. So the fingerprint covers the wire modules, the terminal
/// core boundary (`src/vt/*.rs`, without the PTY, socket and lab files, which
/// move bytes and define nothing the wire carries; the pump stays in, because
/// the close report a host sends a window is defined there), the core's
/// locked version, and which core is compiled in.
///
/// Comments, blank space and each file's trailing `#[cfg(test)]` module are
/// left out, so a reworded comment or a new test does not turn an ordinary
/// update into one that ends somebody's terminals. Everything else counts:
/// when unsure, two builds are called different, which costs a full restart
/// the person is asked about, never a session that silently misreads.
fn wire_fingerprint(manifest: &Path, core: &str) -> String {
    let src = manifest.join("src");
    let mut files: Vec<PathBuf> = ["hostproto.rs", "gridwire.rs", "picturewire.rs"]
        .iter()
        .map(|f| src.join(f))
        .collect();
    if let Ok(entries) = std::fs::read_dir(src.join("vt")) {
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let moves_bytes = matches!(name, "pty.rs" | "socket.rs" | "lab.rs");
            if p.extension().is_some_and(|x| x == "rs") && !moves_bytes {
                files.push(p);
            }
        }
    }
    files.sort();
    let mut h = hash_sources(&src, &files);
    let lock = manifest.join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let lock = std::fs::read_to_string(lock).unwrap_or_default();
    let crate_version = |name: &str| -> String {
        let key = format!("name = \"{name}\"\n");
        lock.find(&key)
            .and_then(|i| lock[i + key.len()..].lines().next())
            .unwrap_or("")
            .to_string()
    };
    h.update(
        format!(
            "core={core};rio-vt {};alacritty_terminal {}",
            crate_version("rio-vt"),
            crate_version("alacritty_terminal")
        )
        .as_bytes(),
    );
    let digest = h.finalize();
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Source text with comments, blank lines and the trailing test module removed.
///
/// The test module is a `#[cfg(test)]` at the start of a line with a `mod`
/// opening on the next. Any other `#[cfg(test)]` (an indented one inside an
/// `impl`, or one on a `mod lab;` near the top) is kept as code, and so is
/// everything after it. Stopping at the first one, as this once did, left
/// most of `vt/mod.rs` and half of `vt/types.rs` out of the fingerprint, so a
/// build that parsed every byte differently still reported the same wire.
pub(crate) fn normalise(text: &str) -> String {
    let mut out = String::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let t = line.trim();
        let test_module = line.starts_with("#[cfg(test)]")
            && lines
                .peek()
                .is_some_and(|next| next.starts_with("mod ") && next.trim_end().ends_with('{'));
        if test_module {
            break;
        }
        let code = match t.find("//") {
            Some(i) => t[..i].trim_end(),
            None => t,
        };
        if !code.is_empty() {
            out.push_str(code);
            out.push('\n');
        }
    }
    out
}
