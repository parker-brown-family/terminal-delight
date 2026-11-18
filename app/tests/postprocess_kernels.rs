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

// ---- td-doc-shoulder (patch 0018) ----
//
// A document read inside a CURVED glow pane is graded by its gauges and then
// drawn 35% hotter (`tube = lit_col * falloff * 1.35 * shade + glare`), so
// everything the gauges left above about 0.74 clipped to white: the glow pane
// did not obey the top of its brightness and contrast sliders. Patch 0018 rolls
// a document's highlights off toward a light grey on that path only. The flat
// reader and the flat tube were already right and are not touched.

fn shader_f32(name: &str) -> f32 {
    let at = SHADER
        .find(&format!("const {name}: f32 = "))
        .unwrap_or_else(|| panic!("crt_pass.wgsl lacks {name}; run scripts/prepare-gpui.sh"));
    SHADER[at..]
        .split_once("= ")
        .unwrap()
        .1
        .split_once(';')
        .unwrap()
        .0
        .trim()
        .parse()
        .unwrap()
}

/// `doc_shoulder` for a document's pixel: under the knee in luminance, as it
/// came; over it, the whole colour scaled so its luminance rolls off.
fn shoulder_rgb(col: [f32; 3]) -> [f32; 3] {
    let (knee, ceiling) = (
        shader_f32("DOC_SHOULDER_KNEE"),
        shader_f32("DOC_SHOULDER_CEILING"),
    );
    let l = col[0] * 0.2126 + col[1] * 0.7152 + col[2] * 0.0722;
    if l <= knee {
        return col;
    }
    let span = ceiling - knee;
    let rolled = knee + span * (1.0 - (-(l - knee) / span).exp());
    col.map(|v| v * (rolled / l))
}

fn luma(c: [f32; 3]) -> f32 {
    c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
}

/// The curved tube's document pixel before 0018 (the framebuffer clips each
/// channel at 1) and after: graded by the gauges, 35% hotter, then the shoulder.
fn curved(c: [f32; 3], brightness: f32, contrast: f32, shouldered: bool) -> [f32; 3] {
    let hot = c.map(|v| (((v - 0.5) * contrast + 0.5) * brightness).clamp(0.0, 1.0) * 1.35);
    if shouldered {
        shoulder_rgb(hot)
    } else {
        hot.map(|v| v.min(1.0))
    }
}

#[test]
fn the_shoulder_is_the_shaders_and_only_on_the_curved_documents_path() {
    for line in [
        "let tube = doc_shoulder(lit_col * falloff * 1.35 * shade + glare_color * glare_amount, g * dims);",
        "return crt.doc_grade[i - 1u].z > 0.5;",
        "if (!in_document(p)) {",
        "let l = dot(col, vec3<f32>(0.2126, 0.7152, 0.0722));",
        "if (l <= DOC_SHOULDER_KNEE) {",
        "let rolled = DOC_SHOULDER_KNEE + span * (1.0 - exp(-(l - DOC_SHOULDER_KNEE) / span));",
        "return col * (rolled / l);",
    ] {
        assert!(SHADER.contains(line), "crt_pass.wgsl lacks: {line}");
    }
    // Defined once, called once: the flat tube, the FOCUS reader and the plain
    // pass return exactly what they returned before 0018.
    assert_eq!(SHADER.matches("doc_shoulder(").count(), 2);
    for flat in [
        "return vec4<f32>(doc_graded(image_bloom(base, p, r, crt.image_glow[i].x), p), original.a);",
        "return vec4<f32>(doc_graded(sharp.rgb, p), 1.0);",
        "return vec4<f32>(mix(frost, doc_graded(sharp.rgb, p), m), 1.0);",
        "return vec4<f32>(doc_graded(col.rgb, p), col.a);",
    ] {
        assert!(SHADER.contains(flat), "a flat path changed: {flat}");
    }
}

#[test]
fn below_the_knee_a_document_is_untouched_and_above_it_rolls_off_under_the_ceiling() {
    let (knee, ceiling) = (
        shader_f32("DOC_SHOULDER_KNEE"),
        shader_f32("DOC_SHOULDER_CEILING"),
    );
    assert!((0.75..=0.8).contains(&knee) && ceiling < 0.95 && ceiling > knee);
    // Every 8-bit grey at or below the knee comes out bit for bit, and so does
    // a saturated mid-tone whose blue alone is bright (a purple glass card).
    for byte in 0..=255u32 {
        let v = byte as f32 / 255.0;
        if v <= knee {
            let out = shoulder_rgb([v; 3]);
            assert_eq!(
                out.map(f32::to_bits),
                [v; 3].map(f32::to_bits),
                "{byte}/255"
            );
        }
    }
    let purple = [0.30, 0.25, 0.85];
    assert!(luma(purple) <= knee);
    assert_eq!(shoulder_rgb(purple), purple, "a mid-tone keeps its colour");
    // Above it: luminance rising, under the ceiling however hot, no step at
    // the knee, and the hue kept (the channels keep their ratios).
    let mut last = luma(shoulder_rgb([knee; 3]));
    for i in 1..=72 {
        let v = knee + i as f32 * 0.01;
        let out = luma(shoulder_rgb([v; 3]));
        assert!(out > last && out < ceiling, "{v} -> {out}");
        last = out;
    }
    for v in [2.0f32, 4.0, 100.0] {
        assert!(luma(shoulder_rgb([v; 3])) <= ceiling);
    }
    let slope = (luma(shoulder_rgb([knee + 1e-3; 3])) - knee) / 1e-3;
    assert!((slope - 1.0).abs() < 0.01, "slope 1 at the knee: {slope}");
    let cyan = shoulder_rgb([0.9, 1.3, 1.3]);
    assert!(
        (cyan[1] / cyan[0] - 1.3 / 0.9).abs() < 1e-5,
        "hue kept: {cyan:?}"
    );
}

#[test]
fn a_white_card_under_the_gauges_reads_light_grey_and_mid_values_are_unchanged() {
    // The gauges Parker runs in the glow pane: brightness 1.17, contrast 1.23.
    let (b, c) = (1.17, 1.23);
    let (knee, ceiling) = (
        shader_f32("DOC_SHOULDER_KNEE"),
        shader_f32("DOC_SHOULDER_CEILING"),
    );
    // A card ground and a light-grey button: both clipped to white before.
    for v in [0.87f32, 0.95, 1.0] {
        assert_eq!(
            curved([v; 3], b, c, false),
            [1.0; 3],
            "{v} clipped before 0018"
        );
        let now = luma(curved([v; 3], b, c, true));
        assert!(
            now < ceiling && now > 0.8,
            "{v} now reads light grey: {now}"
        );
    }
    // Mid and low values, whatever the gauges did to them, as before: bit for
    // bit where they land at or under the knee, within 1/255 just over it (a
    // mid-grey 0.5 lands at 0.79 under these gauges).
    for v in [0.0f32, 0.1, 0.25, 0.4, 0.45, 0.5] {
        let (before, now) = (curved([v; 3], b, c, false), curved([v; 3], b, c, true));
        if luma(before) <= knee {
            assert_eq!(now.map(f32::to_bits), before.map(f32::to_bits), "{v}");
        } else {
            assert!(
                (luma(now) - luma(before)).abs() <= 1.0 / 255.0,
                "{v}: {before:?} -> {now:?}"
            );
        }
    }
}
