//! The snapshot engine against a real Chromium.
//!
//! The unit tests in `docview/cdp.rs` hold the DevTools client to a fake
//! browser on a socket pair. These hold the engine to the real one: the
//! protocol names, the flags, the pipe, the page probe and the captures are
//! only right if a Chromium agrees.
//!
//! **No Chromium, no run — and it says so.** Each test looks for a browser the
//! way the engine does (chromium, chromium-browser, google-chrome-stable,
//! google-chrome on PATH) and, finding none, prints `SKIPPED` with the sentence
//! a person would see and returns. A machine without a browser cannot tell us
//! anything about one, and it should not turn CI red for lacking one. The
//! same goes for a browser that is there but cannot start its sandbox, which
//! is GitHub's Ubuntu 24.04 runner: AppArmor forbids the user namespaces the
//! sandbox needs, and TD never runs a browser without it.
//!
//! **Nothing started here outlives the test.** Every browser a test launches
//! is recorded with its whole process tree while it runs; at the end the
//! engine is shut down and every one of those pids must be gone, and the
//! profile directory with them. Other Chromium processes on the machine are
//! never looked at.
//!
//! The engine's source is compiled into this test directly (the binary crate
//! has no library to link), which is why nothing under `docview/` other than
//! the view itself may name `crate::`.

#![allow(dead_code)]

#[path = "../src/docview"]
mod docview {
    pub mod cdp;
    pub mod engine;
    pub mod notes;
    pub mod pref;
    pub mod snapshot;
}

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use docview::engine::{
    bands, layout_hash, png_size, ConcurSupport, EngineError, Geometry, PageEngine, PageLayout,
    PageRequest,
};
use docview::notes;
use docview::pref::Html;
use docview::snapshot::SnapshotEngine;

/// One browser at a time: each is 265–570 MiB, and the cleanup check reads
/// the process table, which a neighbour's browser would only muddy.
static SERIAL: Mutex<()> = Mutex::new(());

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/page")
        .join(name)
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("td-snapshot-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The browser the engine would use, or a printed reason there is none.
///
/// Written straight to the process's stderr rather than through `eprintln!`,
/// which the test harness captures and shows only for a failure: a skip is
/// exactly the passing test whose reason a CI log has to show.
fn chromium(test: &str) -> Option<PathBuf> {
    let say = |line: String| {
        let _ = writeln!(std::io::stderr().lock(), "{line}");
    };
    match SnapshotEngine::locate(&Html::default(), std::env::var_os("PATH").as_deref()) {
        Ok(p) => {
            say(format!("snapshot_engine::{test}: driving {}", p.display()));
            Some(p)
        }
        Err(why) => {
            say(format!(
                "SKIPPED snapshot_engine::{test}: {} (these tests drive a real Chromium)",
                why.sentence()
            ));
            None
        }
    }
}

/// Every process under `root`, root included, from /proc.
fn tree(root: u32) -> Vec<u32> {
    let mut procs = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for e in entries.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                continue;
            };
            let Some(after) = stat.rfind(')').map(|i| &stat[i + 2..]) else {
                continue;
            };
            let ppid = after
                .split(' ')
                .nth(1)
                .and_then(|p| p.parse::<u32>().ok())
                .unwrap_or(0);
            procs.push((pid, ppid));
        }
    }
    let mut out = vec![root];
    let mut grew = true;
    while grew {
        grew = false;
        for (pid, ppid) in &procs {
            if !out.contains(pid) && out.contains(ppid) {
                out.push(*pid);
                grew = true;
            }
        }
    }
    out
}

/// Running, as opposed to gone or a zombie waiting to be reaped.
fn running(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| s.rfind(')').map(|i| s[i + 2..].starts_with('Z')))
        .is_some_and(|zombie| !zombie)
}

struct Rig {
    engine: SnapshotEngine,
    runtime: PathBuf,
    seen: Vec<u32>,
    profiles: Vec<PathBuf>,
    _serial: std::sync::MutexGuard<'static, ()>,
}

impl Rig {
    fn new(test: &str, pref: Html, idle: Duration) -> Option<Rig> {
        let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let binary = chromium(test)?;
        let runtime = scratch(&format!("{test}-runtime"));
        let rig = Rig {
            engine: SnapshotEngine::with_idle(pref, binary, idle, runtime.clone()),
            runtime,
            seen: Vec::new(),
            profiles: Vec::new(),
            _serial: serial,
        };
        // A browser that is there but cannot start its sandbox — Ubuntu
        // 24.04's AppArmor forbids the user namespaces it needs, as on
        // GitHub's runners — is a machine that cannot run a browser safely,
        // and says nothing about TD. TD never launches one without its
        // sandbox, so these tests do not either: they skip, and say why.
        // Any other failure to start is a failure.
        match rig.engine.warm() {
            Ok(()) => Some(rig),
            Err(EngineError::Launch(why)) if why.contains("No usable sandbox") => {
                let said = why
                    .find("No usable sandbox")
                    .map_or(why.as_str(), |i| &why[i..]);
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "SKIPPED snapshot_engine::{test}: the browser cannot start its sandbox here: {}",
                    said.chars().take(160).collect::<String>()
                );
                let _ = std::fs::remove_dir_all(&rig.runtime);
                None
            }
            Err(e) => panic!("the browser would not start: {e}"),
        }
    }

    fn record(&mut self) {
        if let Some(pid) = self.engine.browser_pid() {
            for p in tree(pid) {
                if !self.seen.contains(&p) {
                    self.seen.push(p);
                }
            }
        }
        if let Some(dir) = self.engine.profile_dir() {
            if !self.profiles.contains(&dir) {
                self.profiles.push(dir);
            }
        }
    }

    fn open(&mut self, path: &Path, g: Geometry) -> Result<PageLayout, EngineError> {
        let bytes = std::fs::read(path).unwrap();
        let out = self.engine.open(&PageRequest {
            path: path.to_path_buf(),
            geometry: g,
            expect: layout_hash(&bytes),
        });
        self.record();
        out
    }

    /// Shut down, and prove it: every pid this test saw is gone and every
    /// profile it made is removed.
    fn done(mut self) {
        self.record();
        assert!(
            !self.seen.is_empty(),
            "the test never saw a browser, so it proved nothing"
        );
        self.engine.shutdown();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut left: Vec<u32> = self.seen.clone();
        while Instant::now() < deadline {
            left.retain(|p| running(*p));
            if left.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            left.is_empty(),
            "processes this test started are still running: {left:?}"
        );
        for p in &self.profiles {
            assert!(!p.exists(), "profile {} was left behind", p.display());
        }
        let _ = std::fs::remove_dir_all(&self.runtime);
    }
}

const NARROW: Geometry = Geometry {
    css_width: 600,
    viewport_css_height: 800,
    scale: 1.6,
};

#[test]
fn a_brief_renders_to_anchors_and_tiles_in_one_pass() {
    let Some(mut rig) = Rig::new("anchors", Html::default(), Duration::from_secs(300)) else {
        return;
    };
    let brief = fixture("brief.html");
    let layout = rig.open(&brief, NARROW).expect("the fixture renders");
    #[derive(serde::Deserialize)]
    struct Want {
        nid: String,
        title: String,
        dialog: Option<String>,
    }
    let want: Vec<Want> =
        serde_json::from_str(&std::fs::read_to_string(fixture("anchors.json")).unwrap()).unwrap();
    let got: Vec<(&str, &str, Option<&str>)> = layout
        .anchors
        .iter()
        .map(|a| (a.nid.as_str(), a.title.as_str(), a.dialog.as_deref()))
        .collect();
    let expected: Vec<(&str, &str, Option<&str>)> = want
        .iter()
        .map(|w| (w.nid.as_str(), w.title.as_str(), w.dialog.as_deref()))
        .collect();
    assert_eq!(got, expected, "the page's own tagging, in document order");
    for a in &layout.anchors {
        match a.dialog {
            // Inside a closed dialog nothing is laid out: no place, not zero.
            Some(_) => assert!(
                a.rect.is_none(),
                "{} has a rect inside a closed dialog",
                a.nid
            ),
            None => {
                let r = a.rect.unwrap_or_else(|| panic!("{} has no rect", a.nid));
                assert!(r.y >= 0.0 && r.y < layout.height_css, "{}: {r:?}", a.nid);
                assert!(
                    a.button.is_some(),
                    "{}: the page's own (hidden) note button keeps its box",
                    a.nid
                );
            }
        }
    }
    assert_eq!(layout.capability.notes_islands, 1);
    assert_eq!(layout.capability.tagged, 8);
    assert_eq!(layout.notes_file.as_deref(), Some("brief.html"));
    assert_eq!(layout.dialogs, vec!["d-evidence".to_string()]);
    assert!(layout
        .openers
        .iter()
        .any(|o| o.dialog == "d-evidence" && o.rect.is_some()));
    let jump = layout
        .links
        .iter()
        .find(|l| l.href == "#sec-2")
        .expect("the fragment link");
    let sec2 = layout
        .anchors
        .iter()
        .find(|a| a.nid == "finding-anchors-hold")
        .unwrap();
    assert_eq!(
        jump.fragment_top_css,
        sec2.rect.map(|r| r.y),
        "a fragment link knows where it lands"
    );
    // A fragment arriving from another document lands in the same place a
    // link on the page itself does, and finds an <a name> as a browser does.
    assert_eq!(layout.fragment_top_css("sec-2"), jump.fragment_top_css);
    let more = layout
        .fragment_top_css("more")
        .expect("an <a name> is a place a fragment can name");
    assert!(more > 0.0 && Some(more) < jump.fragment_top_css, "{more}");
    assert_eq!(
        layout.fragment_top_css("d-evidence"),
        None,
        "a closed dialog is named, and is not a place"
    );
    assert_eq!(layout.fragment_top_css("report-notes"), None);
    assert_eq!(layout.fragment_top_css("no-such-id"), None);
    assert_eq!(layout.fragment_top_css("top"), Some(0.0));
    let back = layout
        .links
        .iter()
        .find(|l| l.href == "#top")
        .expect("the link back to the top");
    assert_eq!(
        back.fragment_top_css,
        Some(0.0),
        "#top with nothing named top is the top of the page, as in a browser"
    );
    assert!(layout
        .links
        .iter()
        .any(|l| l.href == "https://example.com/brief" && l.fragment_top_css.is_none()));

    let page = layout.page.expect("a live page");
    let height_dev = layout.geometry.height_dev(layout.height_css);
    let all = bands(height_dev);
    assert!(
        all.len() >= 3,
        "the fixture is long enough for several bands: {height_dev} rows"
    );
    for band in &all {
        let tile = rig
            .engine
            .tile(page, layout.generation, *band)
            .expect("a tile");
        assert_eq!(png_size(&tile.png), Some((tile.width_dev, tile.height_dev)));
        assert_eq!(tile.width_dev, 960, "600 CSS px at scale 1.6");
        assert_eq!(tile.height_dev, band.height_dev, "band at {}", band.top_dev);
    }
    // Pixels and anchors come from one page instance: a later generation's
    // tiles are never handed out for this layout.
    assert_eq!(
        rig.engine
            .tile(page, layout.generation + 1000, all[0])
            .err(),
        Some(EngineError::Stale)
    );
    rig.engine.close(page);

    // And every brief in the skill's shared fixtures, each laid out by its
    // own notes.js: the ids and titles the skill's check recorded in a
    // browser, a concur space exactly where the browser made one, the page's
    // NOTES_FILE, and whether the brief takes concurs at all.
    for (name, case) in shared_cases() {
        let dir = scratch(&format!("anchors-{name}"));
        let brief = dir.join("brief.html");
        std::fs::copy(case.join("brief.html"), &brief).unwrap();
        let layout = rig
            .open(&brief, NARROW)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let want = case_anchors(&case);
        let got: Vec<(String, String, bool)> = layout
            .anchors
            .iter()
            .map(|a| (a.nid.clone(), a.title.clone(), a.concur_zone.is_some()))
            .collect();
        let expected: Vec<(String, String, bool)> = want
            .iter()
            .map(|w| (w.nid.clone(), w.title.clone(), w.concurrable))
            .collect();
        assert_eq!(got, expected, "{name}: anchors, in document order");
        let expect = case_expect(&case);
        assert_eq!(
            layout.notes_file.as_deref(),
            expect["notes_file"].as_str(),
            "{name}: NOTES_FILE"
        );
        let support = match expect["concur_support"].as_str() {
            Some("supported") => ConcurSupport::Supported,
            Some("unsupported") => ConcurSupport::NotSupported,
            _ => ConcurSupport::Unknown,
        };
        assert_eq!(layout.capability.concur, support, "{name}: concurs");
        if let Some(page) = layout.page {
            rig.engine.close(page);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    rig.done();
}

// ── the notes, against the skill's shared fixtures ───────────────────────────

fn shared_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/decision-brief/notes-format")
}

/// Every case the skill's fixtures carry, sorted by name.
fn shared_cases() -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(shared_fixtures().join("cases"))
        .expect("the vendored fixtures")
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    out.sort();
    assert_eq!(out.len(), 15);
    out
}

#[derive(serde::Deserialize)]
struct CaseAnchor {
    nid: String,
    title: String,
    concurrable: bool,
}

fn case_anchors(case: &Path) -> Vec<CaseAnchor> {
    serde_json::from_slice(&std::fs::read(case.join("anchors.json")).unwrap()).unwrap()
}

fn case_expect(case: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(case.join("expect.json")).unwrap()).unwrap()
}

/// A file as the disk has it: its bytes, its length and when it changed.
fn stat(path: &Path) -> (Vec<u8>, u64, std::time::SystemTime) {
    let meta = std::fs::metadata(path).unwrap();
    (
        std::fs::read(path).unwrap(),
        meta.len(),
        meta.modified().unwrap(),
    )
}

/// Opening a brief, reading its notes and closing it again writes nothing:
/// for every brief in the shared fixtures, and for every brief the skill
/// writes, the file's bytes and its modification time afterwards are the
/// ones it had before. The notes are read the way the view reads them —
/// from the bytes the render was made from — and the map built from them
/// with the page's own anchors and NOTES_FILE is, byte for byte, what the
/// brief's own copy map gave in a browser.
#[test]
fn opening_reading_and_closing_a_brief_leaves_its_bytes_alone() {
    let Some(mut rig) = Rig::new("read-only", Html::default(), Duration::from_secs(300)) else {
        return;
    };
    let mut maps = 0;
    for (name, case) in shared_cases() {
        for file in ["brief.html", "expected.html"] {
            let Ok(original) = std::fs::read(case.join(file)) else {
                continue;
            };
            let dir = scratch(&format!("read-{name}-{file}"));
            let brief = dir.join("brief.html");
            std::fs::write(&brief, &original).unwrap();
            // A second older than now, so a write in the same instant
            // could not hide behind a coarse clock.
            let old = std::time::SystemTime::now() - Duration::from_secs(60);
            std::fs::File::options()
                .write(true)
                .open(&brief)
                .unwrap()
                .set_modified(old)
                .unwrap();
            let before = stat(&brief);

            let bytes = std::fs::read(&brief).unwrap();
            let read = notes::read(&bytes);
            let layout = rig
                .open(&brief, NARROW)
                .unwrap_or_else(|e| panic!("{name} {file}: {e}"));
            if file == "expected.html" {
                let label = layout.notes_file.clone().expect("notes.js ran");
                let pairs: Vec<(&str, &str)> = layout
                    .anchors
                    .iter()
                    .map(|a| (a.nid.as_str(), a.title.as_str()))
                    .collect();
                let concurs = match layout.capability.concur {
                    ConcurSupport::Supported => read.concurs.clone().unwrap(),
                    _ => notes::ConcurMap::default(),
                };
                let notes = read.notes.clone().expect("an island").expect("readable");
                let map = notes::build_map(&label, &notes, &concurs, &pairs);
                let want = std::fs::read_to_string(case.join("expected-map.txt")).unwrap();
                assert_eq!(map, want, "{name}: the map, through the engine");
                maps += 1;
            }
            if let Some(page) = layout.page {
                // A dialog opened and closed on the way, as a reader would.
                for id in &layout.dialogs {
                    let _ = rig.engine.dialog(page, layout.generation, id);
                }
                rig.engine.close(page);
            }
            let after = stat(&brief);
            assert!(
                before == after,
                "{name} {file}: the file changed: {} bytes then {}, modified {:?} then {:?}",
                before.1,
                after.1,
                before.2,
                after.2
            );
            assert_eq!(after.0, original, "{name} {file}: not one byte moved");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    assert_eq!(maps, 11);
    rig.done();
}

/// The picture of a brief is the same whatever notes it holds: the page's
/// own notes chrome is hidden, and so is the rule it draws beside an anchor
/// with notes, because TD draws its own from the file. So a saved note moves
/// nothing in the picture and nothing in the layout, and a render stays true
/// after a save. Measured on the skill's own pair: a brief with empty
/// islands and the same brief after two notes and two concurs were written.
#[test]
fn a_briefs_notes_are_not_in_the_picture() {
    let Some(mut rig) = Rig::new("notes-picture", Html::default(), Duration::from_secs(300)) else {
        return;
    };
    let case = shared_fixtures().join("cases/current-pristine");
    let mut shots = Vec::new();
    for file in ["brief.html", "expected.html"] {
        let dir = scratch(&format!("picture-{file}"));
        let brief = dir.join("brief.html");
        std::fs::copy(case.join(file), &brief).unwrap();
        let layout = rig.open(&brief, NARROW).unwrap();
        let page = layout.page.unwrap();
        let tiles: Vec<Vec<u8>> = bands(layout.geometry.height_dev(layout.height_css))
            .into_iter()
            .map(|b| rig.engine.tile(page, layout.generation, b).unwrap().png)
            .collect();
        rig.engine.close(page);
        shots.push((layout, tiles));
        let _ = std::fs::remove_dir_all(&dir);
    }
    let ((empty, empty_tiles), (noted, noted_tiles)) = (&shots[0], &shots[1]);
    assert_eq!(empty.height_css, noted.height_css);
    let rects = |l: &PageLayout| {
        l.anchors
            .iter()
            .map(|a| (a.nid.clone(), a.rect, a.button, a.concur_zone))
            .collect::<Vec<_>>()
    };
    assert_eq!(rects(empty), rects(noted), "a saved note moved an anchor");
    assert_eq!(empty_tiles.len(), noted_tiles.len());
    for (i, (a, b)) in empty_tiles.iter().zip(noted_tiles).enumerate() {
        assert!(
            a == b,
            "band {i} differs between the brief with no notes and the brief with two ({} vs {} bytes)",
            a.len(),
            b.len()
        );
    }
    rig.done();
}

/// Bands meet with no gap and no overlap only if every capture comes back
/// exactly as many rows as it was asked for, at scales that do not divide
/// 2,048 evenly as well as at 1.6, which does.
#[test]
fn a_tile_is_exactly_the_band_it_was_asked_for_at_any_scale() {
    let Some(mut rig) = Rig::new("scales", Html::default(), Duration::from_secs(300)) else {
        return;
    };
    for scale in [1.0f32, 1.25, 1.5, 2.0] {
        let g = Geometry { scale, ..NARROW };
        let layout = rig.open(&fixture("brief.html"), g).unwrap();
        let page = layout.page.unwrap();
        let all = bands(g.height_dev(layout.height_css));
        for (i, band) in all.iter().enumerate() {
            let t = rig.engine.tile(page, layout.generation, *band).unwrap();
            assert_eq!(t.width_dev, (600.0 * scale).round() as u32, "scale {scale}");
            // The page ends on a fractional device row at some scales, and
            // Chromium stops at the last whole one: the final band may be a
            // row short, which is why the view draws a tile at the height its
            // PNG says rather than the height it asked for. Every other band
            // is exact, or two bands would overlap or gap at their seam.
            let last = i + 1 == all.len();
            let short = band.height_dev - t.height_dev;
            assert!(
                short == 0 || (last && short == 1),
                "scale {scale}, band at {}: asked {} rows, got {}",
                band.top_dev,
                band.height_dev,
                t.height_dev
            );
        }
        rig.engine.close(page);
    }
    rig.done();
}

#[test]
fn anchor_ids_do_not_depend_on_the_width() {
    let Some(mut rig) = Rig::new("widths", Html::default(), Duration::from_secs(300)) else {
        return;
    };
    let brief = fixture("brief.html");
    let wide = rig
        .open(
            &brief,
            Geometry {
                css_width: 968,
                viewport_css_height: 1400,
                scale: 1.6,
            },
        )
        .unwrap();
    let narrow = rig.open(&brief, NARROW).unwrap();
    let ids = |l: &PageLayout| l.anchors.iter().map(|a| a.nid.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&wide), ids(&narrow));
    // The rects are not width-invariant, and must not be reused across widths.
    let x = |l: &PageLayout| l.anchors[0].rect.map(|r| r.w);
    assert_ne!(
        x(&wide),
        x(&narrow),
        "the same anchor is laid out differently at each width"
    );
    rig.done();
}

#[test]
fn the_pages_own_notes_ui_is_not_in_the_picture() {
    let Some(mut rig) = Rig::new("chrome", Html::default(), Duration::from_secs(300)) else {
        return;
    };
    let with = fixture("brief.html");
    let src = std::fs::read_to_string(&with).unwrap();
    // The same page with the notebar and the note buttons taken out of the
    // markup altogether.
    let mut without = String::new();
    let mut rest = src.as_str();
    for (open, close) in [
        ("<!--NOTES-CHROME-->", "<!--/NOTES-CHROME-->"),
        ("/*NOTES-CHROME*/", "/*/NOTES-CHROME*/"),
    ] {
        let a = rest.find(open).expect("the fixture's markers");
        let b = rest.find(close).expect("the fixture's markers") + close.len();
        without.push_str(&rest[..a]);
        rest = &rest[b..];
    }
    without.push_str(rest);
    let dir = scratch("chrome");
    let bare = dir.join("brief.html");
    std::fs::write(&bare, without).unwrap();

    let a = rig.open(&with, NARROW).unwrap();
    let b = rig.open(&bare, NARROW).unwrap();
    assert_eq!(
        a.height_css, b.height_css,
        "hiding the notes UI moved nothing"
    );
    let first = bands(a.geometry.height_dev(a.height_css))[0];
    let shot_a = rig
        .engine
        .tile(a.page.unwrap(), a.generation, first)
        .unwrap();
    let shot_b = rig
        .engine
        .tile(b.page.unwrap(), b.generation, first)
        .unwrap();
    // The notebar sits over the first viewport and the buttons over every
    // card: with them drawn, these differ in thousands of bytes.
    assert!(
        shot_a.png == shot_b.png,
        "the first band differs from the same page with no notes UI ({} vs {} bytes)",
        shot_a.png.len(),
        shot_b.png.len()
    );
    let _ = std::fs::remove_dir_all(&dir);
    rig.done();
}

#[test]
fn a_briefs_confirm_does_not_hang_the_engine() {
    let Some(mut rig) = Rig::new("confirm", Html::default(), Duration::from_secs(300)) else {
        return;
    };
    let dir = scratch("confirm");
    let page = dir.join("confirm.html");
    // notes.js's ✕ asks confirm('Delete every note in this brief?'); an
    // unanswered one blocks the page's main thread, and the load with it.
    std::fs::write(
        &page,
        "<!DOCTYPE html><html><body><p>before</p><script>\
         var ok = confirm('Delete every note in this brief?'); alert('and this');\
         var d = document.createElement('div'); d.className = 'notable';\
         d.dataset.nid = 'confirm-' + ok; d.dataset.ntitle = 'answered'; d.textContent = 'x';\
         document.body.appendChild(d);</script></body></html>",
    )
    .unwrap();
    let started = Instant::now();
    let layout = rig
        .open(&page, NARROW)
        .expect("a page that asks still renders");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
    assert_eq!(
        layout.anchors.first().map(|a| a.nid.as_str()),
        Some("confirm-false"),
        "dismissed, not accepted"
    );
    let _ = std::fs::remove_dir_all(&dir);
    rig.done();
}

/// A listener that answers every connection with a 404 and counts them.
struct Listener {
    port: u16,
    seen: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Listener {
    fn start() -> Listener {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.set_nonblocking(true).unwrap();
        let port = l.local_addr().unwrap().port();
        let (seen, stop) = (
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicBool::new(false)),
        );
        let thread = {
            let (seen, stop) = (seen.clone(), stop.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match l.accept() {
                        Ok((mut s, _)) => {
                            seen.fetch_add(1, Ordering::SeqCst);
                            let _ = s.set_read_timeout(Some(Duration::from_millis(200)));
                            let mut buf = [0u8; 2048];
                            let _ = s.read(&mut buf);
                            let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(10)),
                    }
                }
            })
        };
        Listener {
            port,
            seen,
            stop,
            thread: Some(thread),
        }
    }

    fn page(&self, dir: &Path) -> PathBuf {
        let p = self.port;
        let path = dir.join(format!("net-{p}.html"));
        std::fs::write(
            &path,
            format!(
                "<!DOCTYPE html><html><head>\
                 <link rel=\"preconnect\" href=\"http://127.0.0.1:{p}\">\
                 <link rel=\"dns-prefetch\" href=\"http://localhost:{p}\">\
                 <link rel=\"stylesheet\" href=\"http://127.0.0.1:{p}/a.css\">\
                 <script async src=\"http://localhost:{p}/s.js\"></script>\
                 </head><body><img src=\"http://127.0.0.1:{p}/x.png\" alt=\"\">\
                 <script>fetch('http://127.0.0.1:{p}/f', {{ mode: 'no-cors' }}).catch(function () {{}});\
                 try {{ new WebSocket('ws://127.0.0.1:{p}/w'); }} catch (e) {{}}\
                 navigator.sendBeacon && navigator.sendBeacon('http://127.0.0.1:{p}/b', 'x');</script>\
                 </body></html>"
            ),
        )
        .unwrap();
        path
    }

    fn connections_after(&self, wait: Duration) -> usize {
        std::thread::sleep(wait);
        self.seen.load(Ordering::SeqCst)
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[test]
fn the_page_cannot_reach_the_network() {
    let dir = scratch("network");
    {
        let Some(mut rig) = Rig::new("network", Html::default(), Duration::from_secs(300)) else {
            return;
        };
        let listener = Listener::start();
        let page = listener.page(&dir);
        rig.open(&page, NARROW)
            .expect("the page renders, without what it asked for");
        assert_eq!(
            listener.connections_after(Duration::from_millis(1500)),
            0,
            "a page in the engine reached a listener on this machine"
        );
        rig.done();
    }
    // The control: the same page with the network allowed does reach it, so
    // the zero above is the block working and not a listener nobody could hit.
    let Some(mut rig) = Rig::new(
        "network-allowed",
        Html {
            network: Some("allowed".into()),
            ..Default::default()
        },
        Duration::from_secs(300),
    ) else {
        return;
    };
    let listener = Listener::start();
    let page = listener.page(&dir);
    let _ = rig.open(&page, NARROW);
    assert!(
        listener.connections_after(Duration::from_millis(1500)) > 0,
        "with the network allowed the page should have reached the listener"
    );
    let _ = std::fs::remove_dir_all(&dir);
    rig.done();
}

#[test]
fn a_dialog_is_drawn_on_its_own_with_its_close_button_and_links() {
    let Some(mut rig) = Rig::new("dialog", Html::default(), Duration::from_secs(300)) else {
        return;
    };
    let layout = rig.open(&fixture("brief.html"), NARROW).unwrap();
    let page = layout.page.unwrap();
    let d = rig
        .engine
        .dialog(page, layout.generation, "d-evidence")
        .expect("the dialog renders");
    assert_eq!(png_size(&d.png), Some((d.width_dev, d.height_dev)));
    // 560 CSS px or 92% of the view, whichever is less, at scale 1.6.
    assert!((880..=900).contains(&d.width_dev), "{}", d.width_dev);
    assert_eq!(d.closers.len(), 1, "the dialog's own × button");
    assert!(
        d.grew_viewport,
        "the body scrolls at 800 px tall, so the viewport grew to show it all"
    );
    let inside: Vec<&str> = d.anchors.iter().map(|a| a.nid.as_str()).collect();
    assert_eq!(inside, vec!["sub-first-source", "sub-second-source"]);
    let height_css = d.height_dev as f32 / 1.6;
    for a in &d.anchors {
        let r = a.rect.expect("laid out while open");
        assert!(
            r.y >= 0.0 && r.y + r.h <= height_css + 1.0,
            "{} relative to the dialog: {r:?}",
            a.nid
        );
    }
    assert!(d
        .links
        .iter()
        .any(|l| l.href == "https://example.com/source" && l.rect.is_some()));
    // Asked for a dialog the page does not have, the engine says so.
    assert!(matches!(
        rig.engine.dialog(page, layout.generation, "d-nothing"),
        Err(EngineError::Page(_))
    ));
    // The dialog was closed again: the page's tiles are unchanged by it.
    let first = bands(layout.geometry.height_dev(layout.height_css))[0];
    assert!(rig.engine.tile(page, layout.generation, first).is_ok());
    rig.done();
}

#[test]
fn an_idle_browser_is_closed_and_its_profile_removed() {
    let Some(mut rig) = Rig::new("idle", Html::default(), Duration::from_secs(2)) else {
        return;
    };
    let brief = fixture("brief.html");
    let layout = rig.open(&brief, NARROW).unwrap();
    let pid = rig.engine.browser_pid().expect("a browser runs");
    let profile = rig.engine.profile_dir().expect("with a profile");
    assert!(profile.starts_with(&rig.runtime) && profile.is_dir());
    rig.engine.close(layout.page.unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    while rig.engine.browser_pid().is_some() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        rig.engine.browser_pid(),
        None,
        "two idle seconds should have closed it"
    );
    let tree_gone = Instant::now() + Duration::from_secs(5);
    while running(pid) && Instant::now() < tree_gone {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!running(pid), "the browser process is gone");
    let removed = Instant::now() + Duration::from_secs(5);
    while profile.exists() && Instant::now() < removed {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!profile.exists(), "and its profile with it");
    // The next document starts a new one.
    rig.open(&brief, NARROW).unwrap();
    assert!(rig.engine.browser_pid().is_some_and(|p| p != pid));
    rig.done();
}

#[test]
fn a_file_that_changed_since_it_was_read_is_not_rendered_as_if_it_had_not() {
    let Some(mut rig) = Rig::new("changed", Html::default(), Duration::from_secs(300)) else {
        return;
    };
    let brief = fixture("brief.html");
    let stale = rig.engine.open(&PageRequest {
        path: brief.clone(),
        geometry: NARROW,
        expect: layout_hash(b"the bytes somebody read before an agent rewrote the file"),
    });
    rig.record();
    assert_eq!(stale.err(), Some(EngineError::FileChanged));
    rig.open(&brief, NARROW).expect("read fresh, it renders");
    rig.done();
}

// ── notes, written ───────────────────────────────────────────────────────────

/// A case's edits, as TD's writer takes them.
fn case_edits(case: &Path) -> Vec<notes::NoteEdit> {
    let v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(case.join("edits.json")).unwrap()).unwrap();
    let s = |e: &serde_json::Value, k: &str| e[k].as_str().map(str::to_string);
    v["edits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let nid = s(e, "nid").unwrap();
            match e["op"].as_str().unwrap() {
                "add" => notes::NoteEdit::Add {
                    nid,
                    title: s(e, "title").unwrap(),
                    text: s(e, "text").unwrap(),
                    ts: s(e, "ts").unwrap(),
                },
                "delete" => notes::NoteEdit::Delete {
                    nid,
                    text: s(e, "text").unwrap(),
                    ts: s(e, "ts"),
                },
                "concur" => notes::NoteEdit::Concur {
                    nid,
                    ts: s(e, "ts").unwrap(),
                },
                _ => notes::NoteEdit::Unconcur { nid },
            }
        })
        .collect()
}

/// The anchors whose place differs between two layouts of one page by more
/// than half a CSS pixel, or that one of them lacks.
fn moved(before: &PageLayout, after: &PageLayout) -> Vec<String> {
    let close =
        |a: Option<docview::engine::RectCss>, b: Option<docview::engine::RectCss>| match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                (a.x - b.x).abs() <= 0.5
                    && (a.y - b.y).abs() <= 0.5
                    && (a.w - b.w).abs() <= 0.5
                    && (a.h - b.h).abs() <= 0.5
            }
            _ => false,
        };
    let mut out: Vec<String> = before
        .anchors
        .iter()
        .filter(|a| {
            !after.anchors.iter().any(|b| {
                b.nid == a.nid
                    && close(a.rect, b.rect)
                    && close(a.button, b.button)
                    && close(a.concur_zone, b.concur_zone)
            })
        })
        .map(|a| a.nid.clone())
        .collect();
    out.extend(
        after
            .anchors
            .iter()
            .filter(|b| !before.anchors.iter().any(|a| a.nid == b.nid))
            .map(|b| b.nid.clone()),
    );
    if (before.height_css - after.height_css).abs() > 0.5 {
        out.push(format!(
            "(height {} -> {})",
            before.height_css, after.height_css
        ));
    }
    out
}

/// TD writes each case's edits into a copy of its brief with the whole
/// commit — backup, temporary file, rename, bytes read back — then the
/// engine opens the written file fresh, and the brief's own notes.js shows a
/// note on exactly the anchors written and a stamp on exactly the decisions
/// concurred. And not one anchor moved: the page lays out the same with the
/// notes as without them.
#[test]
fn a_saved_note_is_read_back_as_written_and_moves_no_anchor() {
    let Some(mut rig) = Rig::new("write-back", Html::default(), Duration::from_secs(300)) else {
        return;
    };
    let mut checked = 0;
    for (name, case) in shared_cases() {
        let expect = case_expect(&case);
        if expect["refuse"].is_string() {
            continue;
        }
        let dir = scratch(&format!("write-{name}"));
        let brief = dir.join("brief.html");
        std::fs::copy(case.join("brief.html"), &brief).unwrap();
        let before = rig.open(&brief, NARROW).unwrap();
        if let Some(p) = before.page {
            rig.engine.close(p);
        }
        let bytes = std::fs::read(&brief).unwrap();
        let pairs: Vec<(&str, &str)> = before
            .anchors
            .iter()
            .map(|a| (a.nid.as_str(), a.title.as_str()))
            .collect();
        let rev = notes::iso_millis(std::time::SystemTime::now());
        let plan = notes::plan_write(
            &bytes,
            &case_edits(&case),
            &notes::WriteArgs {
                anchors: &pairs,
                label: before.notes_file.as_deref().unwrap(),
                concurs: before.capability.concur == ConcurSupport::Supported,
                rev: &rev,
                path: &brief,
            },
        )
        .unwrap_or_else(|r| panic!("{name}: {}", r.sentence()));
        notes::verify(&bytes, &plan).unwrap();
        let backups = notes::Backups {
            dir: dir.join("state/brief-backups/x"),
            keep: notes::KEEP_BACKUPS,
        };
        let written = notes::commit(&brief, &bytes, &plan, &backups).unwrap();
        assert_eq!(std::fs::read(&written.backup).unwrap(), bytes);
        let back = rig.engine.read_back(&brief, NARROW).unwrap();
        rig.record();
        notes::confirm(&plan, &back.anchors)
            .unwrap_or_else(|why| panic!("{name}: the written file shows otherwise: {why}"));
        assert_eq!(
            moved(&before, &back),
            Vec::<String>::new(),
            "{name}: a saved note moved an anchor"
        );
        assert_eq!(
            back.rendered, before.rendered,
            "{name}: the layout's hash kept"
        );
        checked += 1;
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert_eq!(checked, 11);
    rig.done();
}

/// Least-confident decision 2, measured on Parker's own briefs: a COPY of
/// every brief in the report archive gets a note on its first anchor (and a
/// stamp on its first decision, where it takes one), written by TD's whole
/// commit; the copy is read back through the engine, and every anchor is
/// compared with the render of the copy before the note. The originals are
/// only ever read.
///
/// Ignored in the ordinary run: it reads folders only this machine has.
/// `TD_ARCHIVE` names them, colon-separated; unset, the two report folders.
///
///   cargo test --test snapshot_engine -- --ignored --nocapture archive
#[test]
#[ignore]
fn a_saved_note_moves_no_anchor_across_the_report_archive() {
    let Some(mut rig) = Rig::new("archive", Html::default(), Duration::from_secs(600)) else {
        return;
    };
    let home = std::env::var("HOME").unwrap();
    let dirs = std::env::var("TD_ARCHIVE")
        .unwrap_or_else(|_| format!("{home}/Work/terminal-delight/reports:{home}/Work/reports"));
    let mut briefs: Vec<PathBuf> = dirs
        .split(':')
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|r| r.flatten().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "html"))
        .collect();
    briefs.sort();
    let (mut measured, mut moved_in, mut skipped, mut unshown) =
        (0, Vec::new(), Vec::new(), Vec::new());
    let wide = Geometry {
        css_width: 968,
        viewport_css_height: 1400,
        scale: 1.6,
    };
    for (i, original) in briefs.iter().enumerate() {
        let before_bytes = std::fs::read(original).unwrap();
        let dir = scratch(&format!("archive-{i}"));
        // The same name, so NOTES_FILE and the storage key are the brief's own.
        let copy = dir.join(original.file_name().unwrap());
        std::fs::write(&copy, &before_bytes).unwrap();
        let before = match rig.open(&copy, wide) {
            Ok(l) => l,
            Err(e) => {
                skipped.push(format!("{}: {e}", original.display()));
                let _ = std::fs::remove_dir_all(&dir);
                continue;
            }
        };
        if let Some(p) = before.page {
            rig.engine.close(p);
        }
        let first = before.anchors.first();
        let decision = before.anchors.iter().find(|a| a.concur_zone.is_some());
        let Some(first) = first else {
            skipped.push(format!("{}: no anchors", original.display()));
            let _ = std::fs::remove_dir_all(&dir);
            continue;
        };
        let mut edits = vec![notes::NoteEdit::Add {
            nid: first.nid.clone(),
            title: first.title.clone(),
            text: "A note written by the anchor measurement.".into(),
            ts: notes::utc_minute(std::time::SystemTime::now()),
        }];
        let supported = before.capability.concur == ConcurSupport::Supported;
        if let (true, Some(d)) = (supported, decision) {
            edits.push(notes::NoteEdit::Concur {
                nid: d.nid.clone(),
                ts: notes::utc_minute(std::time::SystemTime::now()),
            });
        }
        let pairs: Vec<(&str, &str)> = before
            .anchors
            .iter()
            .map(|a| (a.nid.as_str(), a.title.as_str()))
            .collect();
        let rev = notes::iso_millis(std::time::SystemTime::now());
        let label = before.notes_file.clone().unwrap_or_default();
        let plan = match notes::plan_write(
            &before_bytes,
            &edits,
            &notes::WriteArgs {
                anchors: &pairs,
                label: &label,
                concurs: supported,
                rev: &rev,
                path: &copy,
            },
        ) {
            Ok(p) => p,
            Err(r) => {
                skipped.push(format!("{}: refused, {}", original.display(), r.kind()));
                let _ = std::fs::remove_dir_all(&dir);
                continue;
            }
        };
        let backups = notes::Backups {
            dir: dir.join("backups"),
            keep: notes::KEEP_BACKUPS,
        };
        notes::commit(&copy, &before_bytes, &plan, &backups).unwrap();
        let back = rig.engine.read_back(&copy, wide).unwrap();
        rig.record();
        let m = moved(&before, &back);
        let shown = notes::confirm(&plan, &back.anchors);
        println!(
            "{} anchors={} moved={} read-back={}",
            original.display(),
            before.anchors.len(),
            m.len(),
            if shown.is_ok() {
                "ok".to_string()
            } else {
                format!("{shown:?}")
            }
        );
        if !m.is_empty() {
            moved_in.push(format!("{}: {m:?}", original.display()));
        }
        if let Err(why) = shown {
            unshown.push(format!("{}: {why}", original.display()));
        }
        measured += 1;
        // Never the original: its bytes are what they were.
        assert_eq!(std::fs::read(original).unwrap(), before_bytes);
        let _ = std::fs::remove_dir_all(&dir);
    }
    println!(
        "MEASURED {measured} briefs; {} with an anchor moved; {} read back otherwise; {} not written ({} files)",
        moved_in.len(),
        unshown.len(),
        skipped.len(),
        briefs.len()
    );
    for u in &unshown {
        println!("UNSHOWN {u}");
    }
    for m in &moved_in {
        println!("MOVED {m}");
    }
    for s in &skipped {
        println!("SKIPPED {s}");
    }
    rig.done();
    assert!(
        moved_in.is_empty(),
        "a saved note moved anchors in {} briefs",
        moved_in.len()
    );
    assert!(
        unshown.is_empty(),
        "{} written briefs do not show what was written",
        unshown.len()
    );
}

/// Concur zones a brief draws in its own markup are not concur support:
/// only a notes script that makes zones reads a concurs island, so only
/// that script's brief is offered a stamp.
#[test]
fn a_concur_zone_in_the_markup_is_not_concur_support() {
    let Some(mut rig) = Rig::new("markup-zones", Html::default(), Duration::from_secs(300)) else {
        return;
    };
    let dir = scratch("markup-zones");
    let page = dir.join("stickers.html");
    std::fs::write(
        &page,
        "<!DOCTYPE html><html><body>\
         <div class=\"notable\" data-nid=\"ask-x\" data-ntitle=\"X\">X<button class=\"concur-zone\">?</button></div>\
         <script>/* reader notes */ function tag() {}</script></body></html>",
    )
    .unwrap();
    let layout = rig.open(&page, NARROW).unwrap();
    assert_eq!(layout.capability.tagged, 1);
    assert_eq!(layout.capability.concur, ConcurSupport::NotSupported);
    let _ = std::fs::remove_dir_all(&dir);
    rig.done();
}
