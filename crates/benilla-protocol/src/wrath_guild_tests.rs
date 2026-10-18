use super::*;
fn word(b: &mut Vec<u8>, n: u32) {
    b.extend(n.to_le_bytes());
}
fn roster() -> Vec<u8> {
    let mut b = 2u32.to_le_bytes().to_vec();
    b.extend(b"motd\0info\0");
    word(&mut b, 1);
    for i in 0..14 {
        word(&mut b, u32::MAX - i);
    }
    for status in [0, 255] {
        b.extend(99u64.to_le_bytes());
        b.push(status);
        b.extend([255, 0]);
        word(&mut b, u32::MAX);
        b.extend([80, 3, 254]);
        word(&mut b, 0x80000000);
        if status == 0 {
            word(&mut b, 0x7fc01234);
        }
        b.extend(b"note\0officer\0");
    }
    b
}
#[test]
fn roster_preserves_all_rank_words_gender_raw_names_and_float_bits() {
    let Packet::Roster {
        motd,
        info,
        ranks,
        members,
    } = decode(0x8a, &roster()).unwrap()
    else {
        panic!()
    };
    assert_eq!(motd, b"motd");
    assert_eq!(info, b"info");
    assert_eq!(
        ranks,
        vec![Rank {
            flags: u32::MAX,
            money_limit: u32::MAX - 1,
            tabs: std::array::from_fn(|i| (
                u32::MAX - 2 - i as u32 * 2,
                u32::MAX - 3 - i as u32 * 2
            ))
        }]
    );
    assert_eq!(members.len(), 2);
    for (i, m) in members.iter().enumerate() {
        assert_eq!(m.guid, 99);
        assert_eq!(m.name, vec![255]);
        assert_eq!(m.rank, u32::MAX);
        assert_eq!(
            (m.level, m.class, m.gender, m.area),
            (80, 3, 254, 0x80000000)
        );
        assert_eq!(
            m.last_save_bits,
            if i == 0 { Some(0x7fc01234) } else { None }
        );
        assert_eq!(m.note, b"note");
        assert_eq!(m.officer_note, b"officer");
    }
    let mut empty = 0u32.to_le_bytes().to_vec();
    empty.extend([0, 0]);
    word(&mut empty, 0);
    assert!(
        matches!(decode(0x8a,&empty).unwrap(),Packet::Roster { ranks,members,.. } if ranks.is_empty() && members.is_empty())
    );
}
#[test]
fn query_info_invite_results_and_event_guid_branches_are_native() {
    let mut b = 7u32.to_le_bytes().to_vec();
    b.extend([255, 0]);
    for i in 0..10 {
        b.extend([b'a' + i, 0]);
    }
    for i in 0..6 {
        word(&mut b, u32::MAX - i);
    }
    let Packet::Query {
        guild_id,
        name,
        ranks,
        emblem,
        rank_count,
    } = decode(0x55, &b).unwrap()
    else {
        panic!()
    };
    assert_eq!(guild_id, 7);
    assert_eq!(name, vec![255]);
    assert_eq!(ranks[9], b"j");
    assert_eq!(
        emblem,
        [
            u32::MAX,
            u32::MAX - 1,
            u32::MAX - 2,
            u32::MAX - 3,
            u32::MAX - 4
        ]
    );
    assert_eq!(rank_count, u32::MAX - 5);
    let mut b = b"guild\0".to_vec();
    for n in [0xffffffff, 0x80000000, 99] {
        word(&mut b, n);
    }
    assert_eq!(
        decode(0x88, &b).unwrap(),
        Packet::Info {
            name: b"guild".to_vec(),
            packed_date: u32::MAX,
            members: 0x80000000,
            accounts: 99
        }
    );
    assert_eq!(
        decode(0x83, b"inviter\0guild\0").unwrap(),
        Packet::Invite {
            inviter: b"inviter".to_vec(),
            guild: b"guild".to_vec()
        }
    );
    let mut b = u32::MAX.to_le_bytes().to_vec();
    b.extend([255, 0]);
    word(&mut b, 0x80000000);
    assert_eq!(
        decode(0x93, &b).unwrap(),
        Packet::CommandResult {
            command: u32::MAX,
            name: vec![255],
            result: 0x80000000
        }
    );
    for event in 0..=255 {
        let guid = matches!(event, 3 | 4 | 12 | 13).then_some(u64::MAX);
        let mut b = vec![event, 2, 255, 0, 0];
        if let Some(g) = guid {
            b.extend(g.to_le_bytes());
        }
        assert_eq!(
            decode(0x92, &b).unwrap(),
            Packet::Event {
                event,
                params: vec![vec![255], vec![]],
                guid
            }
        );
    }
}
#[test]
fn strict_framing_counts_strings_and_event_guids() {
    let fixtures = [
        (0x8a, roster()),
        (0x83, b"a\0b\0".to_vec()),
        (0x88, vec![0; 13]),
        (0x93, vec![0; 9]),
        (0x92, vec![3, 0, 1, 2, 3, 4, 5, 6, 7, 8]),
        (0x55, vec![0; 39]),
    ];
    for (op, b) in fixtures {
        assert!(decode(op, &b).is_ok(), "{op:x}");
        for end in 0..b.len() {
            assert!(decode(op, &b[..end]).is_err(), "{op:x}/{end}");
        }
        let mut tail = b;
        tail.push(0);
        assert!(decode(op, &tail).is_err());
    }
    let mut b = roster();
    b[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode(0x8a, &b).is_err());
    let mut b = roster();
    b[14..18].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode(0x8a, &b).is_err());
    assert!(decode(0xffff, &[]).is_err());
}

#[test]
fn emblem_tabard_and_event_log_replies_decode_strictly() {
    assert_eq!(decode(0x1f1, &3u32.to_le_bytes()).unwrap(), Packet::EmblemResult { error: 3 });
    assert_eq!(
        decode(0x1f2, &7u64.to_le_bytes()).unwrap(),
        Packet::TabardVendor { vendor: 7 }
    );
    let mut log = vec![3u8];
    // A leave has no second player; a demote carries the rank; an invite has both players.
    log.push(6);
    log.extend(1u64.to_le_bytes());
    log.extend(10u32.to_le_bytes());
    log.push(4);
    log.extend(2u64.to_le_bytes());
    log.extend(3u64.to_le_bytes());
    log.push(5);
    log.extend(20u32.to_le_bytes());
    log.push(1);
    log.extend(4u64.to_le_bytes());
    log.extend(5u64.to_le_bytes());
    log.extend(30u32.to_le_bytes());
    let Packet::EventLog { entries } = decode(0x3ff, &log).unwrap() else {
        panic!()
    };
    assert_eq!(
        entries,
        [
            EventLogEntry { kind: 6, player: 1, other: None, rank: None, seconds_ago: 10 },
            EventLogEntry { kind: 4, player: 2, other: Some(3), rank: Some(5), seconds_ago: 20 },
            EventLogEntry { kind: 1, player: 4, other: Some(5), rank: None, seconds_ago: 30 },
        ]
    );
    for end in 0..log.len() {
        assert!(decode(0x3ff, &log[..end]).is_err(), "{end}");
    }
    assert!(decode(0x3ff, &[log.clone(), vec![0]].concat()).is_err());
}

#[test]
fn replies_map_onto_the_shared_guild_events() {
    use crate::SessionEvent as E;
    let info = Packet::Info {
        name: b"G".to_vec(),
        // 2009-10-17: years since 2000, 0-based month and day.
        packed_date: (9 << 24) | (9 << 20) | (16 << 14),
        members: 5,
        accounts: 3,
    };
    let [E::GuildInfo(info)] = &session_events(info)[..] else { panic!() };
    assert_eq!((info.created_day, info.created_month, info.created_year), (17, 10, 2009));
    assert!(matches!(
        &session_events(Packet::EmblemResult { error: 0 })[..],
        [E::SaveGuildEmblemResult(0)]
    ));
    assert!(matches!(
        &session_events(Packet::TabardVendor { vendor: 9 })[..],
        [E::TabardVendorActivate(9)]
    ));
    let event = Packet::Event { event: 12, params: vec![b"Bob".to_vec()], guid: Some(5) };
    let [E::GuildEvent(notice)] = &session_events(event)[..] else { panic!() };
    assert_eq!((notice.event, notice.params[0].as_str(), notice.guid), (12, "Bob", Some(5)));
    let Packet::Roster { .. } = decode(0x8a, &roster()).unwrap() else { panic!() };
    let events = session_events(decode(0x8a, &roster()).unwrap());
    let [E::GuildRoster(shared), E::WrathGuild { .. }] = &events[..] else { panic!() };
    assert_eq!(shared.members.len(), 2);
    assert_eq!(shared.members[1].last_online_days, 0.0, "online members carry no time");
    assert!(shared.members[0].last_online_days.is_nan(), "the raw bits survive as the float");
}
