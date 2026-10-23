//! The per-draw point-light pick shared by the model, terrain and static lanes.

/// CPU twin of `point_light_sum`'s pick: 12340's ascending four-nearest gather (`0x834f60`) and
/// its commit (`0x8353d0`), which seats GL slots 1-3 from the farthest down and stops at slot 3.
fn seated(d2: &[f32]) -> Vec<usize> {
    let mut near: Vec<(f32, usize)> = Vec::new();
    for (i, &d) in d2.iter().enumerate() {
        if near.len() == 4 && d >= near[3].0 {
            continue;
        }
        let k = near.partition_point(|&(e, _)| e < d);
        near.insert(k, (d, i));
        near.truncate(4);
    }
    near.iter().rev().take(3).map(|&(_, i)| i).collect()
}

#[test]
fn four_candidates_seat_the_farthest_three() {
    assert_eq!(seated(&[]), Vec::<usize>::new());
    assert_eq!(seated(&[9.0, 1.0]), [0, 1]);
    assert_eq!(seated(&[9.0, 1.0, 4.0]), [0, 2, 1]);
    // The fourth candidate pushes the nearest (index 1) out of the GL slots.
    assert_eq!(seated(&[9.0, 1.0, 4.0, 16.0]), [3, 0, 2]);
    // A fifth beyond the gathered four is not gathered; one inside them evicts the farthest.
    assert_eq!(seated(&[9.0, 1.0, 4.0, 16.0, 25.0]), [3, 0, 2]);
    assert_eq!(seated(&[9.0, 1.0, 4.0, 16.0, 2.0]), [0, 2, 4]);
}

#[test]
fn every_point_light_lane_seats_from_the_farthest_gathered() {
    for (lane, src) in [
        ("wow_model", include_str!("shaders/wow_model.wgsl")),
        ("terrain", include_str!("shaders/terrain.wgsl")),
        (
            "static_gx",
            include_str!("../../wrathplus-world/src/shaders/static_gx.wgsl"),
        ),
    ] {
        assert!(
            src.contains("var sd = array<f32, 4>(1e30, 1e30, 1e30, 1e30);"),
            "{lane}: the gather keeps four"
        );
        assert!(
            src.contains("if (n == 4u && d2 >= sd[3]) {"),
            "{lane}: a full gather rejects the farther"
        );
        assert!(
            src.contains("for (var s = n - min(n, 3u); s < n; s = s + 1u) {"),
            "{lane}: slots 1-3 take the farthest three"
        );
    }
}

/// CPU twin of the M2 lane's candidacy: the light's 20-yd grid cell (`0x834c70`) inside the
/// receiver sphere's cell sweep (`0x81e400`), on each horizontal axis.
fn swept(centre: [f32; 2], radius: f32, light: [f32; 2]) -> bool {
    (0..2).all(|i| {
        let cell = (light[i] * 0.05).floor();
        cell >= ((centre[i] - radius) * 0.05 - 0.5).floor()
            && cell <= ((centre[i] + radius) * 0.05 + 0.5).floor()
    })
}

#[test]
fn the_m2_lane_sweeps_the_receiver_spheres_cells() {
    // A 1-yd receiver at 5 sweeps cells [floor(-0.3), floor(0.8)] = [-1, 0]: [-20, 20) yd.
    assert!(swept([5.0, 5.0], 1.0, [-19.0, 0.0]));
    assert!(!swept([5.0, 5.0], 1.0, [20.5, 0.0]));
    // A box, not a disc: the far corner of the swept cells is still a candidate.
    assert!(swept([5.0, 5.0], 1.0, [19.5, -19.5]));
    // A 30-yd receiver sweeps [floor(-1.75), floor(2.25)] = [-2, 2]: [-40, 60) yd.
    assert!(swept([5.0, 5.0], 30.0, [59.0, -39.0]));
    assert!(!swept([5.0, 5.0], 30.0, [61.0, 0.0]));

    let forward = include_str!("shaders/wow_model.wgsl");
    assert!(forward.contains("let lo = floor((anchor.xz - vec2<f32>(reach)) * 0.05 - 0.5);"));
    assert!(forward.contains("let hi = floor((anchor.xz + vec2<f32>(reach)) * 0.05 + 0.5);"));
    assert!(forward.contains("let cell = floor(pos_range.xz * 0.05);"));
    assert!(forward.contains("reach = m.light_sphere.w * scale;"));
}
