use super::*;

fn w(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn q(v: u64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn debug(events: &[SessionEvent]) -> String {
    // `SessionEvent` has no `PartialEq`; its `Debug` form is total over the fields.
    format!("{events:?}")
}

fn fixtures() -> Vec<(u16, Vec<u8>, Vec<SessionEvent>)> {
    vec![
        (0x103, [w(10), q(9)].concat(), vec![SessionEvent::Emote { emote_id: 10, guid: 9 }]),
        (
            0x105,
            [q(9), w(101), w(0), w(3), b"Bob\0".to_vec()].concat(),
            vec![SessionEvent::TextEmote { guid: 9, text_emote: 101, target_name: "Bob".into() }],
        ),
        (
            0x105,
            [q(9), w(101), w(0), w(0), vec![0]].concat(),
            vec![SessionEvent::TextEmote { guid: 9, text_emote: 101, target_name: String::new() }],
        ),
        (0x1cb, b"Hi\0".to_vec(), vec![SessionEvent::Notification { text: "Hi".into() }]),
        (0x219, vec![], vec![SessionEvent::ChatWrongFaction]),
        (0x254, w(12), vec![SessionEvent::ZoneUnderAttack { area_id: 12 }]),
        (
            0x291,
            [w(3), b"5\0".to_vec()].concat(),
            vec![SessionEvent::ServerMessage { message_type: 3, text: "5".into() }],
        ),
        (0x2a9, b"Bob\0".to_vec(), vec![SessionEvent::ChatPlayerNotFound { name: "Bob".into() }]),
        (
            0x2b8,
            [w(3), b"Go\0".to_vec()].concat(),
            vec![SessionEvent::AreaTriggerMessage { text: "Go".into() }],
        ),
        (0x2d2, w(888), vec![SessionEvent::PlaySound { sound_id: 888 }]),
        (0x2fd, vec![1], vec![SessionEvent::ChatRestricted]),
        (
            0x32d,
            b"Bob\0".to_vec(),
            vec![SessionEvent::WrathChatNotice { packet: Packet::PlayerAmbiguous { name: "Bob".into() } }],
        ),
        (0x33d, [w(2), b"Welcome\0Rules\0".to_vec()].concat(), vec![system_line("Welcome".into()), system_line("Rules".into())]),
        (0x33d, w(0), vec![]),
    ]
}

#[test]
fn every_fixture_maps_to_its_shared_events_and_rejects_bad_extent() {
    let fixtures = fixtures();
    let covered: std::collections::BTreeSet<_> = fixtures.iter().map(|f| f.0).collect();
    assert_eq!(covered, OPCODES.into_iter().collect());
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in fixtures {
        assert_eq!(debug(&decode(opcode, &body).unwrap()), debug(&expected), "{opcode:#x}");
        for end in 0..body.len() {
            assert!(decode(opcode, &body[..end]).is_err(), "{opcode:#x} truncated at {end}");
        }
        let trailing = [body.as_slice(), &[0]].concat();
        assert!(decode(opcode, &trailing).is_err(), "{opcode:#x} trailing");
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(debug(&session_events), debug(&expected), "{opcode:#x}")
            }
            other => panic!("{opcode:#x}: {other:?}"),
        }
    }
    // A one-byte name is sent as a lone NUL; a mismatched name length is malformed.
    assert!(decode(0x105, &[q(9), w(1), w(0), w(3), b"Bo\0".to_vec()].concat()).is_err());
    assert!(decode(0x2b8, &[w(9), b"Go\0".to_vec()].concat()).is_err());
}

#[test]
fn channel_notices_follow_ac_tails_and_leave_joined_left_to_wrath_channel() {
    let name = b"Trade\0".to_vec();
    let notice = |n: u8, tail: Vec<u8>| [vec![n], name.clone(), tail].concat();
    let rows = [
        (0x00, q(9), ChannelNoticeTail::Guid(9)),
        (0x07, q(9), ChannelNoticeTail::Actor(9)),
        (0x0d, q(9), ChannelNoticeTail::Actor(9)),
        (0x0f, q(9), ChannelNoticeTail::Actor(9)),
        (0x22, q(9), ChannelNoticeTail::Actor(9)),
        (0x11, vec![], ChannelNoticeTail::Empty),
        (0x13, vec![], ChannelNoticeTail::Empty),
        (0x21, vec![], ChannelNoticeTail::Empty),
        (0x0b, b"Nobody\0".to_vec(), ChannelNoticeTail::Name("Nobody".into())),
        (0x0c, [q(9), vec![1, 2]].concat(), ChannelNoticeTail::ModeChange { guid: 9, old_flags: 1, new_flags: 2 }),
        (0x12, [q(9), q(8)].concat(), ChannelNoticeTail::Actors { target: 9, source: 8 }),
    ];
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (n, tail, expected) in rows {
        let body = notice(n, tail);
        let expected = SessionEvent::ChannelNotify { notice: n, channel: "Trade".into(), tail: expected };
        assert_eq!(format!("{:?}", decode_channel_notify(&body).unwrap()), format!("{expected:?}"));
        assert!(decode_channel_notify(&[body.as_slice(), &[0]].concat()).is_err());
        assert!(decode_channel_notify(&body[..body.len() - 1]).is_err() || body.len() == name.len() + 1);
        match adapter.dispatch(0x99, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(debug(&session_events), debug(&[expected]))
            }
            other => panic!("{n:#x}: {other:?}"),
        }
    }
    assert!(decode_channel_notify(&notice(0x24, vec![])).is_err());
}

#[test]
fn chat_lines_translate_types_and_tags_and_never_addon_traffic() {
    use crate::messages as m;
    let line = |chat_type: u8, language: u32, tag: u8| crate::wrath_chat::Message {
        gm: false,
        chat_type,
        language,
        sender_guid: 9,
        flags: 0,
        sender_name: None,
        receiver_guid: 8,
        receiver_name: None,
        channel: (chat_type == 0x11).then(|| b"Trade".to_vec()),
        text: b"hi".to_vec(),
        chat_tag: tag,
        achievement_id: None,
    };
    for (wrath, shared) in [
        (0x00, m::CHAT_MSG_SYSTEM),
        (0x01, m::CHAT_MSG_SAY),
        (0x07, m::CHAT_MSG_WHISPER),
        (0x09, m::CHAT_MSG_WHISPER_INFORM),
        (0x11, m::CHAT_MSG_CHANNEL),
        (0x28, m::CHAT_MSG_RAID_WARNING),
        (0x33, m::CHAT_MSG_PARTY),
    ] {
        let out = crate::wrath_chat::shared_message(&line(wrath, 7, 0)).unwrap();
        assert_eq!((out.chat_type, out.sender_guid, out.target_guid, out.text.as_str()), (shared, 9, 8, "hi"));
    }
    let channel = crate::wrath_chat::shared_message(&line(0x11, 7, 4 | 1)).unwrap();
    assert_eq!((channel.channel.as_deref(), channel.chat_tag), (Some("Trade"), m::chat_tag::GM));
    assert!(crate::wrath_chat::shared_message(&line(0x02, crate::wrath_chat::LANGUAGE_ADDON, 0)).is_none());
    assert!(crate::wrath_chat::shared_message(&line(0x30, 0, 0)).is_none(), "achievement lines stay raw");
}

#[test]
fn commands_encode_ac_read_order() {
    for (command, opcode, body) in [
        (
            Command::Join { channel_id: 2, name: "Trade - City".into(), password: String::new() },
            0x97,
            [w(2), vec![0, 0], b"Trade - City\0\0".to_vec()].concat(),
        ),
        (Command::Leave { name: "x".into() }, 0x98, [w(0), b"x\0".to_vec()].concat()),
        (Command::Named { opcode: 0x9a, name: "x".into() }, 0x9a, b"x\0".to_vec()),
        (Command::NamedPair { opcode: 0xa4, name: "x".into(), other: "Bob".into() }, 0xa4, b"x\0Bob\0".to_vec()),
        (Command::TextEmote { text_emote: 101, emote_num: 0, target: 9 }, 0x104, [w(101), w(0), q(9)].concat()),
        (Command::ChatIgnored { guid: 9 }, 0x225, [q(9), vec![0]].concat()),
    ] {
        assert_eq!(encode(&command), (opcode, body), "{command:?}");
    }
}
