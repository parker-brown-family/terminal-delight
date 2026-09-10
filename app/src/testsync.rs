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
