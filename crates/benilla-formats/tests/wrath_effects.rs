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

#[test]
fn installed_wrath_unsupported_ribbon_census() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let entries = chain
        .list_filtered(|p| p.to_ascii_lowercase().ends_with(".m2"))
        .unwrap();
    let (mut models, mut ribbon_models, mut cubic_models, mut global_models, mut parse_errors) =
        (0, 0, 0, 0, 0);
    for entry in entries {
        let b = chain.read_file(&entry.name).unwrap();
        if b.get(4..8) != Some(&264u32.to_le_bytes()) {
            continue;
        }
        models += 1;
        if u32::from_le_bytes(b[0x120..0x124].try_into().unwrap()) == 0 {
            continue;
        }
        ribbon_models += 1;
        let m = match parse_wrath_m2(&mut Cursor::new(&b)) {
            Ok(m) => m,
            Err(e) => {
                parse_errors += 1;
                eprintln!("CENSUS ERROR {}: {e}", entry.name);
                continue;
            }
        };
        let cubic = m.ribbons().iter().any(|r| {
            [
                r.color.interpolation,
                r.alpha.interpolation,
                r.height_above.interpolation,
                r.height_below.interpolation,
            ]
            .iter()
            .any(|&i| i > 1)
        });
        let global = m.ribbons().iter().any(|r| {
            r.visibility.global_sequence != u16::MAX
                && r.visibility.sequences.first().is_some_and(|keys| {
                    keys.first()
                        .is_some_and(|first| keys.iter().any(|k| k.value != first.value))
                })
        });
        if cubic {
            cubic_models += 1;
        }
        if global {
            global_models += 1;
        }
        if cubic || global {
            eprintln!(
                "UNSUPPORTED {} cubic={cubic} global_visibility={global}",
                entry.name
            );
        }
    }
    eprintln!("CENSUS native_models={models} ribbon_models={ribbon_models} cubic_models={cubic_models} animated_global_visibility_models={global_models} ribbon_parse_errors={parse_errors}");
    assert!(models > 0 && ribbon_models > 0);
}
