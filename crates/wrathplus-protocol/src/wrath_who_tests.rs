use super::*;

fn strict(opcode: u16, body: &[u8]) {
    for n in 0..body.len() {
        assert!(decode(opcode, &body[..n]).is_err(), "accepted prefix {n}");
    }
    let mut extra = body.to_vec();
    extra.push(0);
    assert!(decode(opcode, &extra).is_err());
}

#[test]
fn native_gender_and_raw_ordered_rows_with_independent_total() {
    let body = [
        2, 0, 0, 0, 0, 0, 0, 0, 255, b'A', 0, 128, 0, 4, 3, 2, 1, 255, 255, 255, 255, 0, 0, 0, 0,
        254, 8, 7, 6, 5, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 0, 3, 0, 0, 0,
    ];
    assert_eq!(
        decode(0x63, &body).unwrap(),
        Packet::Who {
            total: 0,
            entries: vec![
                Entry {
                    name: vec![255, b'A'],
                    guild: vec![128],
                    level: 0x01020304,
                    class: u32::MAX,
                    race: 0,
                    gender: 254,
                    zone: 0x05060708
                },
                Entry {
                    name: vec![],
                    guild: vec![],
                    level: 0,
                    class: 1,
                    race: 2,
                    gender: 0,
                    zone: 3
                },
            ]
        }
    );
    strict(0x63, &body);
}

#[test]
fn empty_counts_bombs_and_no_invented_reply_cap() {
    let body = [0, 0, 0, 0, 255, 255, 255, 255];
    assert_eq!(
        decode(0x63, &body).unwrap(),
        Packet::Who {
            total: u32::MAX,
            entries: vec![]
        }
    );
    strict(0x63, &body);
    assert!(decode(0x63, &[255; 8]).is_err());
    let mut body = vec![0, 1, 0, 0, 1, 0, 0, 0];
    for _ in 0..256 {
        body.extend([0; 19]);
    }
    let Packet::Who { total, entries } = decode(0x63, &body).unwrap() else {
        panic!()
    };
    assert_eq!(total, 1);
    assert_eq!(entries.len(), 256);
    assert!(entries.iter().all(|e| e.name.is_empty() && e.gender == 0));
    assert!(decode(0xffff, &[]).is_err());
}

#[test]
fn whois_raw_and_empty_synthetic_messages_strict_framing() {
    for (body, message) in [
        (vec![255, b'X', 128, 0], vec![255, b'X', 128]),
        (vec![0], vec![]),
    ] {
        assert_eq!(decode(0x65, &body).unwrap(), Packet::Whois { message });
        strict(0x65, &body);
    }
}

fn query() -> Query {
    Query {
        level_min: 0,
        level_max: 100,
        player_name: vec![],
        guild_name: vec![],
        race_mask: u32::MAX,
        class_mask: u32::MAX,
        zones: vec![],
        search_terms: vec![],
    }
}

#[test]
fn full_query_body_and_whois_request_are_raw_not_normalized() {
    let cmd = Command::Who(Query {
        level_min: 0x01020304,
        level_max: u32::MAX,
        player_name: vec![255, b'A'],
        guild_name: vec![128],
        race_mask: 0x80000000,
        class_mask: 0,
        zones: vec![0x05060708, 0],
        search_terms: vec![vec![], vec![254, b'Z']],
    });
    assert_eq!(cmd.opcode(), 0x62);
    assert_eq!(
        cmd.encode().unwrap(),
        [
            4, 3, 2, 1, 255, 255, 255, 255, 255, b'A', 0, 128, 0, 0, 0, 0, 128, 0, 0, 0, 0, 2, 0,
            0, 0, 8, 7, 6, 5, 0, 0, 0, 0, 2, 0, 0, 0, 0, 254, b'Z', 0,
        ]
    );
    assert_eq!(
        Command::Who(query()).encode().unwrap(),
        [
            0, 0, 0, 0, 100, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0,
            0, 0, 0,
        ]
    );
    let cmd = Command::Whois {
        name: vec![255, b'a'],
    };
    assert_eq!(cmd.opcode(), 0x64);
    assert_eq!(cmd.encode().unwrap(), [255, b'a', 0]);
    assert_eq!(Command::Whois { name: vec![] }.encode().unwrap(), [0]);
}

#[test]
fn native_filter_limits_reject_instead_of_truncate_and_all_strings_reject_nul() {
    let mut q = query();
    q.zones = vec![u32::MAX; 10];
    q.search_terms = vec![vec![]; 4];
    let body = Command::Who(q.clone()).encode().unwrap();
    assert_eq!(&body[18..22], &[10, 0, 0, 0]);
    assert_eq!(&body[22..62], &[255; 40]);
    assert_eq!(&body[62..], &[4, 0, 0, 0, 0, 0, 0, 0]);
    q.zones.push(0);
    assert!(Command::Who(q.clone()).encode().is_err());
    q.zones.pop();
    q.search_terms.push(vec![]);
    assert!(Command::Who(q).encode().is_err());
    for field in 0..3 {
        let mut q = query();
        match field {
            0 => q.player_name = vec![0],
            1 => q.guild_name = vec![1, 0, 2],
            _ => q.search_terms = vec![vec![0]],
        };
        assert_eq!(
            Command::Who(q).encode().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert_eq!(
        Command::Whois {
            name: vec![1, 0, 2]
        }
        .encode()
        .unwrap_err()
        .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn whois_debug_redacts_synthetic_request_and_reply_bytes() {
    let packet = Packet::Whois {
        message: b"synthetic-sensitive".to_vec(),
    };
    let command = Command::Whois {
        name: b"synthetic-sensitive".to_vec(),
    };
    assert_eq!(format!("{packet:?}"), "Whois { <redacted> }");
    assert_eq!(format!("{command:?}"), "Whois { <redacted> }");
    let event = crate::SessionEvent::WrathWho { packet };
    assert!(!format!("{event:?}").contains("synthetic-sensitive"));
}
