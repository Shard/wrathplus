use super::*;
use crate::wire::write_packed_guid;
use crate::wrath_events::{WrathDispatch, WrathEventAdapter};

const ME: u64 = 0x0000_0000_0000_0042;
const PET: u64 = 0xF140_0000_0100_0007;

fn words(b: &mut Vec<u8>, values: &[u32]) {
    for v in values {
        b.extend(v.to_le_bytes());
    }
}
fn strict(opcode: u16, b: &[u8]) {
    for end in 0..b.len() {
        assert!(
            decode(opcode, &b[..end], Some(ME)).is_err(),
            "{opcode:#x} prefix {end}"
        );
    }
    let mut extra = b.to_vec();
    extra.push(0);
    assert!(
        decode(opcode, &extra, Some(ME)).is_err(),
        "{opcode:#x} trailing"
    );
}
fn one(opcode: u16, b: &[u8]) -> SessionEvent {
    let mut events = decode(opcode, b, Some(ME)).unwrap();
    assert_eq!(events.len(), 1);
    strict(opcode, b);
    events.remove(0)
}

#[test]
fn cooldown_event_and_clear_read_spell_then_raw_guid() {
    let mut b = Vec::new();
    words(&mut b, &[5384]);
    b.extend(ME.to_le_bytes());
    assert!(matches!(
        one(SMSG_COOLDOWN_EVENT, &b),
        SessionEvent::CooldownEvent {
            spell_id: 5384,
            caster: ME
        }
    ));
    let mut b = Vec::new();
    words(&mut b, &[17253]);
    b.extend(PET.to_le_bytes());
    assert!(matches!(
        one(SMSG_CLEAR_COOLDOWN, &b),
        SessionEvent::ClearCooldown {
            spell_id: 17253,
            caster: PET
        }
    ));
}

#[test]
fn only_our_channel_drives_the_shared_bar_others_are_captured() {
    let start = |caster| {
        let mut b = Vec::new();
        write_packed_guid(caster, &mut b).unwrap();
        words(&mut b, &[5143, 5000]);
        b
    };
    assert!(matches!(
        one(MSG_CHANNEL_START, &start(ME)),
        SessionEvent::ChannelStart {
            spell_id: 5143,
            duration_ms: 5000
        }
    ));
    let SessionEvent::WrathSpellControl { packet } = one(MSG_CHANNEL_START, &start(PET)) else {
        panic!()
    };
    assert_eq!(
        packet,
        Packet::ChannelStart {
            caster: PET,
            spell_id: 5143,
            duration_ms: 5000
        }
    );
    // Before our create arrives nothing is ours.
    assert!(matches!(
        decode(MSG_CHANNEL_START, &start(ME), None).unwrap()[..],
        [SessionEvent::WrathSpellControl { .. }]
    ));
    let update = |caster| {
        let mut b = Vec::new();
        write_packed_guid(caster, &mut b).unwrap();
        words(&mut b, &[1200]);
        b
    };
    assert!(matches!(
        one(MSG_CHANNEL_UPDATE, &update(ME)),
        SessionEvent::ChannelUpdate { remaining_ms: 1200 }
    ));
    assert!(matches!(
        one(MSG_CHANNEL_UPDATE, &update(PET)),
        SessionEvent::WrathSpellControl {
            packet: Packet::ChannelUpdate {
                caster: PET,
                remaining_ms: 1200
            }
        }
    ));
}

fn resurrect(name: &str, sickness: u8, timer: bool) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(PET.to_le_bytes());
    words(&mut b, &[name.len() as u32 + 1]);
    b.extend(name.as_bytes());
    b.extend([0, 0, sickness]);
    if timer {
        words(&mut b, &[0]);
    }
    b
}

#[test]
fn resurrect_request_frames_both_terminators_and_the_optional_delay() {
    let SessionEvent::ResurrectRequest {
        caster,
        name,
        sickness,
        has_timer,
    } = one(
        SMSG_RESURRECT_REQUEST,
        &resurrect("Spirit Healer", 1, false),
    )
    else {
        panic!()
    };
    assert_eq!(
        (caster, name.as_str(), sickness, has_timer),
        (PET, "Spirit Healer", true, false)
    );
    let mut events = decode(SMSG_RESURRECT_REQUEST, &resurrect("", 0, true), Some(ME)).unwrap();
    assert!(matches!(
        events.remove(0),
        SessionEvent::ResurrectRequest {
            sickness: false,
            has_timer: true,
            ..
        }
    ));
    // A length that disagrees with the name, or a partial delay word, is rejected.
    let mut bad = resurrect("Abc", 0, false);
    bad[8] = 9;
    assert!(decode(SMSG_RESURRECT_REQUEST, &bad, Some(ME)).is_err());
    let mut partial = resurrect("", 0, false);
    partial.extend([0, 0]);
    assert!(decode(SMSG_RESURRECT_REQUEST, &partial, Some(ME)).is_err());
}

#[test]
fn pet_refusals_and_sounds_keep_their_shared_shapes() {
    assert!(matches!(
        one(SMSG_PET_TAME_FAILURE, &[12]),
        SessionEvent::PetTameFailure { reason: 12 }
    ));
    assert!(matches!(
        one(SMSG_PET_ACTION_FEEDBACK, &[2]),
        SessionEvent::PetActionFeedback { reason: 2 }
    ));
    let mut b = PET.to_le_bytes().to_vec();
    words(&mut b, &[1]);
    assert!(matches!(
        one(SMSG_PET_ACTION_SOUND, &b),
        SessionEvent::PetActionSound {
            pet_guid: PET,
            talk: 1
        }
    ));
    let mut b = Vec::new();
    words(&mut b, &[904]);
    for f in [1.0f32, 2.0, 3.0] {
        b.extend(f.to_le_bytes());
    }
    assert!(matches!(
        one(SMSG_PET_DISMISS_SOUND, &b),
        SessionEvent::PetDismissSound {
            model_id: 904,
            position: [1.0, 2.0, 3.0]
        }
    ));
}

#[test]
fn pet_name_invalid_keeps_reason_name_and_declined_forms() {
    let mut b = Vec::new();
    words(&mut b, &[3]);
    b.extend(b"Al\0");
    b.push(0);
    let SessionEvent::PetNameInvalid {
        reason,
        name,
        declined,
    } = one(SMSG_PET_NAME_INVALID, &b)
    else {
        panic!()
    };
    assert_eq!((reason, name.as_str(), declined), (3, "Al", None));
    let mut b = Vec::new();
    words(&mut b, &[16]);
    b.extend(b"Rex\0");
    b.push(1);
    for form in ["a", "b", "c", "d", "e"] {
        b.extend(form.as_bytes());
        b.push(0);
    }
    let SessionEvent::PetNameInvalid { declined, .. } = one(SMSG_PET_NAME_INVALID, &b) else {
        panic!()
    };
    assert_eq!(
        declined.as_deref(),
        Some(&["a", "b", "c", "d", "e"].map(String::from))
    );
}

#[test]
fn visual_modifiers_combo_and_totem() {
    let mut b = ME.to_le_bytes().to_vec();
    words(&mut b, &[406]);
    assert!(matches!(
        one(SMSG_PLAY_SPELL_VISUAL, &b),
        SessionEvent::PlaySpellVisual {
            unit: ME,
            kit_id: 406
        }
    ));
    assert!(matches!(
        one(SMSG_PLAY_SPELL_IMPACT, &b),
        SessionEvent::WrathSpellControl {
            packet: Packet::SpellImpact {
                target: ME,
                kit_id: 406
            }
        }
    ));
    // Build 12340's third family word: bit 70 is on the wire as-is.
    let mut b = vec![70, 10];
    b.extend((-15i32).to_le_bytes());
    assert!(matches!(
        one(SMSG_SET_PCT_SPELL_MODIFIER, &b),
        SessionEvent::SpellModifier {
            flat: false,
            mask_bit: 70,
            op: 10,
            value: -15
        }
    ));
    assert!(matches!(
        one(SMSG_SET_FLAT_SPELL_MODIFIER, &b),
        SessionEvent::SpellModifier { flat: true, .. }
    ));
    let mut b = Vec::new();
    write_packed_guid(PET, &mut b).unwrap();
    b.push(3);
    assert!(matches!(
        one(SMSG_UPDATE_COMBO_POINTS, &b),
        SessionEvent::ComboPoints {
            target: PET,
            points: 3
        }
    ));
    assert!(matches!(
        one(SMSG_UPDATE_COMBO_POINTS, &[0, 0]),
        SessionEvent::ComboPoints {
            target: 0,
            points: 0
        }
    ));
    let mut b = vec![1];
    b.extend(PET.to_le_bytes());
    words(&mut b, &[120_000, 8071]);
    assert!(matches!(
        one(SMSG_TOTEM_CREATED, &b),
        SessionEvent::WrathSpellControl {
            packet: Packet::TotemCreated {
                slot: 1,
                totem: PET,
                duration_ms: 120_000,
                spell_id: 8071
            }
        }
    ));
}

#[test]
fn requests_write_the_bodies_ac_reads() {
    let guid = PET.to_le_bytes();
    let cases: Vec<(Command, u32, Vec<u8>)> = vec![
        (
            Command::CancelChannelling { spell_id: 5143 },
            CMSG_CANCEL_CHANNELLING,
            5143u32.to_le_bytes().to_vec(),
        ),
        (
            Command::ResurrectResponse {
                caster: PET,
                accept: true,
            },
            CMSG_RESURRECT_RESPONSE,
            [&guid[..], &[1]].concat(),
        ),
        (
            Command::ResurrectResponse {
                caster: PET,
                accept: false,
            },
            CMSG_RESURRECT_RESPONSE,
            [&guid[..], &[0]].concat(),
        ),
        (
            Command::PetSetAction {
                pet: PET,
                entries: vec![(3, 0xC100_0001)],
            },
            CMSG_PET_SET_ACTION,
            [
                &guid[..],
                &3u32.to_le_bytes(),
                &0xC100_0001u32.to_le_bytes(),
            ]
            .concat(),
        ),
        (
            Command::PetAction {
                pet: PET,
                word: 0x0700_0003,
                target: ME,
            },
            CMSG_PET_ACTION,
            [&guid[..], &0x0700_0003u32.to_le_bytes(), &ME.to_le_bytes()].concat(),
        ),
        (
            Command::PetAbandon { pet: PET },
            CMSG_PET_ABANDON,
            guid.to_vec(),
        ),
        (
            Command::PetRename {
                pet: PET,
                name: "Rex".into(),
            },
            CMSG_PET_RENAME,
            [&guid[..], b"Rex\0", &[0]].concat(),
        ),
        (
            Command::PetCancelAura {
                pet: PET,
                spell_id: 7,
            },
            CMSG_PET_CANCEL_AURA,
            [&guid[..], &7u32.to_le_bytes()].concat(),
        ),
        (
            Command::CancelAutoRepeatSpell,
            CMSG_CANCEL_AUTO_REPEAT_SPELL,
            vec![],
        ),
        (Command::RequestPetInfo, CMSG_REQUEST_PET_INFO, vec![]),
        (Command::CancelGrowthAura, CMSG_CANCEL_GROWTH_AURA, vec![]),
        (
            Command::PetStopAttack { pet: PET },
            CMSG_PET_STOP_ATTACK,
            guid.to_vec(),
        ),
        (
            Command::PetSpellAutocast {
                pet: PET,
                spell_id: 7,
                enabled: true,
            },
            CMSG_PET_SPELL_AUTOCAST,
            [&guid[..], &7u32.to_le_bytes(), &[1]].concat(),
        ),
        (Command::CancelMountAura, CMSG_CANCEL_MOUNT_AURA, vec![]),
        (
            Command::SpellClick { unit: PET },
            CMSG_SPELLCLICK,
            guid.to_vec(),
        ),
        (
            Command::TotemDestroyed { slot: 2 },
            CMSG_TOTEM_DESTROYED,
            vec![2],
        ),
        (
            Command::PetLearnTalent {
                pet: PET,
                talent_id: 2107,
                rank: 0,
            },
            CMSG_PET_LEARN_TALENT,
            [&guid[..], &2107u32.to_le_bytes(), &0u32.to_le_bytes()].concat(),
        ),
    ];
    for (command, opcode, body) in cases {
        assert_eq!(encode(&command).unwrap(), (opcode, body), "{command:?}");
    }
    // AC counts set-action entries by body size: 16 or 24 bytes only.
    let moved = Command::PetSetAction {
        pet: PET,
        entries: vec![(3, 1), (4, 2)],
    };
    assert_eq!(encode(&moved).unwrap().1.len(), 24);
    for entries in [vec![], vec![(0, 0); 3]] {
        assert!(encode(&Command::PetSetAction { pet: PET, entries }).is_err());
    }
    assert!(encode(&Command::PetRename {
        pet: PET,
        name: "a\0b".into()
    })
    .is_err());
}

#[test]
fn dispatch_routes_the_family_and_leaves_registry_only_rows_undecoded() {
    let mut adapter = WrathEventAdapter::new();
    let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(SMSG_PET_TAME_FAILURE, &[4], 0).unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        session_events[..],
        [SessionEvent::PetTameFailure { reason: 4 }]
    ));
    assert!(matches!(
        adapter.dispatch(SMSG_PET_TAME_FAILURE, &[4, 0], 0).unwrap(),
        WrathDispatch::Unhandled { .. }
    ));
    for opcode in REGISTRY_ONLY {
        assert!(!handles(opcode), "{opcode:#x}");
        assert!(
            !matches!(
                adapter.dispatch(opcode, &[0; 16], 0),
                Ok(WrathDispatch::Handled { .. })
            ),
            "{opcode:#x}"
        );
    }
}
