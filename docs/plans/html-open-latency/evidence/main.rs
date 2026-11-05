#![allow(dead_code)]
#[path = "/home/parker/Work/td-html-open-latency/app/src"]
mod pack { pub mod lang; }
use pack::lang;
#[path = "/home/parker/Work/td-html-open-latency/app/src/docview"]
mod docview {
    pub mod cache;
    pub mod cdp;
    pub mod engine;
    pub mod notes;
    pub mod pref;
    pub mod snapshot;
}
use docview::engine::{bands, layout_hash, Geometry, PageEngine, PageRequest};
use docview::{cache, notes, pref::Html, snapshot::SnapshotEngine};
use serde_json::json;
use std::{path::PathBuf, time::{Duration, Instant}};
fn ms(t: Instant) -> f64 { t.elapsed().as_secs_f64()*1000.0 }
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let pref = Html { gpu: std::env::var("PROFILE_GPU").ok(), chromium:std::env::var_os("PROFILE_CHROMIUM").map(PathBuf::from), ..Html::default() };
    let binary = SnapshotEngine::locate(&pref, std::env::var_os("PATH").as_deref()).unwrap();
    let runtime = PathBuf::from(format!("/tmp/td-html-open-profile-{}",std::process::id()));
    let root = runtime.join("cache");
    let engine = SnapshotEngine::with_idle(pref, binary, Duration::from_secs(300), runtime.clone());
    let g = Geometry { css_width:968, viewport_css_height:1400, scale:1.6 };
    let t=Instant::now(); engine.warm().unwrap(); let warm_ms=ms(t);
    println!("{}",json!({"kind":"startup","ms":warm_ms,"gpu":std::env::var("PROFILE_GPU").ok()}));
    for file in args {
        let path=PathBuf::from(file);
        for repeat in 0..3 {
            let begin=Instant::now();
            let bytes=std::fs::read(&path).unwrap(); let read_ms=ms(begin);
            let t=Instant::now(); let hash=layout_hash(&bytes); let hash_ms=ms(t);
            let t=Instant::now(); let _notes=notes::read(&bytes); let notes_ms=ms(t);
            let t=Instant::now();
            let layout=engine.open(&PageRequest {path:path.clone(),geometry:g,expect:hash}).unwrap();
            let open_ms=ms(t); let page=layout.page.unwrap();
            let mut tiles=vec![]; let mut captures=vec![]; let mut decodes=vec![];
            let mut first_ms=None; let mut viewport_ms=None;
            for band in bands(g.height_dev(layout.height_css)) {
                let t=Instant::now(); let tile=engine.tile(page,layout.generation,band).unwrap(); captures.push(ms(t));
                let t=Instant::now(); let mut raw=image::load_from_memory(&tile.png).unwrap().into_rgba8().into_raw();
                for px in raw.chunks_exact_mut(4) { px.swap(0,2); }
                decodes.push(ms(t));
                if first_ms.is_none() {first_ms=Some(ms(begin));}
                if viewport_ms.is_none() && band.top_dev+band.height_dev >= g.height_dev(layout.height_css.min(g.viewport_css_height as f32)) { viewport_ms=Some(ms(begin)); }
                tiles.push((band,tile.png));
            }
            let all_ms=ms(begin);
            let key=cache::key(&path,hash,g,engine.name(),docview::snapshot::EXTRACT_VERSION);
            let borrowed:Vec<_>=tiles.iter().map(|(b,p)|(*b,p.as_slice())).collect();
            let t=Instant::now(); cache::store(&root,key,&layout,&borrowed).unwrap(); let cache_store_ms=ms(t);
            let t=Instant::now(); let cached=cache::lookup(&root,key).unwrap();
            let cached_bytes:Vec<_>=cached.tiles.iter().map(|(_,p)|std::fs::read(p).unwrap()).collect(); let cache_read_ms=ms(t);
            println!("{}",json!({"kind":"sample","path":path,"repeat":repeat,"bytes":bytes.len(),"height_css":layout.height_css,"anchors":layout.anchors.len(),"read_ms":read_ms,"hash_ms":hash_ms,"notes_ms":notes_ms,"open_ms":open_ms,"capture_ms":captures,"decode_ms":decodes,"first_ms":first_ms,"viewport_ms":viewport_ms,"all_ms":all_ms,"cache_store_ms":cache_store_ms,"cache_read_ms":cache_read_ms,"cache_bytes":cached_bytes.iter().map(Vec::len).sum::<usize>()}));
            engine.close(page);
        }
    }
    engine.shutdown();
    std::fs::remove_dir_all(runtime).unwrap();
}
