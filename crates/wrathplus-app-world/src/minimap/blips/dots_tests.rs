//! The ObjectIcons dot classifiers and emitters, over synthetic objects.

use super::*;
use wrathplus_protocol::types::Field;

/// Creature type 7, Humanoid: a player's, by race (`ChrRaces.dbc` column 9).
const CREATURE_TYPE_HUMANOID: u32 = 7;

/// The first row is the control: an ordinary live creature still passes.
#[test]
fn the_dead_our_own_minions_and_the_untrackable_draw_no_dot_of_either_kind() {
    use crate::app_core::session::ObjectStore;
    use wrathplus_protocol::ObjectFields;

    const FIELD_HEALTH: u16 = Field::UnitHealth.index(); // UNIT_FIELD_HEALTH
    const FIELD_SUMMONEDBY: u16 = Field::UnitSummonedby.index(); // UNIT_FIELD_SUMMONEDBY (2 dwords)
    const FIELD_CHARMEDBY: u16 = Field::UnitCharmedby.index(); // UNIT_FIELD_CHARMEDBY (2 dwords)
    const FIELD_BYTES_1: u16 = Field::UnitBytes1.index();
    /// `UNIT_FIELD_BYTES_1` byte 2 bit 2, the `& 4` the classifier tests.
    const UNTRACKABLE: u32 = 0x4 << 16;
    const ME: u64 = 0x0000_0000_0000_0007;
    const SOMEONE_ELSE: u64 = 0x0000_0000_0000_0042;

    let store = |pairs: &[(u16, u32)]| ObjectStore(ObjectFields::from_pairs(pairs));
    let alive = [(FIELD_HEALTH, 100u32)];
    let with = |extra: &[(u16, u32)]| {
        let mut v = alive.to_vec();
        v.extend_from_slice(extra);
        store(&v)
    };
    let lo = |g: u64| (g & 0xffff_ffff) as u32;
    let hi = |g: u64| (g >> 32) as u32;

    assert!(
        unit_dot_eligible(Some(&with(&[])), Some(ME)),
        "the control: an ordinary live creature still takes a dot"
    );
    assert!(
        !unit_dot_eligible(Some(&store(&[(FIELD_HEALTH, 0)])), Some(ME)),
        "the dead draw nothing — a SIGNED `<= 0` at 0x4eac1e"
    );
    assert!(
        !unit_dot_eligible(None, Some(ME)),
        "…and so does a candidate whose descriptor has not landed"
    );
    assert!(
        !unit_dot_eligible(
            Some(&with(&[
                (FIELD_SUMMONEDBY, lo(ME)),
                (FIELD_SUMMONEDBY + 1, hi(ME)),
            ])),
            Some(ME)
        ),
        "our own minion is not a blip — equality on the owner guid is the REJECT"
    );
    assert!(
        unit_dot_eligible(
            Some(&with(&[
                (FIELD_SUMMONEDBY, lo(SOMEONE_ELSE)),
                (FIELD_SUMMONEDBY + 1, hi(SOMEONE_ELSE)),
            ])),
            Some(ME)
        ),
        "…but somebody ELSE's minion still is"
    );
    assert!(
        !unit_dot_eligible(
            Some(&with(&[
                (FIELD_CHARMEDBY, lo(ME)),
                (FIELD_CHARMEDBY + 1, hi(ME)),
                // A non-zero CHARMEDBY wins; SUMMONEDBY is only the fallback.
                (FIELD_SUMMONEDBY, lo(SOMEONE_ELSE)),
                (FIELD_SUMMONEDBY + 1, hi(SOMEONE_ELSE)),
            ])),
            Some(ME)
        ),
        "a unit WE charmed is not a blip, and charm outranks summon"
    );
    assert!(
        !unit_dot_eligible(Some(&with(&[(FIELD_BYTES_1, UNTRACKABLE)])), Some(ME)),
        "and the untrackable bit blanks it outright"
    );
}

/// Status 6 draws nothing on the 1.12 client despite vmangos's "red dot" comment (`0x4eac31`).
#[test]
fn quest_dot_is_status_seven_only_gold_cell_three() {
    assert_eq!(quest_dot_cell(7), Some([0.75, 1.0, 0.0, 0.25]));
    for s in [0, 1, 2, 3, 4, 5, 6, 8] {
        assert_eq!(quest_dot_cell(s), None, "status {s} must not dot");
    }
}

/// AC QuestDef.h: native REWARD2=9 has a world '?' but no dot, REWARD=10 dots.
#[test]
fn native_reward_status_decode_controls_minimap_dot() {
    use wrathplus_protocol::{wrath_npc as n, SessionEvent};
    let guid = 0xf130_0001_2300_4567u64;
    for (native, dots) in [(9, false), (10, true)] {
        let mut body = guid.to_le_bytes().to_vec();
        body.push(native);
        let events = n::decode(n::SMSG_QUESTGIVER_STATUS, &body).unwrap();
        let SessionEvent::QuestGiverStatus { npc, status } = events[0] else {
            panic!()
        };
        let mut giver = crate::app_core::state::QuestGiver::default();
        giver.set_status(npc, status);
        assert_eq!(quest_dot_cell(giver.status(npc).unwrap()).is_some(), dots);
    }
}

#[test]
fn all_object_icons_use_the_selected_grid() {
    use ObjectIcon::*;
    let vanilla = ObjectIconAtlas::from_size(128, 128);
    let wrath = ObjectIconAtlas::from_size(256, 64);
    for (icon, old, native) in [
        (
            TrackedResource,
            [0.0, 0.25, 0.0, 0.25],
            [0.75, 0.875, 0.0, 0.5],
        ),
        (TrackedUnit, [0.25, 0.5, 0.0, 0.25], [0.25, 0.375, 0.0, 0.5]),
        (Party, [0.0, 0.25, 0.25, 0.5], [0.0, 0.125, 0.0, 0.5]),
        (
            CompleteQuest,
            [0.75, 1.0, 0.0, 0.25],
            [0.25, 0.375, 0.5, 1.0],
        ),
    ] {
        assert_eq!(vanilla.icon(icon), Some(old));
        assert_eq!(wrath.icon(icon), Some(native));
    }
    assert_eq!(vanilla.cell(5), None);
    assert_eq!(wrath.cell(5), None);
    for status in [0, 1, 2, 3, 4, 6, 9] {
        assert_eq!(wrath.cell(status), None);
    }
}

/// Bit `1 << (n − 1)`, `n` the lock's skill-slot `LockType` or the creature type.
#[test]
fn tracking_predicates_follow_the_mask_bit_law() {
    use wrathplus_formats::{LockSlot, LOCK_KEY_ITEM, MAX_LOCK_SLOTS};
    // A Mining (LockType 3) skill slot, and a key-item slot with the same index.
    let mut vein = [LockSlot::default(); MAX_LOCK_SLOTS];
    vein[0] = LockSlot {
        key_type: LOCK_KEY_SKILL,
        index: 3,
        skill: 0,
        action: 0,
    };
    let mut keyed = [LockSlot::default(); MAX_LOCK_SLOTS];
    keyed[0] = LockSlot {
        key_type: LOCK_KEY_ITEM,
        index: 3,
        skill: 0,
        action: 0,
    };
    let locks = LockCatalog::from_rows([(38, vein), (40, keyed)]);
    assert!(tracked_resource(1 << 2, 38, &locks), "mining bit lights it");
    assert!(
        !tracked_resource(1 << 1, 38, &locks),
        "herbalism bit doesn't"
    );
    assert!(!tracked_resource(0, 38, &locks), "no mask, no dot");
    assert!(
        !tracked_resource(1 << 2, 0, &locks),
        "lockId 0 never tracks"
    );
    assert!(
        !tracked_resource(1 << 2, 40, &locks),
        "a key-ITEM slot's index is an item entry, not a LockType"
    );
    assert!(!tracked_resource(1 << 2, 99, &locks), "unknown lock id");

    // Track Beasts sets bit 0 (Beast is creature type 1).
    let beasts = SelfTracking {
        creatures: 1,
        ..default()
    };
    assert!(tracked_creature(beasts, Some(1), 0, false));
    assert!(!tracked_creature(
        beasts,
        Some(CREATURE_TYPE_HUMANOID),
        0,
        false
    ));
    assert!(!tracked_creature(
        SelfTracking::default(),
        Some(1),
        0,
        false
    ));
    assert!(
        !tracked_creature(beasts, None, 0, false),
        "type not cached yet — no dot"
    );
    // The always-show pair: Hunter's Mark needs no tracking aura; track-stealthed needs
    // both our bit and the unit's CREEP flag.
    assert!(tracked_creature(
        SelfTracking::default(),
        None,
        UNIT_DYNFLAG_TRACK_UNIT,
        false
    ));
    let stealth = SelfTracking {
        stealthed: true,
        ..default()
    };
    assert!(tracked_creature(stealth, None, 0, true));
    assert!(!tracked_creature(stealth, None, 0, false), "not sneaking");
    assert!(
        !tracked_creature(SelfTracking::default(), None, 0, true),
        "we don't track stealthed"
    );
}

/// The tracking spell's `EffectMiscValue`, the server's mask bit and the node's `Lock.dbc`
/// skill slot, on the install's data.
#[test]
fn real_find_minerals_lights_a_copper_vein_not_an_herb() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
    let spells = wrathplus_formats::load_spell_catalog(&mut chain).expect("Spell.dbc");
    let locks = wrathplus_formats::load_lock_catalog(&mut chain).expect("Lock.dbc");
    let forms =
        wrathplus_formats::load_shapeshift_forms(&mut chain).expect("SpellShapeshiftForm.dbc");

    // Bit `1 << (MiscValue − 1)` of the aura-`kind` effect (44 creatures, 45 resources).
    let mask_of = |spell_id: u32, kind: u32| -> u32 {
        let s = spells.get(spell_id).expect("spell row");
        (0..3)
            .find_map(|i| {
                (s.effect_apply_aura[i] == kind).then(|| {
                    let m = s.effect_misc_value[i];
                    assert!((1..=32).contains(&m), "MiscValue {m} out of mask range");
                    1u32 << (m - 1)
                })
            })
            .expect("tracking effect present")
    };

    // Find Minerals 2580 and a Copper Vein (vmangos `gameobject_template` 1731, lockId 38).
    let minerals = mask_of(2580, 45);
    assert_eq!(minerals, 1 << 2, "Find Minerals' MiscValue is Mining (3)");
    assert!(tracked_resource(minerals, 38, &locks));
    // Find Herbs 2383 and Peacebloom or Silverleaf (lockId 29, Herbalism LockType 2).
    let herbs = mask_of(2383, 45);
    assert!(tracked_resource(herbs, 29, &locks));
    assert!(!tracked_resource(minerals, 29, &locks), "cross-profession");
    assert!(!tracked_resource(herbs, 38, &locks), "cross-profession");
    // Track Beasts 1494 (MiscValue 1, Beast) lights a cat-form (1) druid.
    let beasts = SelfTracking {
        creatures: mask_of(1494, 44),
        ..Default::default()
    };
    assert!(tracked_creature(beasts, Some(1), 0, false));
    assert!(!tracked_creature(
        beasts,
        Some(CREATURE_TYPE_HUMANOID),
        0,
        false
    ));
    // A cat-form player (`UNIT_FIELD_BYTES_2` byte 3 is form 1).
    let cat_druid = ObjectStore(wrathplus_protocol::ObjectFields::from_fields(&[(
        Field::UnitBytes2,
        1 << 24,
    )]));
    let cat = Some(
        CreatureTypeSources {
            names: Some(&NameCache::default()),
            forms: Some(&forms),
        }
        .of(&cat_druid),
    );
    assert_eq!(cat, Some(1), "cat form resolves Beast (DBC col 12)");
    assert!(tracked_creature(beasts, cat, 0, false));
}

/// Town tracking (`0x71acf0`, `0x70f850`): a kind-1 row's mask against `UNIT_NPC_FLAGS`, a kind-2
/// row's type against the object's `GAMEOBJECT_BYTES_1` type; Wrath draws it as cell 8.
#[test]
fn town_tracking_matches_npc_flags_or_the_object_type() {
    let unit = |flags: u32| {
        ObjectStore(wrathplus_protocol::ObjectFields::from_fields(&[(
            Field::UnitNpcFlags,
            flags,
        )]))
    };
    let mailbox = ObjectStore(wrathplus_protocol::ObjectFields::from_fields(&[(
        Field::GameobjectBytes1,
        19 << 8,
    )]));
    let with = |town| SelfTracking {
        town,
        ..Default::default()
    };
    let repair = with(Some((1, 0x1000)));
    let mail = with(Some((2, 19)));
    assert!(town_tracked(
        repair,
        EntityKind::Unit,
        Some(&unit(0x1000 | 0x2))
    ));
    assert!(!town_tracked(repair, EntityKind::Unit, Some(&unit(0x200))));
    assert!(!town_tracked(
        repair,
        EntityKind::GameObject,
        Some(&mailbox)
    ));
    assert!(town_tracked(mail, EntityKind::GameObject, Some(&mailbox)));
    assert!(!town_tracked(mail, EntityKind::Unit, Some(&unit(0x1000))));
    assert!(!town_tracked(
        with(None),
        EntityKind::Unit,
        Some(&unit(0x1000))
    ));
    assert_eq!(
        ObjectIconAtlas::Wrath.icon(ObjectIcon::TrackedTown),
        Some([0.0, 0.125, 0.5, 1.0])
    );
    assert_eq!(ObjectIconAtlas::Vanilla.icon(ObjectIcon::TrackedTown), None);
}
