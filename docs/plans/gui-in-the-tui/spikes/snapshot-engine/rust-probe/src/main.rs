// Snapshot-engine spike probe (not TD code).
//   limits              — every Vulkan adapter's max_texture_dimension_2d, and what gpui_wgpu
//                         would request (downlevel_defaults().using_resolution(adapter.limits())).
//   decode <png>...     — PNG -> RGBA8 with the png crate and with image::load_from_memory,
//                         plus the RGBA->BGRA swizzle gpui's image path performs. Median of 5.
//   upload <w> <h>      — create a BGRA texture of that size on the high-performance adapter,
//                         write it with queue.write_texture, wait for the GPU. Median of 5.
//                         Reports a validation error instead if the size exceeds the limit.
use std::time::Instant;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[v.len() / 2]
}

fn instance() -> wgpu::Instance {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::VULKAN;
    wgpu::Instance::new(desc)
}

fn limits() {
    let inst = instance();
    let adapters = pollster::block_on(inst.enumerate_adapters(wgpu::Backends::VULKAN));
    for a in adapters {
        let info = a.get_info();
        let l = a.limits();
        let gpui = wgpu::Limits::downlevel_defaults().using_resolution(l.clone());
        println!(
            "{{\"adapter\":\"{}\",\"type\":\"{:?}\",\"driver\":\"{} {}\",\"max_texture_dimension_2d\":{},\"max_buffer_size\":{},\"gpui_requested_max_texture_dimension_2d\":{}}}",
            info.name, info.device_type, info.driver, info.driver_info, l.max_texture_dimension_2d, l.max_buffer_size, gpui.max_texture_dimension_2d
        );
    }
}

fn decode(paths: &[String]) {
    for p in paths {
        let bytes = match std::fs::read(p) { Ok(b) => b, Err(e) => { eprintln!("{p}: {e}"); continue; } };
        let mut png_ms = vec![];
        let mut image_ms = vec![];
        let mut swizzle_ms = vec![];
        let (mut w, mut h) = (0, 0);
        for _ in 0..5 {
            let t = Instant::now();
            let mut dec = png::Decoder::new(std::io::Cursor::new(&bytes));
            dec.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
            let mut reader = match dec.read_info() { Ok(r) => r, Err(e) => { eprintln!("{p}: {e}"); return; } };
            let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
            let info = match reader.next_frame(&mut buf) { Ok(i) => i, Err(e) => { eprintln!("{p}: {e}"); return; } };
            png_ms.push(t.elapsed().as_secs_f64() * 1e3);
            w = info.width; h = info.height;

            let t = Instant::now();
            let img = match image::load_from_memory(&bytes) { Ok(i) => i.into_rgba8(), Err(e) => { eprintln!("{p}: {e}"); return; } };
            image_ms.push(t.elapsed().as_secs_f64() * 1e3);

            let mut raw = img.into_raw();
            let t = Instant::now();
            for px in raw.chunks_exact_mut(4) { px.swap(0, 2); }
            swizzle_ms.push(t.elapsed().as_secs_f64() * 1e3);
        }
        println!(
            "{{\"file\":\"{}\",\"png_bytes\":{},\"w\":{},\"h\":{},\"rgba_bytes\":{},\"png_crate_ms\":{:.1},\"image_crate_ms\":{:.1},\"bgra_swizzle_ms\":{:.1}}}",
            p, bytes.len(), w, h, (w as u64) * (h as u64) * 4, median(png_ms), median(image_ms), median(swizzle_ms)
        );
    }
}

fn upload(w: u32, h: u32) {
    let inst = instance();
    let adapter = match pollster::block_on(inst.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    })) { Ok(a) => a, Err(e) => { eprintln!("no adapter: {e}"); return; } };
    let (device, queue) = match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("probe"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()).using_alignment(adapter.limits()),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    })) { Ok(d) => d, Err(e) => { eprintln!("no device: {e}"); return; } };
    let data = vec![0x7fu8; (w as usize) * (h as usize) * 4];
    let mut times = vec![];
    for _ in 0..5 {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let t = Instant::now();
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tile"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let idx = queue.submit([]);
        if let Err(e) = device.poll(wgpu::PollType::Wait { submission_index: Some(idx), timeout: None }) { eprintln!("poll: {e}"); }
        times.push(t.elapsed().as_secs_f64() * 1e3);
        if let Some(err) = pollster::block_on(scope.pop()) {
            println!("{{\"w\":{w},\"h\":{h},\"error\":\"{}\"}}", err.to_string().replace('"', "'").replace('\n', " "));
            return;
        }
    }
    println!("{{\"adapter\":\"{}\",\"w\":{w},\"h\":{h},\"bytes\":{},\"create_write_wait_ms\":{:.1}}}", adapter.get_info().name, data.len(), median(times));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("limits") => limits(),
        Some("decode") => decode(&args[2..]),
        Some("upload") => {
            let w = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1549);
            let h = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(4096);
            upload(w, h)
        }
        _ => eprintln!("usage: td-snapshot-probe limits | decode <png>... | upload <w> <h>"),
    }
}
