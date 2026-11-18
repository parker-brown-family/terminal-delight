//! The production shader must keep the measured fast paths and the original
//! kernel, including the outside-the-tube sample rule at a neighbour's edge.
const SHADER: &str = include_str!("../../../zed-upstream/crates/gpui_wgpu/src/crt_pass.wgsl");

#[test]
fn tube_blur_rotates_one_fixed_disk_per_fragment() {
    let rows = SHADER
        .split_once("const DISK =")
        .unwrap()
        .1
        .split_once(");")
        .unwrap()
        .0;
    let taps: Vec<Vec<f64>> = rows
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("vec3<f32>(")?
                .strip_suffix("),")
                .map(|v| v.split(',').map(|n| n.trim().parse().unwrap()).collect())
        })
        .collect();
    assert_eq!(taps.len(), 32);
    let sum: f64 = (0..32).map(|i| (-1.8 * (i as f64 + 0.5) / 32.).exp()).sum();
    for (i, tap) in taps.iter().enumerate() {
        let t = i as f64 + 0.5;
        let r = (t / 32.).sqrt();
        let a = 2.3999632 * t;
        let expected = [r * a.cos(), r * a.sin(), (-1.8 * t / 32.).exp() / sum];
        assert_eq!(tap.len(), 3);
        assert!(tap.iter().zip(expected).all(|(v, x)| (v - x).abs() < 1e-8));
        assert_eq!(SHADER.matches(&format!("rotation, DISK[{i}]")).count(), 1);
    }
}

#[test]
fn backdrop_and_tube_share_the_same_filter() {
    let backdrop = SHADER.split_once("if (fb_radius > 0.5)").unwrap().1;
    assert!(backdrop.contains("tube_blur(p, vec4<f32>(0.0, 0.0, dims), fb_radius)"));
    assert!(backdrop.contains("if (m > 0.999)"));
}

#[test]
fn interior_glow_uses_the_border_kernel_without_sixteen_bounds_checks() {
    let bloom = SHADER
        .split_once("fn image_bloom(")
        .unwrap()
        .1
        .split_once("// Screen blend")
        .unwrap()
        .0;
    let (inside, outside) = bloom.split_once("} else {").unwrap();
    assert!(inside.contains("p >= r.xy + vec2<f32>(5.31)"));
    assert!(inside.contains("p < r.xy + r.zw - vec2<f32>(5.31)"));
    let interior: Vec<_> = inside
        .lines()
        .filter_map(|l| l.trim().strip_prefix("halo += glow_sample"))
        .collect();
    let border: Vec<_> = outside
        .lines()
        .filter_map(|l| l.trim().strip_prefix("halo += glow_emission"))
        .map(|l| l.replace("1.6, r)", "1.6)"))
        .collect();
    assert_eq!(interior.len(), 16);
    assert_eq!(interior, border);
}

// ---- td-light-aware-glow (patch 0018) ----
//
// Pictures in a document read inside a pane's glass washed out: the image bloom
// lifted every pixel near a bright one toward white, and the document grade then
// clipped the light greys. Measured on a real light screenshot under the gauges
// a person had set (glow 0.51, brightness ×1.17, contrast ×1.23): the 5th-95th
// percentile spread fell from 0.43 to 0.07. The shader's two curves are mirrored
// here in Rust and pinned to the WGSL by their constants and their lines.

fn shader_const(name: &str) -> f32 {
    let at = SHADER
        .find(&format!("const {name}: f32 = "))
        .unwrap_or_else(|| panic!("crt_pass.wgsl lacks {name}; run scripts/prepare-gpui.sh"));
    let rest = &SHADER[at..];
    let value = rest.split_once("= ").unwrap().1.split_once(';').unwrap().0;
    value.trim().parse().unwrap()
}

/// An RGB image, row-major, 0..1.
struct Img {
    w: usize,
    h: usize,
    px: Vec<[f32; 3]>,
}

impl Img {
    fn new(w: usize, h: usize, fill: [f32; 3]) -> Img {
        Img {
            w,
            h,
            px: vec![fill; w * h],
        }
    }
    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, c: [f32; 3]) {
        for yy in y..(y + h).min(self.h) {
            for xx in x..(x + w).min(self.w) {
                self.px[yy * self.w + xx] = c;
            }
        }
    }
    /// A line of glyph-like strokes: 1-2 px wide, 1-2 px apart, 7 px tall.
    fn text(&mut self, x: usize, y: usize, len: usize, c: [f32; 3]) {
        let mut at = x;
        let mut i = 0;
        while at < x + len {
            let stroke = 1 + i % 2;
            let tall = if i % 5 == 0 { 3 } else { 7 };
            self.rect(at, y + 7 - tall, stroke, tall, c);
            at += stroke + 1 + (i % 3 == 0) as usize;
            if i % 7 == 6 {
                at += 4; // a space between words
            }
            i += 1;
        }
    }
    fn at(&self, x: isize, y: isize) -> [f32; 3] {
        let x = x.clamp(0, self.w as isize - 1) as usize;
        let y = y.clamp(0, self.h as isize - 1) as usize;
        self.px[y * self.w + x]
    }
    fn map(&self, f: impl Fn(usize, usize, [f32; 3]) -> [f32; 3]) -> Img {
        let mut out = Img::new(self.w, self.h, [0.; 3]);
        for y in 0..self.h {
            for x in 0..self.w {
                out.px[y * self.w + x] = f(x, y, self.px[y * self.w + x]);
            }
        }
        out
    }
}

fn lum(c: [f32; 3]) -> f32 {
    c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
}

/// `glow_sample`: the part of a pixel bright enough to glow.
fn glow_sample(c: [f32; 3]) -> [f32; 3] {
    let peak = c[0].max(c[1]).max(c[2]);
    let bright = (peak - 0.12).max(0.) / peak.max(0.0001);
    c.map(|v| v * bright)
}

/// `image_bloom`'s halo: sixteen taps at (±1, ±3) × 1.6 px, weights 1-3-3-1
/// squared over 64, sampled at the nearest pixel.
fn halo(img: &Img, x: usize, y: usize) -> [f32; 3] {
    let mut h = [0f32; 3];
    for (oy, wy) in [(-3., 1.), (-1., 3.), (1., 3.), (3., 1.)] {
        for (ox, wx) in [(-3., 1.), (-1., 3.), (1., 3.), (3., 1.)] {
            let s = glow_sample(img.at(
                x as isize + (ox * 1.6f32).round() as isize,
                y as isize + (oy * 1.6f32).round() as isize,
            ));
            for c in 0..3 {
                h[c] += s[c] * wx * wy / 64.;
            }
        }
    }
    h
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// The bloom as it was (`light_aware: false`) and as patch 0018 makes it.
fn bloom(img: &Img, strength: f32, light_aware: bool) -> Img {
    let (lo, hi) = (shader_const("GLOW_ROOM_LO"), shader_const("GLOW_ROOM_HI"));
    img.map(|x, y, base| {
        let h = halo(img, x, y);
        let room = if light_aware {
            1. - smoothstep(lo, hi, lum(h))
        } else {
            1.
        };
        [0, 1, 2].map(|c| base[c] + h[c] * strength * 1.8 * (1. - base[c]) * room)
    })
}

/// The document grade as it was (a clamp) and as patch 0018 makes it.
fn graded(v: f32, brightness: f32, contrast: f32, eased: bool) -> f32 {
    let x = ((v - 0.5) * contrast + 0.5) * brightness;
    if !eased {
        return x.clamp(0., 1.);
    }
    let knee = shader_const("DOC_KNEE");
    if x > knee {
        knee + (1. - knee) * (1. - (-(x - knee) / (1. - knee)).exp())
    } else {
        x.max(0.)
    }
}

/// How far each pixel stands from its 5×5 neighbourhood, on average: what
/// makes text and edges legible.
fn detail(img: &Img) -> f32 {
    let mut total = 0.;
    for y in 0..img.h {
        for x in 0..img.w {
            let mut m = 0.;
            for dy in -2..=2isize {
                for dx in -2..=2isize {
                    m += lum(img.at(x as isize + dx, y as isize + dy));
                }
            }
            total += (lum(img.px[y * img.w + x]) - m / 25.).abs();
        }
    }
    total / (img.w * img.h) as f32
}

/// A light screenshot, the shape of the one that washed out: a near-white
/// card, a teal header with white text, dark-grey text in the body.
fn light_card() -> Img {
    let mut img = Img::new(220, 300, [0.95, 0.95, 0.95]);
    img.rect(0, 0, 220, 50, [0.36, 0.62, 0.62]);
    img.text(16, 14, 120, [0.97, 0.97, 0.97]);
    img.text(16, 30, 90, [0.85, 0.92, 0.92]);
    for row in 0..12 {
        img.text(16, 70 + row * 18, 150 + (row % 3) * 15, [0.3, 0.3, 0.3]);
    }
    img
}

/// A dark terminal: light text on near-black.
fn dark_terminal() -> Img {
    let mut img = Img::new(220, 300, [0.05, 0.05, 0.08]);
    for row in 0..14 {
        img.text(8, 8 + row * 20, 120 + (row % 4) * 20, [0.84, 0.86, 0.82]);
    }
    img
}

const GLOW: f32 = 0.51;
const BRIGHTNESS: f32 = 1.17;
const CONTRAST: f32 = 1.23;

#[test]
fn light_aware_glow_and_eased_grade_are_the_shaders() {
    // The lines that make the curves, as 0018 writes them.
    for line in [
        "let field = dot(halo, vec3<f32>(0.2126, 0.7152, 0.0722));",
        "let room = 1.0 - smoothstep(GLOW_ROOM_LO, GLOW_ROOM_HI, field);",
        "return base + halo * strength * 1.8 * (vec3<f32>(1.0) - base) * room;",
        "return ease_into_white(((col - vec3<f32>(0.5)) * g.y + vec3<f32>(0.5)) * g.x);",
        "+ (1.0 - DOC_KNEE) * (vec3<f32>(1.0) - exp(-over / (1.0 - DOC_KNEE)));",
        "return select(max(x, vec3<f32>(0.0)), eased, x > vec3<f32>(DOC_KNEE));",
    ] {
        assert!(SHADER.contains(line), "crt_pass.wgsl lacks: {line}");
    }
}

#[test]
fn a_light_picture_keeps_its_detail_under_the_glow_and_the_gauges() {
    let card = light_card();
    let made = detail(&card);
    let finish =
        |img: &Img, eased| img.map(|_, _, c| c.map(|v| graded(v, BRIGHTNESS, CONTRAST, eased)));
    let today = detail(&finish(&bloom(&card, GLOW, false), false));
    let now = detail(&finish(&bloom(&card, GLOW, true), true));
    eprintln!("light card detail: as made {made:.4}, today {today:.4}, now {now:.4}");
    assert!(
        today < made * 0.35,
        "the mirror must reproduce the wash-out: {today} of {made}"
    );
    assert!(
        now > made * 0.6,
        "most of the detail survives: {now} of {made}"
    );
}

#[test]
fn a_dark_field_keeps_its_glow() {
    let term = dark_terminal();
    let before = bloom(&term, GLOW, false);
    let after = bloom(&term, GLOW, true);
    let energy = |img: &Img| -> f32 {
        img.px
            .iter()
            .zip(&term.px)
            .map(|(a, b)| (0..3).map(|c| a[c] - b[c]).sum::<f32>())
            .sum()
    };
    let kept = energy(&after) / energy(&before);
    let moved = after
        .px
        .iter()
        .zip(&before.px)
        .filter(|(a, b)| (0..3).any(|c| (a[c] - b[c]).abs() > 8. / 255.))
        .count() as f32
        / term.px.len() as f32;
    eprintln!(
        "dark terminal: glow kept {kept:.4}, pixels moved over 8/255 {:.3}%",
        moved * 100.
    );
    assert!(kept > 0.95, "a dark terminal keeps its glow: {kept}");
    assert!(moved < 0.01, "and almost no pixel of it moves: {moved}");
}

#[test]
fn a_light_grey_under_the_gauges_eases_into_white_instead_of_clipping() {
    let greys = [0.80f32, 0.85, 0.90, 0.95, 1.0];
    let clamped: Vec<f32> = greys
        .iter()
        .map(|&v| graded(v, BRIGHTNESS, CONTRAST, false))
        .collect();
    assert!(
        clamped.iter().filter(|&&v| v == 1.0).count() >= 4,
        "the clamp made them one white: {clamped:?}"
    );
    let eased: Vec<f32> = greys
        .iter()
        .map(|&v| graded(v, BRIGHTNESS, CONTRAST, true))
        .collect();
    assert!(
        eased.windows(2).all(|w| w[0] < w[1]),
        "each stays its own grey: {eased:?}"
    );
    assert!(eased.iter().all(|&v| v < 1.0));
    // Below the knee nothing changes, so a dark page under the same gauges
    // grades exactly as before, and the curve meets the line without a step.
    let knee = shader_const("DOC_KNEE");
    for v in [0.0f32, 0.1, 0.3, 0.5] {
        assert_eq!(
            graded(v, BRIGHTNESS, CONTRAST, true),
            graded(v, BRIGHTNESS, CONTRAST, false)
        );
    }
    let just = |d: f32| ((knee + d) / BRIGHTNESS - 0.5) / CONTRAST + 0.5;
    let (below, above) = (
        graded(just(-1e-4), BRIGHTNESS, CONTRAST, true),
        graded(just(1e-4), BRIGHTNESS, CONTRAST, true),
    );
    assert!(
        (above - below).abs() < 1e-3,
        "continuous at the knee: {below} {above}"
    );
}
