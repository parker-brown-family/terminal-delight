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
