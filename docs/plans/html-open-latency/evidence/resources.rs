#![allow(dead_code)]
#[path = "/home/parker/Work/td-html-open-latency/app/src"]
mod pack { pub mod lang; }
use pack::lang;
#[path = "/home/parker/Work/td-html-open-latency/app/src/docview"]
mod docview { pub mod cache; pub mod cdp; pub mod engine; pub mod notes; pub mod pref; pub mod snapshot; }
use docview::engine::{bands, layout_hash, Geometry, PageEngine, PageRequest};
use docview::{pref::Html, snapshot::SnapshotEngine};
use serde_json::{json, Value};
use std::{io::{self, Write}, path::PathBuf, time::{Duration, Instant}};
fn checkpoint(data: Value) -> bool {
    println!("{data}"); io::stdout().flush().unwrap();
    let mut line = String::new();
    io::stdin().read_line(&mut line).unwrap() > 0 && line.trim() != "stop"
}
fn main() {
    let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    assert!(!files.is_empty());
    let pref = Html { gpu: std::env::var("PROFILE_GPU").ok(), ..Html::default() };
    let binary = SnapshotEngine::locate(&pref, std::env::var_os("PATH").as_deref()).unwrap();
    let runtime = PathBuf::from(format!("/tmp/td-html-resource-profile-{}", std::process::id()));
    let engine = SnapshotEngine::with_idle(pref, binary, Duration::from_secs(300), runtime.clone());
    let g = Geometry { css_width: 968, viewport_css_height: 1400, scale: 1.6 };
    engine.warm().unwrap();
    checkpoint(json!({"stage":"warm", "pid":std::process::id(), "runtime":runtime}));
    let mut pages = Vec::new(); let mut pngs = Vec::new(); let mut samples = Vec::new();
    for i in 0..9 {
        let path = &files[i % files.len()]; let t = Instant::now();
        let bytes = std::fs::read(path).unwrap();
        let layout = engine.open(&PageRequest { path:path.clone(), geometry:g, expect:layout_hash(&bytes) }).unwrap();
        let page = layout.page.unwrap(); pages.push(page);
        let mut png_bytes = 0; let mut viewport_png_bytes = 0; let mut resident_pixels_bytes = 0;
        for (index, band) in bands(g.height_dev(layout.height_css)).into_iter().enumerate() {
            let tile = engine.tile(page, layout.generation, band).unwrap();
            png_bytes += tile.png.len();
            if band.top_dev < g.height_dev(g.viewport_css_height as f32) { viewport_png_bytes += tile.png.len(); }
            // At the top: visible bands plus the next band, the actual planner's wanted set.
            if index < 3 { resident_pixels_bytes += ((g.css_width as f64 * 1.6).ceil() as u64) * u64::from(band.height_dev) * 4; }
            pngs.push(tile.png);
        }
        let sample = json!({"file":path,"png_bytes":png_bytes,"viewport_png_bytes":viewport_png_bytes,"top_decoded_bytes":resident_pixels_bytes,"height_css":layout.height_css,"wall_ms":t.elapsed().as_secs_f64()*1000.0});
        samples.push(sample);
        if !checkpoint(json!({"stage":"retained", "pages":pages.len(), "png_bytes":pngs.iter().map(Vec::len).sum::<usize>(),"samples":samples})) { break; }
    }
    for p in pages { engine.close(p); }
    checkpoint(json!({"stage":"closed_pages", "png_bytes":pngs.iter().map(Vec::len).sum::<usize>()}));
    engine.shutdown();
    checkpoint(json!({"stage":"shutdown", "png_bytes":pngs.iter().map(Vec::len).sum::<usize>()}));
    std::fs::remove_dir_all(runtime).unwrap();
}
