use super::*;

fn q(v: u64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn look() -> Appearance {
    Appearance { gender: 1, skin: 2, hair_color: 3, hair_style: 4, facial_hair: 5, face: 6 }
}

#[test]
fn requests_encode_ac_read_order_and_name_their_reply() {
    let rows = [
        (Command::Rename { guid: 9, name: "Bob".into() }, 0x2c7, [q(9), b"Bob\0".to_vec()].concat(), 0x2c8),
        (
            Command::Customize { guid: 9, name: "Bob".into(), appearance: look() },
            0x473,
            [q(9), b"Bob\0".to_vec(), vec![1, 2, 3, 4, 5, 6]].concat(),
            0x474,
        ),
        (
            Command::ChangeRace { faction: true, guid: 9, name: "Bob".into(), appearance: look(), race: 2 },
            0x4d9,
            [q(9), b"Bob\0".to_vec(), vec![1, 2, 3, 4, 5, 6, 2]].concat(),
            0x4da,
        ),
        (
            Command::ChangeRace { faction: false, guid: 9, name: "Bob".into(), appearance: look(), race: 3 },
            0x4f8,
            [q(9), b"Bob\0".to_vec(), vec![1, 2, 3, 4, 5, 6, 3]].concat(),
            0x4da,
        ),
        (
            Command::DeclinedNames {
                guid: 9,
                name: "Bob".into(),
                declined: ["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
            },
            0x419,
            [q(9), b"Bob\0a\0b\0c\0d\0e\0".to_vec()].concat(),
            0x41a,
        ),
        (Command::RealmSplit { raw: 0xffff_ffff }, 0x38c, vec![0xff; 4], 0x38b),
    ];
    for (command, opcode, body, reply) in rows {
        assert_eq!(encode(&command), (opcode, body, reply), "{command:?}");
    }
}

#[test]
fn replies_decode_success_tails_in_the_reply_order_and_bare_failures() {
    let applied = AppliedAppearance { gender: 1, skin: 2, face: 3, hair_style: 4, hair_color: 5, facial_hair: 6 };
    let rows = [
        (0x2c8, [vec![0], q(9), b"Bob\0".to_vec()].concat(), Reply::Rename { result: 0, renamed: Some((9, "Bob".into())) }),
        (0x2c8, vec![0x31], Reply::Rename { result: 0x31, renamed: None }),
        (
            0x474,
            [vec![0], q(9), b"Bob\0".to_vec(), vec![1, 2, 3, 4, 5, 6]].concat(),
            Reply::Customize { result: 0, applied: Some((9, "Bob".into(), applied.clone())) },
        ),
        (0x474, vec![0x32], Reply::Customize { result: 0x32, applied: None }),
        (
            0x4da,
            [vec![0], q(9), b"Bob\0".to_vec(), vec![1, 2, 3, 4, 5, 6, 10]].concat(),
            Reply::ChangeRace { result: 0, applied: Some((9, "Bob".into(), applied, 10)) },
        ),
        (0x4da, vec![0x32], Reply::ChangeRace { result: 0x32, applied: None }),
        (0x41a, [vec![0; 4], q(9)].concat(), Reply::DeclinedNames { result: 0, guid: 9 }),
        (
            0x38b,
            [vec![0xff; 4], vec![0; 4], b"01/01/01\0".to_vec()].concat(),
            Reply::RealmSplit { raw: 0xffff_ffff, state: 0, date: "01/01/01".into() },
        ),
    ];
    for (opcode, body, expected) in rows {
        assert_eq!(decode(opcode, &body).unwrap(), expected, "{opcode:#x}");
        for end in 0..body.len() {
            assert!(decode(opcode, &body[..end]).is_err(), "{opcode:#x} truncated at {end}");
        }
        assert!(decode(opcode, &[body.as_slice(), &[0]].concat()).is_err(), "{opcode:#x} trailing");
    }
    assert!(decode(0x3a, &[0]).is_err(), "create keeps its own path");
}

#[test]
fn only_applied_renames_customizes_and_race_changes_owe_a_roster() {
    assert!(Reply::Rename { result: 0, renamed: Some((9, "Bob".into())) }.changed_roster());
    assert!(!Reply::Rename { result: 0x31, renamed: None }.changed_roster());
    assert!(!Reply::DeclinedNames { result: 0, guid: 9 }.changed_roster());
    assert!(!Reply::RealmSplit { raw: 0, state: 0, date: String::new() }.changed_roster());
}
