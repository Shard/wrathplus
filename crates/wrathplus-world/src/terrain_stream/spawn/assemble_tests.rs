use super::*;

/// `World\critter\birds\Bird01.m2` (`wrathplus-extract animboundscan`): a 1.2 × 1.8 × 0.23 yd
/// bind-pose box and a 67 × 18 × 7 yd authored all-animation box.
#[test]
fn an_animated_placements_bound_covers_the_whole_flight_path() {
    // Bevy-space conversions of Bird01's two boxes (WoW (x,y,z) -> Bevy (-y, z, -x)).
    let bind = Aabb::from_min_max(
        Vec3::new(-0.863, 9.359, -3.536),
        Vec3::new(0.915, 9.586, -2.366),
    );
    let authored = Aabb::from_min_max(
        Vec3::new(-4.219, 8.816, -30.547),
        Vec3::new(13.571, 16.019, 36.605),
    );
    let widened = cull_bound(Some(&bind), Some(authored)).expect("a bound");
    // `Aabb` stores centre and half-extents, so the rebuilt corners need a float tolerance.
    const EPS: f32 = 1e-4;
    assert!((widened.min() - EPS).cmple(authored.min()).all());
    assert!((widened.max() + EPS).cmpge(authored.max()).all());
    // The bind-pose bound alone is ~37 yd short along the flight axis.
    assert!(authored.max().z - bind.max().z > 36.0);
}

#[test]
fn a_short_authored_box_never_shrinks_the_bound() {
    let bind = Aabb::from_min_max(Vec3::splat(-5.0), Vec3::splat(5.0));
    let authored = Aabb::from_min_max(Vec3::new(-40.0, -1.0, -1.0), Vec3::new(40.0, 1.0, 1.0));
    let b = cull_bound(Some(&bind), Some(authored)).expect("a bound");
    assert!(Vec3::from(b.min()).abs_diff_eq(Vec3::new(-40.0, -5.0, -5.0), 1e-4));
    assert!(Vec3::from(b.max()).abs_diff_eq(Vec3::new(40.0, 5.0, 5.0), 1e-4));
}

#[test]
fn a_static_placement_keeps_its_per_batch_bound() {
    let bind = Aabb::from_min_max(Vec3::splat(-1.0), Vec3::splat(1.0));
    let b = cull_bound(Some(&bind), None).expect("a bound");
    assert!(Vec3::from(b.min()).abs_diff_eq(Vec3::splat(-1.0), 1e-4));
    assert!(Vec3::from(b.max()).abs_diff_eq(Vec3::splat(1.0), 1e-4));
}

/// The ballista's shape: section 10 drawn twice (base and shine), section 4 once.
#[test]
fn both_batches_of_a_shared_section_are_flagged() {
    let flags = shared_geometry(&[Some(4), Some(10), Some(10), Some(7)]);
    assert_eq!(flags, vec![false, true, true, false]);
}

/// WMO batches carry no section, and `None == None` must not count as sharing one.
#[test]
fn section_less_batches_never_share() {
    assert_eq!(shared_geometry(&[None, None, None]), vec![false; 3]);
    assert_eq!(
        shared_geometry(&[None, Some(0), Some(0), None]),
        vec![false, true, true, false]
    );
}
