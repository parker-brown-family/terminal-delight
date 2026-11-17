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
