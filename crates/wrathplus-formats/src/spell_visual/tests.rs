//! Tests for the spell visual tables, on synthetic WDBCs and on the installed data.

use super::*;

/// A WDBC of u32 records with an empty string block.
fn build_wdbc(record_count: u32, field_count: u32, records: &[u8]) -> Vec<u8> {
    let record_size = field_count * 4;
    assert_eq!(records.len(), (record_count * record_size) as usize);
    let mut b = Vec::new();
    b.extend_from_slice(b"WDBC");
    b.extend_from_slice(&record_count.to_le_bytes());
    b.extend_from_slice(&field_count.to_le_bytes());
    b.extend_from_slice(&record_size.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes()); // empty string block
    b.extend_from_slice(records);
    b
}

fn u32le(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

/// One `SpellVisual` row: id, the five stage kits, fields 7 and 9; the rest zero.
fn spell_visual_row(
    id: u32,
    precast: u32,
    cast: u32,
    impact: u32,
    state: u32,
    channel: u32,
    missile_model: u32,
    missile_attach: u32,
) -> Vec<u8> {
    let mut rec = Vec::new();
    for v in [id, precast, cast, impact, state, channel] {
        rec.extend(u32le(v));
    }
    for f in 6..SPELL_VISUAL_FIELDS {
        rec.extend(u32le(match f {
            7 => missile_model,
            9 => missile_attach,
            _ => 0,
        }));
    }
    rec
}

/// One `SpellVisualKit` row: id, anim (field 2), world effect (12), sound (13); the rest zero.
fn spell_visual_kit_row(id: u32, anim: u32, sound: u32, world: u32) -> Vec<u8> {
    let mut rec = Vec::new();
    rec.extend(u32le(id));
    rec.extend(u32le(0)); // field 1
    rec.extend(u32le(anim)); // field 2
    for _ in 3..12 {
        rec.extend(u32le(0)); // fields 3..11 (9 emitter slots)
    }
    rec.extend(u32le(world)); // field 12
    rec.extend(u32le(sound)); // field 13
    for _ in 14..SPELL_VISUAL_KIT_FIELDS {
        rec.extend(u32le(0));
    }
    rec
}

#[test]
fn header_shape_matches_the_pinned_layout() {
    // The row builders stay in step with the field counts.
    let sv = spell_visual_row(1, 0, 0, 0, 0, 0, 0, 0);
    assert_eq!(sv.len(), SPELL_VISUAL_FIELDS * 4, "64B record");
    let svk = spell_visual_kit_row(1, 0, 0, 0);
    assert_eq!(svk.len(), SPELL_VISUAL_KIT_FIELDS * 4, "140B record");
}

#[test]
fn parses_stage_kits_and_zero_means_no_kit_at_that_stage() {
    let mut records = Vec::new();
    records.extend(spell_visual_row(67, 30, 38, 286, 0, 0, 365, 1)); // Fireball's visual row
    records.extend(spell_visual_row(1, 0, 0, 0, 0, 0, 0, 0)); // an all-silent row
    let bytes = build_wdbc(2, SPELL_VISUAL_FIELDS as u32, &records);
    let rs = parse(
        &bytes,
        n_u32_schema("SpellVisual", SPELL_VISUAL_FIELDS),
        "t",
    )
    .unwrap();

    let mut visuals = HashMap::new();
    for r in rs.records() {
        let id = u32_at(r, 0).unwrap();
        let g = |i: usize| u32_at(r, i).unwrap_or(0);
        visuals.insert(
            id,
            VisualStages {
                precast: g(1),
                cast: g(2),
                impact: g(3),
                state: g(4),
                channel: g(5),
                missile_model: g(7),
                missile_attach: g(9),
                missile_sound: u32_at(r, 10).and_then(some_unless_none),
                strike_sound: u32_at(r, 14).and_then(some_unless_none),
                missile_gate: g(6),
                area_gate: g(11),
                area_effect: g(12),
                area_kit: g(13),
            },
        );
    }
    assert_eq!(
        visuals[&67],
        VisualStages {
            precast: 30,
            cast: 38,
            impact: 286,
            state: 0,
            channel: 0,
            missile_model: 365,
            missile_attach: 1,
            missile_sound: None,
            strike_sound: None,
            // The builder writes 0 to field 6 and the dest-anchored block.
            ..Default::default()
        }
    );
    assert_eq!(
        visuals[&1],
        VisualStages::default(),
        "an all-zero row is all-silent"
    );
}

#[test]
fn kit_anim_and_sound_fold_both_none_sentinels_to_none() {
    let mut records = Vec::new();
    records.extend(spell_visual_kit_row(38, 53, 1484, 0)); // Fireball's cast kit
    records.extend(spell_visual_kit_row(1, 0, 0, 0)); // plain-zero "none"
    records.extend(spell_visual_kit_row(2, u32::MAX, u32::MAX, u32::MAX)); // the -1 "none" form
    let bytes = build_wdbc(3, SPELL_VISUAL_KIT_FIELDS as u32, &records);
    let rs = parse(
        &bytes,
        n_u32_schema("SpellVisualKit", SPELL_VISUAL_KIT_FIELDS),
        "t",
    )
    .unwrap();

    let mut kits = HashMap::new();
    for r in rs.records() {
        let id = u32_at(r, 0).unwrap();
        let anim_id = u32_at(r, 2).and_then(some_unless_none).map(|a| a as u16);
        let sound = u32_at(r, 13).and_then(some_unless_none);
        kits.insert(
            id,
            VisualKit {
                anim_id,
                sound,
                ..Default::default()
            },
        );
    }
    assert_eq!(
        kits[&38],
        VisualKit {
            anim_id: Some(53),
            sound: Some(1484),
            ..Default::default()
        }
    );
    assert_eq!(kits[&1], VisualKit::default(), "plain 0 = none");
    assert_eq!(kits[&2], VisualKit::default(), "0xFFFFFFFF = none too");
}

#[test]
fn kit_effect_slots_pair_with_their_attach_tags() {
    let kit = VisualKit {
        // Fireball's precast: left hand (slot 3) and right hand (slot 4).
        effect_slots: [
            None,
            None,
            None,
            Some(287),
            Some(287),
            None,
            None,
            None,
            None,
        ],
        ..Default::default()
    };
    assert_eq!(
        kit.effects().collect::<Vec<_>>(),
        vec![(0x15, 287), (0x16, 287)],
        "LeftHand then RightHand, kit-field order"
    );
    assert_eq!(VisualKit::default().effects().count(), 0);
}

/// Field 12 follows the nine slots in `effects()`, at [`WORLD_EFFECT_TAG`].
#[test]
fn kit_world_effect_joins_effects_at_base() {
    let kit = VisualKit {
        // Frost Nova's state kit: the head sparkle (slot 0) and the feet ice in field 12.
        effect_slots: [Some(54), None, None, None, None, None, None, None, None],
        world_effect: Some(284),
        ..Default::default()
    };
    assert_eq!(
        kit.effects().collect::<Vec<_>>(),
        vec![(0x14, 54), (WORLD_EFFECT_TAG, 284)],
        "the nine first, then the world-plant slot"
    );
}

/// The lootable-corpse sparkle's row (`0x6005d0`) names a model that ships.
#[test]
fn real_effect_name_table_resolves_the_loot_art_row() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_spell_visual_catalog(&mut chain).expect("load the visual catalog");
    assert_eq!(cat.loot_art_effect(), Some((14, "Particles\\LootFX.mdl")));
    // Consumers load an `.mdl` path as `.m2`.
    assert!(
        chain.read_file("Particles\\LootFX.m2").is_ok(),
        "LootFX.m2 must ship in the chain"
    );
}

// ── The chain/beam table ──────────────────────────────────────────────────────

/// Pinned against hand-computed IEEE bits, not against itself.
#[test]
fn char_proc_small_int_recovers_the_integer() {
    // 1.0 + 512.0 = 513.0 = 0x44004000; >>14 = 0x11001; &0xff = 1.
    assert_eq!(char_proc_small_int(1.0), 1);
    assert_eq!(char_proc_small_int(0.0), 0);
    for n in 0..=255u32 {
        assert_eq!(char_proc_small_int(n as f32), n, "round-trips every byte");
    }
    // It truncates, reading the integer part out of the mantissa.
    assert_eq!(char_proc_small_int(3.9), 3);
}

/// The client's fill (`60d4d2`-`60d54c`), field by field.
#[test]
fn the_weapon_merge_fills_only_the_empty_slots() {
    let weapon = VisualStages {
        precast: 7,
        cast: 164,
        impact: 1947,
        state: 999,   // +0x10, never merged
        channel: 998, // +0x14, never merged
        missile_gate: 1,
        missile_model: 42,
        missile_attach: 3,
        missile_sound: Some(4222),
        strike_sound: Some(1143),
        area_gate: 1,
        area_effect: 7,
        area_kit: 9,
    };
    // Multi-Shot's real shape: its own impact + missile, both body kits empty.
    let own = VisualStages {
        impact: 658,
        missile_gate: 1,
        missile_model: 528,
        missile_attach: 1,
        ..Default::default()
    };
    let merged = own.merged_over_weapon(&weapon);
    assert_eq!((merged.precast, merged.cast), (7, 164), "empty slots fill");
    assert_eq!(merged.impact, 658, "a populated slot is kept");
    assert_eq!(
        (merged.missile_model, merged.missile_attach),
        (528, 1),
        "its own missile block survives — the gate was already nonzero"
    );
    assert_eq!(
        merged.missile_sound,
        Some(4222),
        "the flight loop fills from the bow (Serpent Sting / Multi-Shot carry none)"
    );
    assert_eq!(
        (merged.state, merged.channel),
        (0, 0),
        "state/channel are NOT in the client's fill list"
    );
    assert_eq!(
        (merged.area_gate, merged.area_effect, merged.area_kit),
        (0, 0, 0),
        "neither is the dest-anchored block"
    );

    // A row with no missile takes the weapon's, the gate as a literal 1 (`60d50b`).
    let no_missile = VisualStages::default().merged_over_weapon(&VisualStages {
        missile_gate: 5,
        missile_model: 42,
        ..Default::default()
    });
    assert_eq!((no_missile.missile_gate, no_missile.missile_model), (1, 42));
    // A weapon with no missile gives none.
    let neither = VisualStages::default().merged_over_weapon(&VisualStages::default());
    assert_eq!((neither.missile_gate, neither.missile_model), (0, 0));
}

/// The type-8 arm truncates (`_ftol`, `0x40a2b0`); the small-int decode would give no trail.
#[test]
fn a_weapon_trail_proc_decodes_by_truncation() {
    // Kit 324's shipped row: Zero = 16263465 (`0xf82929`), One = 20, Two = 600, Three = 100.
    let proc = CharProc {
        ty: char_proc_type::WEAPON_TRAIL,
        params: [16_263_465.0, 20.0, 600.0, 100.0],
    };
    let trail = proc.as_weapon_trail().expect("a live trail proc");
    assert_eq!(trail.rgb(), [0xf8, 0x29, 0x29], "R248 G41 B41");
    assert_eq!(trail.alpha(), 100);
    assert_eq!(trail.duration_ms, 600);
    assert_eq!(
        trail.packed, 0x64f8_2929,
        "0xAARRGGBB, as `unit+0xd1c` holds it"
    );
    assert_ne!(
        proc.small_int(0),
        u32::from(trail.rgb()[0]),
        "the small-int decode is the WRONG idiom here and must not be reused"
    );
}

/// A zero duration fires nothing (`0x5fe494`).
#[test]
fn a_zero_duration_trail_proc_is_no_trail() {
    let proc = CharProc {
        ty: char_proc_type::WEAPON_TRAIL,
        params: [16_263_465.0, 20.0, 0.0, 100.0],
    };
    assert!(proc.as_weapon_trail().is_none());
}

#[test]
fn only_type_eight_arms_a_trail() {
    for ty in [
        char_proc_type::TINT,
        char_proc_type::ALPHA,
        char_proc_type::ANIM_RATE,
        char_proc_type::CHAIN_CAST,
    ] {
        let proc = CharProc {
            ty,
            params: [16_263_465.0, 20.0, 600.0, 100.0],
        };
        assert!(proc.as_weapon_trail().is_none(), "type {ty}");
    }
}
