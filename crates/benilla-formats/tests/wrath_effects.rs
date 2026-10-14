//! Opt-in read-only native-effect fixtures; no extracted assets are written.
use benilla_formats::Chain;
use benilla_m2::wrath::parse_wrath_m2;
use std::{io::Cursor, path::Path};

#[test]
fn installed_wrath_native_effect_records() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(Path::new(&data), "enUS").unwrap();
    let entries = chain
        .list_filtered(|path| {
            let p = path.to_ascii_lowercase();
            p.ends_with(".m2")
                && (p.contains("frosttrap") || p.contains("brazier") || p.contains("kobold"))
        })
        .unwrap();
    let mut light_models = 0;
    let mut ribbon_models = 0;
    for entry in entries {
        let bytes = chain.read_file(&entry.name).unwrap();
        if bytes.get(4..8) != Some(&264u32.to_le_bytes()) {
            continue;
        }
        let m = parse_wrath_m2(&mut Cursor::new(&bytes))
            .unwrap_or_else(|e| panic!("{}: {e}", entry.name));
        if entry.name.ends_with("DwarvenBrazier01.m2") || entry.name.ends_with("G_FrostTrap.m2") {
            let skin = chain
                .read_file(&format!("{}00.skin", &entry.name[..entry.name.len() - 3]))
                .unwrap();
            let semantic = benilla_formats::parse_wrath_render_model(&bytes, &skin, "", &[])
                .unwrap_or_else(|e| panic!("{} adapter: {e:#}", entry.name));
            if entry.name.ends_with("DwarvenBrazier01.m2") {
                assert_eq!(m.lights().len(), 1);
                assert_eq!(m.lights()[0].light_type, 1);
                assert!(semantic.lights[0].diffuse_intensity > 0.0);
            } else {
                assert_eq!(m.ribbons().len(), 16);
                assert!(semantic.ribbons.iter().any(|r| r
                    .visible
                    .as_ref()
                    .is_some_and(|v| v.per_anim().any(|(_, keys)| keys.len() > 1))));
            }
            assert_eq!(semantic.lights.len(), m.lights().len());
            assert_eq!(semantic.ribbons.len(), m.ribbons().len());
            for r in &semantic.ribbons {
                assert!(r.texture.is_some(), "ribbon texture");
                assert!(r.edge_lifetime >= 0.25);
                assert!(r.tile_rows > 0 && r.tile_cols > 0);
            }
        }
        if !m.lights().is_empty() {
            light_models += 1;
        }
        if !m.ribbons().is_empty() {
            ribbon_models += 1;
        }
        eprintln!(
            "{}: lights={} ribbons={} combiners={}",
            entry.name,
            m.lights().len(),
            m.ribbons().len(),
            m.texture_combiner_combos().len()
        );
    }
    assert!(light_models > 0, "installed point-light fixtures");
    assert!(ribbon_models > 0, "installed ribbon fixtures");
}
