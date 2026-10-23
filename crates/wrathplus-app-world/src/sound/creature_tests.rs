use super::super::kit::occupies_voice_slot;
use super::*;

/// Every `SMSG_AI_REACTION` HOSTILE arrival for a bear pet in one traced session, in seconds
/// from the first: 63 inside 19 s, the rate vmangos sends by design.
const BEAR_BURST_S: [f32; 63] = [
    0.000, 0.000, 0.000, 1.417, 2.950, 4.481, 5.997, 7.530, 9.046, 10.589, 10.670, 10.792, 12.312,
    12.413, 12.546, 12.603, 12.713, 12.813, 12.912, 13.013, 13.112, 13.214, 13.315, 13.424, 13.528,
    13.624, 13.740, 13.829, 13.928, 14.032, 14.137, 14.239, 14.331, 14.449, 14.545, 14.646, 14.745,
    14.847, 16.393, 16.499, 16.684, 16.789, 16.909, 17.013, 17.096, 17.205, 17.303, 17.411, 17.540,
    17.612, 17.711, 17.811, 17.920, 18.032, 18.111, 18.232, 18.340, 18.418, 18.553, 18.623, 18.728,
    18.827, 18.928,
];

/// Kit 478 `BearAggro`'s one variation, `mBearAggroA.wav`: 3.166 s at 22 050 Hz.
const BEAR_AGGRO_S: f32 = 3.166;

/// Replays a burst through the `[unit+0xb20]` slot and returns the arrivals that sound; the
/// slot frees when its sound ends.
fn barks_that_sound(arrivals: &[f32], clip: f32) -> Vec<f32> {
    let bear = Entity::from_raw_u32(1).expect("valid entity id");
    let mut slot: Option<(Option<Entity>, Latch, f32)> = None;
    let mut sounded = Vec::new();
    for &t in arrivals {
        if slot.is_some_and(|(_, _, ends)| ends <= t) {
            slot = None;
        }
        if slot.is_some_and(|(src, l, _)| occupies_voice_slot(src, l, bear)) {
            continue; // category 0 is the lowest: a live bark wins
        }
        slot = Some((Some(bear), Latch::Voice(BARK_AGGRO), t + clip));
        sounded.push(t);
    }
    sounded
}

/// Through the voice slot the measured burst is five barks, none overlapping.
#[test]
fn the_voice_slot_collapses_the_measured_bear_burst() {
    assert_eq!(
        BEAR_BURST_S.len(),
        63,
        "ungated, every arrival sounds — the reported symptom",
    );

    let sounded = barks_that_sound(&BEAR_BURST_S, BEAR_AGGRO_S);
    assert_eq!(
        sounded,
        vec![0.0, 4.481, 9.046, 12.312, 16.393],
        "the slot admits a bark only once the last one has finished",
    );
    for pair in sounded.windows(2) {
        assert!(
            pair[1] - pair[0] >= BEAR_AGGRO_S,
            "two barks {pair:?} overlap — the phasing the report describes",
        );
    }
}

/// The slot is per unit: a second bear keeps its own voice.
#[test]
fn the_voice_slot_is_per_unit() {
    let (a, b) = (
        Entity::from_raw_u32(1).expect("valid entity id"),
        Entity::from_raw_u32(2).expect("valid entity id"),
    );
    let barking = (Some(a), Latch::Voice(BARK_AGGRO));
    assert!(occupies_voice_slot(barking.0, barking.1, a));
    assert!(!occupies_voice_slot(barking.0, barking.1, b));
}

/// The priority rule at each boundary; refusing an equal state is what collapses a burst.
#[test]
fn a_held_bark_admits_only_a_strictly_higher_state() {
    // A free slot admits everything, including the lowest state.
    for state in [BARK_AGGRO, BARK_PET_ORDER, BARK_PET_ATTACK, BARK_DEATH] {
        assert!(bark_admitted(None, state), "free slot, state {state}");
    }
    // Equal is refused: `jle`, not `jl`.
    for state in [BARK_AGGRO, BARK_PET_ORDER, BARK_PET_ATTACK, BARK_DEATH] {
        assert!(!bark_admitted(Some(state), state), "equal, state {state}");
    }
    // A pet's attack bark cuts through its own aggro flare and order bark.
    assert!(bark_admitted(Some(BARK_AGGRO), BARK_PET_ORDER));
    assert!(bark_admitted(Some(BARK_AGGRO), BARK_PET_ATTACK));
    assert!(bark_admitted(Some(BARK_PET_ORDER), BARK_PET_ATTACK));
    // Nothing cuts through the death cry, the table's maximum.
    assert!(bark_admitted(Some(BARK_PET_ATTACK), BARK_DEATH));
    for state in [BARK_AGGRO, BARK_PET_ORDER, BARK_PET_ATTACK] {
        assert!(
            !bark_admitted(Some(BARK_DEATH), state),
            "over death: {state}"
        );
    }
    // The order bark does not interrupt an attack bark.
    assert!(!bark_admitted(Some(BARK_PET_ATTACK), BARK_PET_ORDER));
}

#[test]
fn death_edges_preserve_health_and_feign_transitions_in_both_layouts() {
    use wrathplus_protocol::{
        field::UNIT_DYNFLAG_DEAD,
        types::{Field, ObjectType},
    };
    let mut edge = FieldChanged {
        entity: Entity::PLACEHOLDER,
        guid: 42,
        kind: ObjectType::Unit,
        field: Field::UnitHealth,
        offset: 0,
        old: 100,
        new: 0,
    };
    assert!(is_death_edge(&edge));
    edge.old = 0;
    assert!(!is_death_edge(&edge));
    edge.old = 100;
    edge.new = 50;
    assert!(!is_death_edge(&edge));
    edge.field = Field::UnitDynamicFlags;
    edge.old = 0;
    edge.new = UNIT_DYNFLAG_DEAD;
    assert!(is_death_edge(&edge));
    edge.kind = ObjectType::Player;
    assert!(is_death_edge(&edge));
    edge.old = UNIT_DYNFLAG_DEAD;
    assert!(!is_death_edge(&edge), "already dead does not cry again");
    edge.old = 0;
    edge.kind = ObjectType::Corpse;
    assert!(!is_death_edge(&edge));
    edge.kind = ObjectType::Unit;
    edge.field = Field::UnitFlags;
    assert!(!is_death_edge(&edge));
}

/// The five-arm jump table, column for column, and its two silences.
#[test]
fn the_bark_table_maps_each_state_to_its_column() {
    let voice = wrathplus_formats::CreatureVoice {
        death: 314,
        aggro: 694,
        alert: 1107,
        pet_attack: 9097,
        pet_order: 9098,
        pet_dismiss: 9096,
        ..Default::default()
    };
    assert_eq!(bark_kit(&voice, BARK_AGGRO), 694);
    assert_eq!(bark_kit(&voice, BARK_PET_ORDER), 9098);
    assert_eq!(bark_kit(&voice, BARK_PET_ATTACK), 9097);
    assert_eq!(bark_kit(&voice, BARK_DEATH), 314);
    // State 3 reads no column and a state past 4 is the range check's default; `alert`
    // (column 13) and `pet_dismiss` (column 30) are reached off this table.
    assert_eq!(bark_kit(&voice, 3), 0);
    assert_eq!(bark_kit(&voice, 5), 0);
    assert_eq!(bark_kit(&voice, 255), 0);
}

/// 12340's 3.3.5-only tags reach their columns (`0x7462e0`, `0x732650`); `$FD6` is silent.
#[test]
fn the_native_anim_tags_reach_their_columns() {
    let voice = wrathplus_formats::CreatureVoice {
        fidget: [1, 2, 3, 4, 5],
        birth: 6,
        spell_cast_directed: 7,
        submerge: 8,
        submerged: 9,
        ..Default::default()
    };
    let tags = [b"$FD5", b"$BRT", b"$SCD", b"$SMG", b"$SMD", b"$FD6"];
    assert_eq!(tags.map(|t| anim_vocal_kit(&voice, t)), [5, 6, 7, 8, 9, 0]);
}
