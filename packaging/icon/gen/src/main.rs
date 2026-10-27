//! The WrathPlus app icon: an icy bevelled "W" with a gold "+" on a night-blue rounded square,
//! the login wordmark's palette (`crates/wrathplus-app-ui/assets/branding/wrathplus-logo.png`).
//! Drawn from geometry, so the art is ours alone. Writes `../wrathplus-icon-1024.png`, the
//! `.iconset` sizes `iconutil` packs into `WrathPlus.icns`, and `../wrathplus.ico`.

use image::{imageops, ImageFormat, Rgba, RgbaImage};

const N: u32 = 1024;

fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Distance from `p` to the segment `a`-`b`.
fn seg(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (px, py) = (p.0 - a.0, p.1 - a.1);
    let (bx, by) = (b.0 - a.0, b.1 - a.1);
    let h = ((px * bx + py * by) / (bx * bx + by * by)).clamp(0.0, 1.0);
    ((px - bx * h).powi(2) + (py - by * h).powi(2)).sqrt()
}

/// Signed distance to a rounded square centred in the canvas.
fn rounded_square(p: (f32, f32), half: f32, r: f32) -> f32 {
    let c = N as f32 / 2.0;
    let q = ((p.0 - c).abs() - half + r, (p.1 - c).abs() - half + r);
    let outside = (q.0.max(0.0).powi(2) + q.1.max(0.0).powi(2)).sqrt();
    outside + q.0.max(q.1).min(0.0) - r
}

/// Over-composite `src` (straight alpha) onto `dst`.
fn over(dst: &mut [f32; 4], src: [f32; 3], a: f32) {
    let out_a = a + dst[3] * (1.0 - a);
    if out_a <= 0.0 {
        return;
    }
    for i in 0..3 {
        dst[i] = (src[i] * a + dst[i] * dst[3] * (1.0 - a)) / out_a;
    }
    dst[3] = out_a;
}

fn main() {
    let w_pts = [(292.0, 318.0), (392.0, 700.0), (512.0, 440.0), (632.0, 700.0), (732.0, 318.0)];
    let w_r = 52.0; // stroke half-width
    let plus_c = (744.0, 744.0);
    let plus_arm = 118.0;
    let plus_r = 40.0;
    let mut img = RgbaImage::new(N, N);
    for y in 0..N {
        for x in 0..N {
            let p = (x as f32 + 0.5, y as f32 + 0.5);
            let mut px = [0.0f32; 4];
            // The plate: night blue, lighter at the top, a frost rim and a soft inner glow.
            let d_sq = rounded_square(p, 412.0, 184.0);
            let plate_a = 1.0 - smooth(-1.0, 1.0, d_sq);
            if plate_a > 0.0 {
                let t = p.1 / N as f32;
                let mut c = mix([0.09, 0.17, 0.33], [0.02, 0.05, 0.12], t);
                let glow = (1.0 - ((p.0 - 512.0).powi(2) + (p.1 - 470.0).powi(2)).sqrt() / 520.0)
                    .max(0.0);
                c = mix(c, [0.25, 0.45, 0.70], glow * 0.35);
                let rim = 1.0 - smooth(0.0, 10.0, (-d_sq).max(0.0));
                c = mix(c, [0.70, 0.86, 1.0], rim * 0.55);
                over(&mut px, c, plate_a);
            }
            // The "W": dark outline, then the icy stroke with a bevel highlight along its spine.
            let d_w = w_pts.windows(2).map(|s| seg(p, s[0], s[1])).fold(f32::MAX, f32::min);
            let outline_a = 1.0 - smooth(w_r + 8.0, w_r + 10.0, d_w);
            if outline_a > 0.0 {
                over(&mut px, [0.04, 0.10, 0.22], outline_a);
            }
            let ice_a = 1.0 - smooth(w_r - 1.0, w_r + 1.0, d_w);
            if ice_a > 0.0 {
                let t = ((p.1 - 300.0) / 420.0).clamp(0.0, 1.0);
                let mut c = mix([0.93, 0.97, 1.0], [0.42, 0.66, 0.90], t);
                let spine = 1.0 - (d_w / w_r).clamp(0.0, 1.0);
                c = mix(c, [1.0, 1.0, 1.0], spine.powf(3.0) * 0.6);
                over(&mut px, c, ice_a);
            }
            // The gold "+", outlined like the wordmark's PLUS.
            let d_plus = seg(p, (plus_c.0 - plus_arm, plus_c.1), (plus_c.0 + plus_arm, plus_c.1))
                .min(seg(p, (plus_c.0, plus_c.1 - plus_arm), (plus_c.0, plus_c.1 + plus_arm)));
            let p_out = 1.0 - smooth(plus_r + 8.0, plus_r + 10.0, d_plus);
            if p_out > 0.0 {
                over(&mut px, [0.20, 0.10, 0.02], p_out);
            }
            let gold_a = 1.0 - smooth(plus_r - 1.0, plus_r + 1.0, d_plus);
            if gold_a > 0.0 {
                let t = ((p.1 - (plus_c.1 - plus_arm)) / (2.0 * plus_arm)).clamp(0.0, 1.0);
                let mut c = mix([1.0, 0.90, 0.55], [0.80, 0.50, 0.08], t);
                let spine = 1.0 - (d_plus / plus_r).clamp(0.0, 1.0);
                c = mix(c, [1.0, 0.98, 0.85], spine.powf(3.0) * 0.5);
                over(&mut px, c, gold_a);
            }
            img.put_pixel(
                x,
                y,
                Rgba([
                    (px[0] * 255.0).round() as u8,
                    (px[1] * 255.0).round() as u8,
                    (px[2] * 255.0).round() as u8,
                    (px[3] * 255.0).round() as u8,
                ]),
            );
        }
    }
    img.save("../wrathplus-icon-1024.png").unwrap();
    // The macOS `.iconset` (iconutil names), the runtime PNG and the Windows `.ico`.
    let set = "../WrathPlus.iconset";
    std::fs::create_dir_all(set).unwrap();
    for (name, px) in [
        ("icon_16x16", 16),
        ("icon_16x16@2x", 32),
        ("icon_32x32", 32),
        ("icon_32x32@2x", 64),
        ("icon_128x128", 128),
        ("icon_128x128@2x", 256),
        ("icon_256x256", 256),
        ("icon_256x256@2x", 512),
        ("icon_512x512", 512),
        ("icon_512x512@2x", 1024),
    ] {
        imageops::resize(&img, px, px, imageops::FilterType::Lanczos3)
            .save(format!("{set}/{name}.png"))
            .unwrap();
    }
    imageops::resize(&img, 256, 256, imageops::FilterType::Lanczos3)
        .save("../wrathplus-icon-256.png")
        .unwrap();
    let ico = imageops::resize(&img, 256, 256, imageops::FilterType::Lanczos3);
    ico.save_with_format("../wrathplus.ico", ImageFormat::Ico).unwrap();
}
