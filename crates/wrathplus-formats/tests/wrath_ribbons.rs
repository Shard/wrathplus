//! Opt-in read-only ribbon fixtures over the installed archives: what the adaptation reduces, the
//! modulate blends, and the material flags the ribbon draw (`0x980b70`) reads.
use wrathplus_formats::Chain;
use wrathplus_m2::wrath::parse_wrath_m2;
use std::collections::BTreeMap;
use std::{io::Cursor, path::Path};

/// What the ribbon adaptation (`models::wrath_effects::adapt_ribbon`) reduces, counted over the
/// install: ribbons by their first material's blend mode, more than one texture or material, a
/// keyed texture slot, look tracks on a global sequence, and a non-zero priority plane.
#[test]
fn installed_wrath_ribbon_feature_census() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let entries = chain
        .list_filtered(|p| p.to_ascii_lowercase().ends_with(".m2"))
        .unwrap();
    let mut blends: BTreeMap<u16, (usize, String)> = BTreeMap::new();
    let (mut ribbons, mut multi_texture, mut multi_material, mut keyed_slot) = (0, 0, 0, 0);
    let (mut global_look, mut priority) = (0, 0);
    // The ribbon draw (`0x980b70`) sets lighting off for material flag 0x1 and fog off for 0x2.
    let (mut unlit, mut unfogged) = (0, 0);
    for entry in entries {
        let b = chain.read_file(&entry.name).unwrap();
        if b.get(4..8) != Some(&264u32.to_le_bytes())
            || u32::from_le_bytes(b[0x120..0x124].try_into().unwrap()) == 0
        {
            continue;
        }
        let Ok(m) = parse_wrath_m2(&mut Cursor::new(&b)) else {
            continue;
        };
        for r in m.ribbons() {
            ribbons += 1;
            let blend = r
                .material_indices
                .first()
                .and_then(|&i| m.model().materials.get(i as usize))
                .map_or(u16::MAX, |mat| mat.blend_mode.bits());
            let flags = r
                .material_indices
                .first()
                .and_then(|&i| m.model().materials.get(i as usize))
                .map_or(0, |mat| mat.flags.bits());
            unlit += usize::from(flags & 0x1 != 0);
            unfogged += usize::from(flags & 0x2 != 0);
            let slot = blends.entry(blend).or_insert((0, entry.name.clone()));
            slot.0 += 1;
            multi_texture += usize::from(r.texture_indices.len() > 1);
            multi_material += usize::from(r.material_indices.len() > 1);
            keyed_slot += usize::from(r.texture_slot.sequences.iter().any(|keys| {
                keys.first()
                    .is_some_and(|first| keys.iter().any(|k| k.value != first.value))
            }));
            global_look += usize::from(
                [
                    r.color.global_sequence,
                    r.alpha.global_sequence,
                    r.height_above.global_sequence,
                    r.height_below.global_sequence,
                ]
                .iter()
                .any(|&g| g != u16::MAX),
            );
            priority += usize::from(r.priority_plane != 0);
        }
    }
    for (blend, (n, example)) in &blends {
        eprintln!("RIBBON_BLEND {blend}: {n} (e.g. {example})");
    }
    eprintln!(
        "RIBBON_FEATURES ribbons={ribbons} multi_texture={multi_texture} \
         multi_material={multi_material} keyed_texture_slot={keyed_slot} \
         global_look_tracks={global_look} priority_plane={priority} unlit={unlit} \
         unfogged={unfogged}"
    );
    assert!(ribbons > 0);
}

/// Twelve installed ribbons are modulate-blended (M2 blend 6, Mod2x); they adapt as Mod2x, not
/// the opaque strip the fallback used to give them.
#[test]
fn installed_wrath_mod2x_ribbons_adapt_as_mod2x() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let path = "SPELLS\\ShadowWordDominate_Chest.m2";
    let bytes = chain.read_file(path).unwrap();
    let skin = chain
        .read_file(&format!("{}00.skin", &path[..path.len() - 3]))
        .unwrap();
    let semantic = wrathplus_formats::parse_wrath_render_model(&bytes, &skin, "SPELLS", &[])
        .unwrap_or_else(|e| panic!("{path}: {e:#}"));
    assert!(!semantic.ribbons.is_empty(), "{path} has ribbons");
    assert!(
        semantic
            .ribbons
            .iter()
            .any(|r| r.blend == wrathplus_formats::ParticleBlend::Mod2x),
        "{path}: {:?}",
        semantic.ribbons.iter().map(|r| r.blend).collect::<Vec<_>>()
    );
}

/// The ribbon draw (`0x980b70`) takes its material's unlit (`0x1`) and unfogged (`0x2`) flags; the
/// adapted defs carry them, read against the raw model.
#[test]
fn installed_wrath_ribbons_carry_their_material_flags() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let path = "Creature\\FireSprite\\FireSprite.m2";
    let bytes = chain.read_file(path).unwrap();
    let skin = chain
        .read_file(&format!("{}00.skin", &path[..path.len() - 3]))
        .unwrap();
    let raw = parse_wrath_m2(&mut Cursor::new(&bytes)).unwrap();
    let semantic =
        wrathplus_formats::parse_wrath_render_model(&bytes, &skin, "Creature\\FireSprite", &[])
            .unwrap_or_else(|e| panic!("{path}: {e:#}"));
    assert_eq!(semantic.ribbons.len(), raw.ribbons().len());
    for (def, r) in semantic.ribbons.iter().zip(raw.ribbons()) {
        let flags = r
            .material_indices
            .first()
            .and_then(|&i| raw.model().materials.get(i as usize))
            .map_or(0, |m| m.flags.bits());
        assert_eq!(def.unlit, flags & 0x1 != 0, "{path} ribbon {}", r.id);
        assert_eq!(def.unfogged, flags & 0x2 != 0, "{path} ribbon {}", r.id);
    }
}
