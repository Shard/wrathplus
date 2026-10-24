use super::minimap_day_tint;

fn approx(a: [f32; 3], b: [f32; 3]) {
    for c in 0..3 {
        assert!((a[c] - b[c]).abs() < 0.01, "{a:?} vs {b:?} @ {c}");
    }
}

#[test]
fn full_white_light_leaves_tiles_at_full_brightness() {
    // Both bands white ⇒ tint white ⇒ the tile draws verbatim (the noon-ish bright case).
    approx(minimap_day_tint([1.0; 3], [1.0; 3]), [1.0; 3]);
}

#[test]
fn default_light_dims_tiles_below_white() {
    // The reference's no-light default, ambient grey 0x40 (≈0.251) and diffuse white:
    // 0.25·1 + 0.75·lerp(0.251, 1, (64+96)/256=0.625) = 0.25 + 0.75·0.719 ≈ 0.789.
    let t = minimap_day_tint([0.251; 3], [1.0; 3]);
    approx(t, [0.789; 3]);
    assert!(t[0] < 0.95, "flat white would be too bright");
}

#[test]
fn pitch_black_light_still_tints_partway_to_white() {
    // The +96 luma floor keeps the map dimly visible with no light: 0.75·(0+1·0.375) ≈ 0.28.
    approx(minimap_day_tint([0.0; 3], [0.0; 3]), [0.281; 3]);
}
