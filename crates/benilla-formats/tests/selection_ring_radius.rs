//! A living unit's selection ring (draw `0x608e00`, sizer `0x60aee0`) has radius
//! `OBJECT_FIELD_SCALE_X * sqrt(0.5 * sqrt(dx² + dy²))` over the Stand sequence box's horizontal
//! extents (`animationLookup[0]`). The render sphere (`0xCC`) sizes the corpse decal instead
//! (`0x5d6fe0`, `[unit+0x2b0]`). `M2Bounds::ring_footprint` is the pre-scale part.

use benilla_formats::{load_m2_bounds, open_chain};

#[test]
fn ring_footprint_matches_reference_pixels() {
    let data = benilla_formats::wow_data_or_skip!();
    let mut chain = open_chain(&data).expect("open chain");

    // (model, ring radius measured off a reference frame capture at scale 1).
    let cases = [
        ("Creature\\Chicken\\Chicken.mdx", 0.572_f32),
        ("Character\\Human\\Female\\HumanFemale.mdx", 0.731),
        ("Character\\Human\\Male\\HumanMale.mdx", 0.841),
        ("Creature\\Horse\\Horse.mdx", 1.295),
    ];

    for (path, measured) in cases {
        let b = load_m2_bounds(&mut chain, path).unwrap_or_else(|e| panic!("bounds {path}: {e}"));
        assert!(
            (b.ring_footprint - measured).abs() < 0.01,
            "{path}: ring_footprint {:.4} should reproduce the reference-measured {measured} (Stand-box \
             sqrt(0.5·sqrt(dx²+dy²)))",
            b.ring_footprint
        );
        assert!(
            (b.ring_footprint - 0.5 * b.sphere_radius).abs() > 0.05,
            "{path}: ring_footprint must not coincide with the corpse-path 0.5×renderSphere"
        );
    }
}
