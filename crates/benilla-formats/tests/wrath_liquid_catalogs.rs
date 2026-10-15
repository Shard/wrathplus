//! Effective enUS chain regressions, not stock completeness or rendering parity.
use benilla_formats::{wrath_liquid::load_liquid_catalogs, Chain};

#[test]
fn installed_wrath_liquid_catalogs() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(std::path::Path::new(&data), "enUS").unwrap();
    let c = load_liquid_catalogs(&mut chain).unwrap();
    assert_eq!(c.types().len(), 26);
    assert_eq!(c.materials().len(), 3);
    // Independently inspected raw rows. Native 0x8b2720 preserves word order;
    // these known joins do not establish a general water/lava classification.
    for (id, name, flags, material, selector, texture, second) in [
        (
            1,
            "Water",
            15,
            1,
            0,
            "XTextures\\river\\lake_a.%d.blp",
            "proceduralRiverDepthTex",
        ),
        (
            2,
            "Ocean",
            15,
            1,
            0,
            "XTextures\\ocean\\ocean_h.%d.blp",
            "proceduralOceanDepthTex",
        ),
        (3, "Magma", 120, 2, 1, "XTextures\\lava\\lava.%d.blp", ""),
        (4, "Slime", 322, 2, 1, "XTextures\\slime\\slime.%d.blp", ""),
    ] {
        let r = c.liquid_type(id).unwrap();
        assert_eq!(r.name, name);
        assert_eq!(r.flags(), flags);
        assert_eq!(r.material_id(), material);
        assert_eq!(r.strings[0], texture);
        assert_eq!(r.strings[1], second);
        assert!(r.strings[2..].iter().all(String::is_empty));
        assert_eq!(c.material_for(r).unwrap().raw_render_selector(), selector);
    }
    for words in [[1, 0, 1], [2, 1, 0], [3, 0, 1]] {
        assert_eq!(c.material(words[0]).unwrap().raw_columns(), &words);
    }
    let water = c.liquid_type(1).unwrap().raw_columns();
    assert_eq!(
        (water[3], water[4], water[11], water[41], water[42]),
        (0, 1111, 1065353216, 0, 1250)
    );
    let magma = c.liquid_type(3).unwrap().raw_columns();
    assert_eq!(
        (magma[3], magma[4], magma[10], magma[11], magma[12], magma[13]),
        (2, 3072, 7, 1082130432, 1, 1)
    );
    for id in [0, u32::MAX] {
        assert!(c.liquid_type(id).is_none());
        assert!(c.material(id).is_none());
    }
}
