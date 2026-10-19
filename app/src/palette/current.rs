//! The selected Omarchy palette, watched independently of the installed shelf.
//! Compare bytes at the path, not the inode or mtime: Omarchy replaces a staged
//! directory, and copies may preserve timestamps. All polling I/O is off-thread.

#[cfg(test)]
mod tests;

use super::{from_source, Palette};
use gpui::{App, Global};
use std::{path::PathBuf, time::Duration};

struct Current {
    last_good: Option<Palette>,
    name: Option<String>,
    available: bool,
}
impl Global for Current {}

/// The last complete, validated palette. Absence means no valid load yet.
pub fn current(cx: &App) -> Option<&Palette> {
    cx.try_global::<Current>()?.last_good.as_ref()
}

/// Actual source status; kept separate from the saved wish to follow it.
pub fn current_status(cx: &App) -> (bool, Option<&str>) {
    match cx.try_global::<Current>() {
        Some(c) => (c.available, c.name.as_deref()),
        None => (false, None),
    }
}

#[derive(Clone)]
struct Roots {
    primary: PathBuf,
    legacy: PathBuf,
}
impl Roots {
    fn installed() -> Option<Self> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        Some(Self {
            primary: home.join(".local/state/omarchy"),
            legacy: home.join(".config/omarchy"),
        })
    }

    fn snapshot(&self) -> Snapshot {
        // A temporarily missing/invalid current directory does not authorize
        // silently changing to an older installation's palette.
        let root = if self.primary.exists() {
            &self.primary
        } else {
            &self.legacy
        };
        let dir = root.join("current");
        Snapshot {
            source: std::fs::read_to_string(dir.join("theme/colors.toml")).ok(),
            name: std::fs::read_to_string(dir.join("theme.name"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        }
    }
}

#[derive(PartialEq, Eq)]
struct Snapshot {
    source: Option<String>,
    name: Option<String>,
}
impl Current {
    fn accept(&mut self, snap: &Snapshot) {
        let palette = snap
            .source
            .as_deref()
            .and_then(|src| from_source("omarchy-current", src));
        self.available = palette.is_some();
        if let Some(p) = palette {
            self.last_good = Some(p);
            self.name = snap.name.clone();
        }
    }
}

pub(super) fn init(cx: &mut App) {
    if let Some(roots) = Roots::installed() {
        watch(cx, roots);
    }
}

fn watch(cx: &mut App, roots: Roots) {
    let mut last = roots.snapshot();
    let mut current = Current {
        last_good: None,
        name: None,
        available: false,
    };
    current.accept(&last);
    cx.set_global(current);
    cx.spawn(async move |cx| loop {
        let executor = cx.background_executor().clone();
        executor.timer(Duration::from_millis(300)).await;
        let roots = roots.clone();
        let (now, changed) = executor
            .spawn(async move {
                let now = roots.snapshot();
                let changed = (now != last).then(|| {
                    let mut current = Current {
                        last_good: None,
                        name: None,
                        available: false,
                    };
                    current.accept(&now);
                    current
                });
                (now, changed)
            })
            .await;
        last = now;
        if let Some(next) = changed {
            cx.update(|cx| {
                let current = cx.global_mut::<Current>();
                current.available = next.available;
                if next.available {
                    current.last_good = next.last_good;
                    current.name = next.name;
                }
                crate::theme::palettes_changed(cx);
            });
        }
    })
    .detach();
}
