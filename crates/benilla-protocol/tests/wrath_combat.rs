use benilla_protocol::events::SessionEvent;
use benilla_protocol::messages::AttackSwingError;
use benilla_protocol::wrath_combat::{
    decode_ai_reaction, decode_attack_start, decode_attack_stop, decode_attack_swing_refusal,
    decode_attacker_state, encode_attack_stop, encode_attack_swing, encode_set_selection,
    encode_set_sheathed, AttackSwingRefusal, CombatCodecError, SheathState, HITINFO_BLOCK,
    HITINFO_FULL_ABSORB, HITINFO_PARTIAL_RESIST, HITINFO_RAGE_GAIN, HITINFO_UNK1, SMSG_AI_REACTION,
    SMSG_ATTACKER_STATE_UPDATE, SMSG_ATTACK_START, SMSG_ATTACK_STOP, SMSG_ATTACK_SWING_BAD_FACING,
    SMSG_ATTACK_SWING_CANT_ATTACK, SMSG_ATTACK_SWING_DEAD_TARGET, SMSG_ATTACK_SWING_NOT_IN_RANGE,
};
use benilla_protocol::wrath_events::{WrathDispatch, WrathEvent, WrathEventAdapter};

#[test]
fn native_stand_and_cancel_bodies_are_exact_and_adapter_uses_shared_semantics() {
    use benilla_protocol::wrath_combat::*;
    for state in [0, 1, 3, 8] {
        assert_eq!(
            encode_stand_state_change(state).unwrap(),
            state.to_le_bytes()
        );
    }
    for state in [2, 4, 9, u32::MAX] {
        assert!(encode_stand_state_change(state).is_err());
    }
    assert_eq!(decode_stand_state_update(&[1]).unwrap(), 1);
    assert!(decode_stand_state_update(&[]).is_err());
    assert!(decode_stand_state_update(&[1, 0]).is_err());
    assert!(decode_stand_state_update(&[10]).is_err());
    assert!(decode_cancel_combat(&[]).is_ok());
    assert!(decode_cancel_combat(&[0]).is_err());
    let mut adapter = WrathEventAdapter::new();
    let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(SMSG_STAND_STATE_UPDATE, &[1], 0).unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        &session_events[..],
        [SessionEvent::StandStateUpdate { state: 1 }]
    ));
    let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(SMSG_CANCEL_COMBAT, &[], 0).unwrap()
    else {
        panic!()
    };
    assert!(matches!(&session_events[..], [SessionEvent::CancelCombat]));
}

const ATTACKER: u64 = 0xf130_0001_0000_0011;
const VICTIM: u64 = 0x0000_0000_0000_0022;

fn packed_guid(guid: u64) -> Vec<u8> {
    let bytes = guid.to_le_bytes();
    let mut mask = 0u8;
    let mut packed = Vec::new();
    for (index, byte) in bytes.into_iter().enumerate() {
        if byte != 0 {
            mask |= 1 << index;
            packed.push(byte);
        }
    }
    let mut out = vec![mask];
    out.extend(packed);
    out
}

fn ordinary_attacker_state(hit_info: u32, count: u8) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(hit_info.to_le_bytes());
    body.extend(packed_guid(ATTACKER));
    body.extend(packed_guid(VICTIM));
    body.extend(70u32.to_le_bytes());
    body.extend(9u32.to_le_bytes());
    body.push(count);
    for index in 0..count {
        let damage = 50 + u32::from(index) * 20;
        // Oversized-count fixtures must reach the decoder rather than overflow here.
        body.extend(
            1u32.checked_shl(u32::from(index))
                .unwrap_or(1)
                .to_le_bytes(),
        );
        body.extend((damage as f32).to_le_bytes());
        body.extend(damage.to_le_bytes());
    }
    if hit_info & HITINFO_FULL_ABSORB != 0 {
        for index in 0..count {
            body.extend((3 + u32::from(index)).to_le_bytes());
        }
    }
    if hit_info & HITINFO_PARTIAL_RESIST != 0 {
        for index in 0..count {
            body.extend((5 + u32::from(index)).to_le_bytes());
        }
    }
    body.push(1);
    body.extend(0u32.to_le_bytes());
    body.extend(0u32.to_le_bytes());
    if hit_info & HITINFO_BLOCK != 0 {
        body.extend(7u32.to_le_bytes());
    }
    if hit_info & HITINFO_RAGE_GAIN != 0 {
        body.extend(11u32.to_le_bytes());
    }
    body
}

fn every_truncated_prefix_fails<T: std::fmt::Debug>(
    fixture: &[u8],
    decode: impl Fn(&[u8]) -> Result<T, CombatCodecError>,
) {
    for end in 0..fixture.len() {
        assert_eq!(
            decode(&fixture[..end]).unwrap_err(),
            CombatCodecError::Truncated,
            "prefix {end}"
        );
    }
}

#[test]
fn client_target_attack_stop_and_sheath_bodies_match_handlers() {
    assert_eq!(encode_set_selection(ATTACKER), ATTACKER.to_le_bytes());
    assert_eq!(encode_attack_swing(VICTIM), VICTIM.to_le_bytes());
    assert!(encode_attack_stop().is_empty());
    assert_eq!(
        encode_set_sheathed(SheathState::Unarmed),
        0u32.to_le_bytes()
    );
    assert_eq!(encode_set_sheathed(SheathState::Melee), 1u32.to_le_bytes());
    assert_eq!(encode_set_sheathed(SheathState::Ranged), 2u32.to_le_bytes());
}

#[test]
fn attack_start_stop_and_ai_reaction_use_reference_guid_forms() {
    let mut start = ATTACKER.to_le_bytes().to_vec();
    start.extend(VICTIM.to_le_bytes());
    assert_eq!(decode_attack_start(&start).unwrap().attacker, ATTACKER);
    every_truncated_prefix_fails(&start, decode_attack_start);
    let mut trailing = start.clone();
    trailing.push(0);
    assert_eq!(
        decode_attack_start(&trailing).unwrap_err(),
        CombatCodecError::TrailingBytes(1)
    );

    let mut stop = packed_guid(ATTACKER);
    stop.extend(packed_guid(VICTIM));
    stop.extend(1u32.to_le_bytes());
    let decoded = decode_attack_stop(&stop).unwrap();
    assert_eq!(
        (decoded.attacker, decoded.victim, decoded.now_dead),
        (ATTACKER, VICTIM, true)
    );
    every_truncated_prefix_fails(&stop, decode_attack_stop);
    let mut trailing = stop.clone();
    trailing.push(0);
    assert_eq!(
        decode_attack_stop(&trailing).unwrap_err(),
        CombatCodecError::TrailingBytes(1)
    );

    let mut reaction = ATTACKER.to_le_bytes().to_vec();
    reaction.extend(2u32.to_le_bytes());
    assert_eq!(decode_ai_reaction(&reaction).unwrap().reaction, 2);
    every_truncated_prefix_fails(&reaction, decode_ai_reaction);
    reaction.push(0);
    assert_eq!(
        decode_ai_reaction(&reaction).unwrap_err(),
        CombatCodecError::TrailingBytes(1)
    );
}

#[test]
fn refusal_opcodes_are_empty_and_distinct_natively() {
    for (opcode, expected) in [
        (
            SMSG_ATTACK_SWING_NOT_IN_RANGE,
            AttackSwingRefusal::NotInRange,
        ),
        (SMSG_ATTACK_SWING_BAD_FACING, AttackSwingRefusal::BadFacing),
        (
            SMSG_ATTACK_SWING_DEAD_TARGET,
            AttackSwingRefusal::DeadTarget,
        ),
        (
            SMSG_ATTACK_SWING_CANT_ATTACK,
            AttackSwingRefusal::CantAttack,
        ),
    ] {
        assert_eq!(decode_attack_swing_refusal(opcode, &[]).unwrap(), expected);
        assert_eq!(
            decode_attack_swing_refusal(opcode, &[0]).unwrap_err(),
            CombatCodecError::TrailingBytes(1)
        );
    }
    assert_eq!(
        decode_attack_swing_refusal(0xffff, &[]).unwrap_err(),
        CombatCodecError::UnsupportedOpcode(0xffff)
    );
}

#[test]
fn attacker_state_retains_wrath_metadata_and_conditional_arrays() {
    let flags = HITINFO_FULL_ABSORB | HITINFO_PARTIAL_RESIST | HITINFO_BLOCK | HITINFO_RAGE_GAIN;
    let fixture = ordinary_attacker_state(flags, 2);
    let state = decode_attacker_state(&fixture).unwrap();
    assert_eq!(state.hit_info, flags);
    assert_eq!((state.attacker, state.victim), (ATTACKER, VICTIM));
    assert_eq!((state.damage, state.overkill), (70, 9));
    assert_eq!(state.damages.len(), 2);
    assert_eq!(
        (
            state.damages[0].school_mask,
            state.damages[0].damage_float,
            state.damages[0].damage
        ),
        (1, 50.0, 50)
    );
    assert_eq!(
        (state.damages[1].absorb, state.damages[1].resist),
        (Some(4), Some(6))
    );
    assert_eq!(
        (state.target_state, state.blocked, state.rage_gain),
        (1, Some(7), Some(11))
    );

    every_truncated_prefix_fails(&fixture, decode_attacker_state);
    let mut trailing = fixture.clone();
    trailing.push(0);
    assert_eq!(
        decode_attacker_state(&trailing).unwrap_err(),
        CombatCodecError::TrailingBytes(1)
    );
}

#[test]
fn attacker_state_rejects_unbounded_or_unproven_shapes() {
    for count in [0, 3, u8::MAX] {
        assert_eq!(
            decode_attacker_state(&ordinary_attacker_state(0, count)).unwrap_err(),
            CombatCodecError::UnsupportedDamageCount(count)
        );
    }

    let mut bad_target = ordinary_attacker_state(0, 1);
    let target_at = bad_target.len() - 9;
    bad_target[target_at] = 9;
    assert_eq!(
        decode_attacker_state(&bad_target).unwrap_err(),
        CombatCodecError::UnsupportedTargetState(9)
    );

    let mut bad_attacker = ordinary_attacker_state(0, 1);
    let attacker_at = bad_attacker.len() - 8;
    bad_attacker[attacker_at..attacker_at + 4].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        decode_attacker_state(&bad_attacker).unwrap_err(),
        CombatCodecError::UnsupportedAttackerState(1)
    );

    assert_eq!(
        decode_attacker_state(&ordinary_attacker_state(HITINFO_UNK1, 1)).unwrap_err(),
        CombatCodecError::UnsupportedHitInfo(HITINFO_UNK1)
    );
    assert_eq!(
        decode_attacker_state(&ordinary_attacker_state(0x8000_0000, 1)).unwrap_err(),
        CombatCodecError::UnsupportedHitInfo(0x8000_0000)
    );
}

#[test]
fn adapter_projects_only_lossless_neutral_combat_semantics() {
    let mut adapter = WrathEventAdapter::new();
    let mut start = ATTACKER.to_le_bytes().to_vec();
    start.extend(VICTIM.to_le_bytes());
    let WrathDispatch::Handled {
        session_events,
        wrath_events,
        ..
    } = adapter.dispatch(SMSG_ATTACK_START, &start, 0).unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        &session_events[..],
        [SessionEvent::AttackStart {
            attacker: ATTACKER,
            victim: VICTIM
        }]
    ));
    assert!(wrath_events.is_empty());

    let mut stop = packed_guid(ATTACKER);
    stop.extend(packed_guid(VICTIM));
    stop.extend(1u32.to_le_bytes());
    let WrathDispatch::Handled {
        session_events,
        wrath_events,
        ..
    } = adapter.dispatch(SMSG_ATTACK_STOP, &stop, 0).unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        &session_events[..],
        [SessionEvent::AttackStop {
            attacker: ATTACKER,
            victim: VICTIM
        }]
    ));
    assert!(matches!(&wrath_events[..], [WrathEvent::AttackStop(stop)] if stop.now_dead));

    let WrathDispatch::Handled {
        session_events,
        wrath_events,
        ..
    } = adapter
        .dispatch(SMSG_ATTACK_SWING_DEAD_TARGET, &[], 0)
        .unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        &session_events[..],
        [SessionEvent::AttackSwingError(
            AttackSwingError::DeadOrUnattackable
        )]
    ));
    assert!(matches!(
        &wrath_events[..],
        [WrathEvent::AttackSwingRefusal(
            AttackSwingRefusal::DeadTarget
        )]
    ));

    let swing = ordinary_attacker_state(0, 1);
    let WrathDispatch::Handled {
        session_events,
        wrath_events,
        ..
    } = adapter
        .dispatch(SMSG_ATTACKER_STATE_UPDATE, &swing, 0)
        .unwrap()
    else {
        panic!()
    };
    assert!(
        matches!(&session_events[..], [SessionEvent::WrathAttackerState(state)] if state.overkill == 9),
        "native metadata stays in an explicitly Wrath-typed event, not Vanilla AttackerState"
    );
    assert!(matches!(&wrath_events[..], [WrathEvent::AttackerState(state)] if state.overkill == 9));

    let mut reaction = ATTACKER.to_le_bytes().to_vec();
    reaction.extend(2u32.to_le_bytes());
    let WrathDispatch::Handled {
        session_events,
        wrath_events,
        ..
    } = adapter.dispatch(SMSG_AI_REACTION, &reaction, 0).unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        &session_events[..],
        [SessionEvent::AiReaction {
            unit: ATTACKER,
            reaction: 2
        }]
    ));
    assert!(matches!(&wrath_events[..], [WrathEvent::AiReaction(value)] if value.reaction == 2));
}
