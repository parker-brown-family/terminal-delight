//! Serialisation between tests that cannot safely overlap.
//!
//! A `fork` duplicates every open descriptor the parent holds, so for the
//! instant between fork and exec the child owns the parent's file locks too.
//! A lock the parent has just released is therefore not free until that child
//! has exec'd — and a test that releases a lock and immediately asserts it is
//! free will occasionally find it held by a child belonging to an entirely
//! unrelated test running on another thread.
//!
//! That is not hypothetical. Adding tests that start real pseudoterminals made
//! the session-ownership tests fail three runs in eight, while the same suite
//! with the pane tests skipped passed eight in eight. The tests that appeared
//! broken were the ones that had not changed.
//!
//! So: anything that forks takes this guard across the fork, and anything that
//! asserts on lock ownership takes it for its duration. It is deliberately not
//! a lock around "the filesystem" or "the environment" — naming what it
//! actually protects is what stops it becoming a mutex everything grabs.
//!
//! **It cannot be complete,** because the code under test starts processes too:
//! the engstate tests run `git` dozens of times without it, and beside them the
//! instance tests' "released on drop" failed in 14 runs of 150 (#908). So a test
//! that drops a descriptor and asserts what the kernel says about it next waits
//! for the close to land, through [`soon`], and a descriptor never closed still
//! fails it.

use std::sync::{Mutex, MutexGuard, OnceLock};

/// Held across a fork, and by any test asserting who owns a lock.
pub fn forks_and_locks() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        // A test that panicked while holding it poisoned it; the next test
        // still needs the guard, and the panic has already been reported.
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether `done` comes true within two seconds: for asserting what follows a
/// dropped descriptor, a lock that is free again or a socket that is gone.
///
/// The drop closes the descriptor at once in this process, and not always in
/// the kernel's eyes: a test on another thread that starts a process hands the
/// child a copy of every open descriptor until the child execs, and the lock or
/// the socket lives until the last copy closes. Measured on 2026-10-07 (#908),
/// 150 runs of the instance tests each: alone, "released on drop" never failed;
/// beside the engstate tests it failed in 14; beside the channel tests, in 1.
/// A descriptor that is never closed still fails, two seconds later.
#[cfg(test)]
pub fn soon(mut done: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if done() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Write a program a test is about to run, without this process ever holding
/// it open for writing.
///
/// The kernel refuses to run a file anything has open for writing (`ETXTBSY`,
/// "Text file busy"). `fs::write` closes its descriptor before returning, but a
/// test on another thread that forks in between has a copy of it until its own
/// exec, so the run straight after is refused now and then: twice in CI, on
/// 2026-11-17 and 2026-11-25. A shell of its own holds it here instead, and
/// that has ended before this returns.
#[cfg(test)]
pub fn write_program(path: &std::path::Path, text: &str) {
    use std::io::Write as _;
    let mut sh = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"cat > "$0" && chmod 755 "$0""#)
        .arg(path)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("a shell to write the program");
    sh.stdin
        .take()
        .expect("its input")
        .write_all(text.as_bytes())
        .expect("the program's text");
    let ended = sh.wait().expect("the shell to end");
    assert!(ended.success(), "writing {} failed", path.display());
}

/// A directory of this test's own, taken away when the test ends.
///
/// Removed on **drop**, not at the end of a test body, and that is the whole
/// point: a test that fails leaves by panicking, so tidying written after the
/// assertions is skipped exactly when there is most to skip. Every copy of this
/// helper in the suite tidied at the end of the body, or not at all, and `/tmp`
/// held thirteen thousand of their leftovers before anybody counted.
///
/// Named per process AND per thread, because the suite runs tests in parallel
/// and two of them sharing a tag would otherwise share a directory.
///
/// Derefs to `Path`, so a call site reads exactly as it did when this returned
/// a `PathBuf` — with one trap that is worth knowing about, because four tests
/// hit it the moment this landed. A guard used inline,
/// `Scratch::new("x").join("f")`, is dropped at the end of that statement and
/// takes the directory with it before the next line runs. Bind it to a name
/// first. The failure is loud when the test touches the directory afterwards
/// and silent when it does not, which is the only reason this note exists.
#[cfg(test)]
pub struct Scratch {
    root: std::path::PathBuf,
}

#[cfg(test)]
impl Scratch {
    pub fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "td-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch directory");
        Self { root }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.root
    }
}

#[cfg(test)]
impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.root
    }
}

#[cfg(test)]
impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.root
    }
}

#[cfg(test)]
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Runs `write`-then-run over and over while other threads fork, and counts
    /// the runs the kernel refused because the file was still open for writing.
    fn refused(write: impl Fn(&std::path::Path), rounds: usize) -> usize {
        let dir = std::env::temp_dir().join(format!(
            "td-etxtbsy-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let stop = AtomicBool::new(false);
        let count = std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    while !stop.load(Ordering::Relaxed) {
                        let _ = std::process::Command::new("/bin/true").status();
                    }
                });
            }
            let mut count = 0;
            for n in 0..rounds {
                let program = dir.join(format!("p{n}"));
                write(&program);
                if let Err(e) = std::process::Command::new(&program).status() {
                    assert_eq!(e.raw_os_error(), Some(26), "only ETXTBSY is counted: {e}");
                    count += 1;
                }
            }
            stop.store(true, Ordering::Relaxed);
            count
        });
        let _ = std::fs::remove_dir_all(&dir);
        count
    }

    /// A program written this way can always be run at once, whatever the rest
    /// of the suite is starting.
    #[test]
    fn a_program_written_for_a_test_is_never_busy() {
        assert_eq!(refused(|p| write_program(p, "#!/bin/sh\nexit 0\n"), 150), 0);
    }

    /// The control, run by hand: the ordinary way is refused often enough here
    /// for the test above to mean something. Not in the suite, because what it
    /// asserts is a race.
    #[test]
    #[ignore = "asserts a race; run by hand as the control"]
    fn a_program_written_by_this_process_is_sometimes_busy() {
        use std::os::unix::fs::PermissionsExt as _;
        let busy = refused(
            |p| {
                std::fs::write(p, "#!/bin/sh\nexit 0\n").unwrap();
                std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
            },
            600,
        );
        eprintln!("refused {busy} of 600");
        assert!(busy > 0);
    }
}
