//! One cancellable speculative render per window process. The last nine
//! visited panes are candidates, never nine resident browser pages.
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use super::{
    cache,
    engine::{layout_hash, Geometry, PageEngine, PageRequest},
    snapshot::EXTRACT_VERSION,
};
use gpui::{App, Global};

const RECENT: usize = 9;
const SETTLE: Duration = Duration::from_millis(400);

#[derive(Clone, PartialEq)]
struct Candidate {
    pane: u64,
    path: PathBuf,
    geometry: Geometry,
}
struct Pending {
    candidate: Candidate,
    offered: Instant,
    checked: Option<Instant>,
    stamp: Option<Option<(u64, std::time::SystemTime)>>,
}
#[derive(Default)]
struct Queue {
    recent: VecDeque<Pending>,
    stopped: bool,
}
impl Queue {
    fn offer(&mut self, candidate: Candidate) -> bool {
        if self
            .recent
            .front()
            .is_some_and(|p| p.candidate == candidate)
        {
            return false;
        }
        self.recent.retain(|p| p.candidate.pane != candidate.pane);
        self.recent.push_front(Pending {
            candidate,
            offered: Instant::now(),
            checked: None,
            stamp: None,
        });
        self.recent.truncate(RECENT);
        true
    }
}
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    foreground: AtomicUsize,
    epoch: AtomicU64,
}
struct Preparation(Arc<Shared>);
impl Global for Preparation {}
impl Drop for Preparation {
    fn drop(&mut self) {
        self.0
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stopped = true;
        self.0.epoch.fetch_add(1, Ordering::SeqCst);
        self.0.wake.notify_all();
    }
}

fn state(cx: &mut App) -> Arc<Shared> {
    if let Some(p) = cx.try_global::<Preparation>() {
        return p.0.clone();
    }
    let shared = Arc::new(Shared {
        queue: Mutex::new(Queue::default()),
        wake: Condvar::new(),
        foreground: AtomicUsize::new(0),
        epoch: AtomicU64::new(0),
    });
    cx.set_global(Preparation(shared.clone()));
    shared
}

pub struct Foreground(Arc<Shared>);
impl Drop for Foreground {
    fn drop(&mut self) {
        self.0.foreground.fetch_sub(1, Ordering::SeqCst);
        self.0.wake.notify_all();
    }
}
pub fn foreground(cx: &mut App) -> Foreground {
    let shared = state(cx);
    shared.foreground.fetch_add(1, Ordering::SeqCst);
    shared.epoch.fetch_add(1, Ordering::SeqCst);
    Foreground(shared)
}

/// Offered from the focused pane with the same geometry its opening path uses.
/// No filesystem access occurs on the UI thread.
pub fn offer(pane: u64, path: PathBuf, geometry: Geometry, cx: &mut App) {
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
    {
        return;
    }
    let shared = state(cx);
    let candidate = Candidate {
        pane,
        path,
        geometry,
    };
    let mut q = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
    let start = q.recent.is_empty();
    if !q.offer(candidate) {
        return;
    }
    shared.epoch.fetch_add(1, Ordering::SeqCst);
    drop(q);
    if start {
        if let Ok(engine) = super::engine(cx) {
            let worker = shared.clone();
            if let Err(e) = std::thread::Builder::new()
                .name("td-page-prepare".into())
                .spawn(move || run(worker, engine))
            {
                shared
                    .queue
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .recent
                    .clear();
                eprintln!("terminal-delight: cannot start page preparation: {e}");
            }
        } else {
            shared
                .queue
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .recent
                .clear();
        }
    }
    shared.wake.notify_all();
}

fn run(shared: Arc<Shared>, engine: Arc<dyn PageEngine>) {
    loop {
        let mut q = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        if q.stopped {
            return;
        }
        let next = if shared.foreground.load(Ordering::SeqCst) == 0 {
            q.recent
                .iter_mut()
                .find(|p| {
                    p.offered.elapsed() >= SETTLE
                        && p.checked
                            .is_none_or(|at| at.elapsed() > Duration::from_secs(2))
                })
                .map(|p| {
                    p.checked = Some(Instant::now());
                    (p.candidate.clone(), p.stamp)
                })
        } else {
            None
        };
        let Some((candidate, previous)) = next else {
            drop(
                shared
                    .wake
                    .wait_timeout(q, SETTLE)
                    .unwrap_or_else(|e| e.into_inner()),
            );
            continue;
        };
        let epoch = shared.epoch.load(Ordering::SeqCst);
        drop(q);
        let stamp = std::fs::metadata(&candidate.path)
            .ok()
            .and_then(|m| Some((m.len(), m.modified().ok()?)));
        if previous == Some(stamp) {
            continue;
        }
        let cancelled = || {
            shared.epoch.load(Ordering::SeqCst) != epoch
                || shared.foreground.load(Ordering::SeqCst) != 0
        };
        let started = Instant::now();
        let result = prepare(&*engine, &candidate, &cancelled);
        {
            let mut q = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(p) = q.recent.iter_mut().find(|p| p.candidate == candidate) {
                if cancelled() {
                    p.checked = None;
                } else if result != "memory-pressure" {
                    p.stamp = Some(stamp);
                }
            }
        }
        if std::env::var_os("TD_DOCDEBUG").is_some() {
            eprintln!(
                "[doc-prepare] {}",
                serde_json::json!({"path":candidate.path,"result":result,"ms":started.elapsed().as_secs_f64()*1000.0})
            );
        }
    }
}

struct OpenPage<'a> {
    engine: &'a dyn PageEngine,
    id: super::engine::PageId,
}
impl Drop for OpenPage<'_> {
    fn drop(&mut self) {
        self.engine.close(self.id);
    }
}

fn prepare(engine: &dyn PageEngine, c: &Candidate, cancelled: &dyn Fn() -> bool) -> &'static str {
    // Limits bound speculative work; foreground opens remain unrestricted.
    let available = std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| {
        s.lines().find_map(|l| {
            l.strip_prefix("MemAvailable:")
                .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        })
    });
    if !available.is_some_and(|kb| kb >= 1024 * 1024) {
        return "memory-pressure";
    }
    if !std::fs::metadata(&c.path).is_ok_and(|m| m.len() <= 32 << 20) {
        return "source-limit";
    }
    let Ok(bytes) = std::fs::read(&c.path) else {
        return "unreadable";
    };
    let hash = layout_hash(&bytes);
    let key = cache::key(&c.path, hash, c.geometry, engine.name(), EXTRACT_VERSION);
    let root = cache::root();
    if cache::lookup(&root, key).is_some() {
        return "hit";
    }
    if cancelled() {
        return "cancelled";
    }
    let Ok(mut layout) = engine.open(&PageRequest {
        path: c.path.clone(),
        geometry: c.geometry,
        expect: hash,
    }) else {
        return "open-failed";
    };
    let Some(id) = layout.page else {
        return "no-page";
    };
    let page = OpenPage { engine, id };
    if u64::from(c.geometry.height_dev(layout.height_css))
        * u64::from(c.geometry.height_dev(c.geometry.css_width as f32))
        > 64 * 1024 * 1024
    {
        return "pixel-limit";
    }
    if let Some(videos) = layout.videos.as_deref_mut() {
        layout.diagnostics.extend(cache::keep_clips(&root, videos));
    }
    let mut tiles = Vec::new();
    let mut bytes = 0;
    for band in layout.bands() {
        if cancelled() {
            return "cancelled";
        }
        let Ok(tile) = engine.tile(id, layout.generation, band) else {
            return "capture-failed";
        };
        bytes += tile.png.len();
        if bytes > 64 << 20 {
            return "png-limit";
        }
        tiles.push((band, tile.png));
    }
    if cancelled() {
        return "cancelled";
    }
    let Some(dependencies) = engine.dependencies(id) else {
        return "dependencies-unknown";
    };
    drop(page);
    if cancelled() {
        return "cancelled";
    }
    let borrowed: Vec<_> = tiles.iter().map(|(b, p)| (*b, &p[..])).collect();
    if cache::store_validated(&root, key, &layout, &borrowed, &dependencies).is_err() {
        return "store-failed";
    }
    let _ = cache::sweep(&root, cache::CAP_BYTES);
    "stored"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(pane: u64) -> Candidate {
        Candidate {
            pane,
            path: format!("/briefs/{pane}.html").into(),
            geometry: Geometry {
                css_width: 968,
                viewport_css_height: 1400,
                scale: 1.6,
            },
        }
    }

    #[test]
    fn fifty_panes_keep_only_nine_candidates_and_repaints_do_not_postpone_them() {
        let mut q = Queue::default();
        for pane in 0..50 {
            assert!(q.offer(candidate(pane)));
        }
        assert_eq!(
            q.recent
                .iter()
                .map(|p| p.candidate.pane)
                .collect::<Vec<_>>(),
            (41..50).rev().collect::<Vec<_>>()
        );
        let when = q.recent[0].offered;
        for _ in 0..100 {
            assert!(!q.offer(candidate(49)));
        }
        assert_eq!(q.recent[0].offered, when);
        assert!(q.offer(candidate(44)));
        assert_eq!(q.recent[0].candidate.pane, 44);
        assert_eq!(q.recent.len(), 9);
        let mut resized = candidate(44);
        resized.geometry.css_width = 1200;
        assert!(q.offer(resized));
        assert_eq!(
            q.recent.iter().filter(|p| p.candidate.pane == 44).count(),
            1
        );
    }

    #[gpui::test]
    fn a_foreground_open_cancels_preparation_until_the_last_open_finishes(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let shared = state(cx);
            let epoch = shared.epoch.load(Ordering::SeqCst);
            let a = foreground(cx);
            let b = foreground(cx);
            assert_ne!(shared.epoch.load(Ordering::SeqCst), epoch);
            assert_eq!(shared.foreground.load(Ordering::SeqCst), 2);
            drop(a);
            assert_eq!(shared.foreground.load(Ordering::SeqCst), 1);
            drop(b);
            assert_eq!(shared.foreground.load(Ordering::SeqCst), 0);
        });
    }
}
