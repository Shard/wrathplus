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
fn contacts_preserve_combined_unknown_flags_raw_notes_and_conditional_presence() {
    let body = [
        255, 255, 255, 255, 3, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 128, 7, 0, 0, 128, 255, 0, 128, 4, 3,
        2, 1, 0, 0, 0, 0, 255, 255, 255, 255, 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 3, 0, 0, 0,
        0, 0, 0, 0, 6, 0, 0, 0, b'N', 0,
    ];
    assert_eq!(
        decode(0x67, &body).unwrap(),
        Packet::ContactList {
            flags: u32::MAX,
            contacts: vec![
                Contact {
                    guid: 0x8000000000000001,
                    flags: 0x80000007,
                    note: vec![255],
                    friend: Some(Friend {
                        status: 128,
                        presence: Some(Presence {
                            area: 0x01020304,
                            level: 0,
                            class: u32::MAX
                        })
                    })
                },
                Contact {
                    guid: 2,
                    flags: 1,
                    note: vec![],
                    friend: Some(Friend {
                        status: 0,
                        presence: None
                    })
                },
                Contact {
                    guid: 3,
                    flags: 6,
                    note: vec![b'N'],
                    friend: None
                },
            ]
        }
    );
    strict(0x67, &body);
}

#[test]
fn list_empty_count_limits_and_duplicates_are_wire_not_client_policy() {
    let empty = [4, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(
        decode(0x67, &empty).unwrap(),
        Packet::ContactList {
            flags: 4,
            contacts: vec![]
        }
    );
    strict(0x67, &empty);
    assert!(decode(0x67, &[0, 0, 0, 0, 255, 255, 255, 255]).is_err());
    let mut body = vec![0, 0, 0, 128, 0, 1, 0, 0]; // 256 rows, no u8 count/cap.
    for _ in 0..256 {
        body.extend([0; 13]);
    }
    let Packet::ContactList { flags, contacts } = decode(0x67, &body).unwrap() else {
        panic!()
    };
    assert_eq!(flags, 0x80000000);
    assert_eq!(contacts.len(), 256);
    assert!(contacts.iter().all(|c| c.guid == 0 && c.friend.is_none()));
    assert!(decode(0xffff, &[]).is_err());
}

#[test]
fn friend_status_notes_and_online_tail_use_result_not_status_predicates() {
    for result in 0..=255 {
        let mut body = vec![result, 8, 7, 6, 5, 4, 3, 2, 1];
        if matches!(result, 6 | 7) {
            body.extend([255, 0]);
        }
        if matches!(result, 2 | 6) {
            body.extend([0, 4, 3, 2, 1, 0, 0, 0, 0, 255, 255, 255, 255]);
        }
        assert_eq!(
            decode(0x68, &body).unwrap(),
            Packet::FriendStatus {
                result,
                guid: 0x0102030405060708,
                note: if matches!(result, 6 | 7) {
                    Some(vec![255])
                } else {
                    None
                },
                online: if matches!(result, 2 | 6) {
                    Some(Friend {
                        status: 0,
                        presence: Some(Presence {
                            area: 0x01020304,
                            level: 0,
                            class: u32::MAX,
                        }),
                    })
                } else {
                    None
                },
            }
        );
        strict(0x68, &body);
    }
    assert_eq!(
        decode(0x68, &[7, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        Packet::FriendStatus {
            result: 7,
            guid: 0,
            note: Some(vec![]),
            online: None
        }
    );
}

#[test]
fn request_opcodes_and_full_body_fixtures() {
    let fixtures = [
        (
            Command::ContactList { flags: 0x80000007 },
            0x66,
            vec![7, 0, 0, 128],
        ),
        (
            Command::AddFriend {
                name: vec![255, b'A'],
                note: vec![128],
            },
            0x69,
            vec![255, b'A', 0, 128, 0],
        ),
        (
            Command::DeleteFriend {
                guid: 0x0102030405060708,
            },
            0x6a,
            vec![8, 7, 6, 5, 4, 3, 2, 1],
        ),
        (
            Command::SetContactNotes {
                guid: 0x0102030405060708,
                note: vec![255],
            },
            0x6b,
            vec![8, 7, 6, 5, 4, 3, 2, 1, 255, 0],
        ),
        (
            Command::AddIgnore {
                name: vec![128, b'B'],
            },
            0x6c,
            vec![128, b'B', 0],
        ),
        (Command::DeleteIgnore { guid: u64::MAX }, 0x6d, vec![255; 8]),
        (
            Command::AddFriend {
                name: vec![],
                note: vec![],
            },
            0x69,
            vec![0, 0],
        ),
        (
            Command::SetContactNotes {
                guid: 0,
                note: vec![],
            },
            0x6b,
            vec![0; 9],
        ),
        (Command::AddIgnore { name: vec![] }, 0x6c, vec![0]),
    ];
    for (cmd, opcode, bytes) in fixtures {
        assert_eq!(cmd.opcode(), opcode);
        assert_eq!(cmd.encode().unwrap(), bytes);
    }
}

#[test]
fn every_request_string_rejects_embedded_nul_without_lossy_conversion() {
    for cmd in [
        Command::AddFriend {
            name: vec![0],
            note: vec![],
        },
        Command::AddFriend {
            name: vec![],
            note: vec![0],
        },
        Command::SetContactNotes {
            guid: 1,
            note: vec![1, 0, 2],
        },
        Command::AddIgnore {
            name: vec![1, 0, 2],
        },
    ] {
        assert_eq!(
            cmd.encode().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
