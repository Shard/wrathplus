use super::*;

// Independent fixtures follow AC BuildChatPacket's branch layout; no production encoder used.
fn sized(b: &mut Vec<u8>, s: &[u8]) {
    b.extend(((s.len() + 1) as u32).to_le_bytes());
    b.extend(s);
    b.push(0);
}
fn fixture(kind: u8, gm: bool, receiver: u64) -> Vec<u8> {
    let mut b = vec![kind];
    b.extend(0xffff_ffffu32.to_le_bytes());
    b.extend(0xabcdefu64.to_le_bytes());
    b.extend(0x12345678u32.to_le_bytes());
    match kind {
        0x0c..=0x10 | 0x29 | 0x2a | 0x2f | 0x08 => sized(&mut b, b"\xffsender"),
        0x24..=0x26 | 0x30 | 0x31 => (),
        _ => {
            if gm {
                sized(&mut b, b"");
            }
            if kind == 0x11 {
                b.extend(b"General\0");
            }
        }
    }
    b.extend(receiver.to_le_bytes());
    match (kind, receiver >> 48, receiver != 0) {
        (0x0c..=0x10 | 0x29 | 0x2a | 0x2f, h, true) if h != 0 && h != 0xf140 => {
            sized(&mut b, b"target")
        }
        (0x24..=0x26, h, true) if h != 0 => sized(&mut b, b"target"),
        _ => (),
    }
    sized(&mut b, b"\xff\x01addon\tdata");
    b.push(0x83);
    if matches!(kind, 0x30 | 0x31) {
        b.extend(1234u32.to_le_bytes());
    }
    b
}
#[test]
fn all_types_and_gm_branches_preserve_raw_fields_and_guid_categories() {
    for kind in 0..=u8::MAX {
        for gm in [false, true] {
            for receiver in [
                0,
                42,
                0x0000_1234_0000_0042,
                0xf140_0000_0000_0042,
                0xf130_0000_0000_0042,
            ] {
                let opcode = if gm {
                    SMSG_GM_MESSAGECHAT
                } else {
                    SMSG_MESSAGECHAT
                };
                let m = decode(opcode, &fixture(kind, gm, receiver)).unwrap();
                assert_eq!(
                    (m.gm, m.chat_type, m.language, m.flags, m.receiver_guid),
                    (gm, kind, u32::MAX, 0x12345678, receiver)
                );
                assert_eq!(m.sender_guid, 0xabcdef);
                assert_eq!(m.text, b"\xff\x01addon\tdata");
                assert_eq!(m.chat_tag, 0x83);
                let named = matches!(kind, 0x0c..=0x10 | 0x29 | 0x2a | 0x2f | 0x08);
                let special = named || matches!(kind, 0x24..=0x26 | 0x30 | 0x31);
                assert_eq!(
                    m.sender_name,
                    if named {
                        Some(b"\xffsender".to_vec())
                    } else if gm && !special {
                        Some(vec![])
                    } else {
                        None
                    }
                );
                assert_eq!(
                    m.channel,
                    if kind == 0x11 {
                        Some(b"General".to_vec())
                    } else {
                        None
                    }
                );
                assert_eq!(
                    m.achievement_id,
                    if matches!(kind, 0x30 | 0x31) {
                        Some(1234)
                    } else {
                        None
                    }
                );
                let high = receiver >> 48;
                let has_name = receiver != 0
                    && ((matches!(kind, 0x0c..=0x10 | 0x29 | 0x2a | 0x2f)
                        && high != 0
                        && high != 0xf140)
                        || (matches!(kind, 0x24..=0x26) && high != 0));
                assert_eq!(
                    m.receiver_name,
                    if has_name {
                        Some(b"target".to_vec())
                    } else {
                        None
                    }
                );
            }
        }
    }
}
#[test]
fn strict_framing_every_truncation_and_trailing_byte() {
    for (kind, gm) in [
        (1, false),
        (0x11, true),
        (0x0c, false),
        (0x24, false),
        (0x30, true),
        (8, true),
    ] {
        let op = if gm {
            SMSG_GM_MESSAGECHAT
        } else {
            SMSG_MESSAGECHAT
        };
        let mut b = fixture(kind, gm, 0xf130_0000_0000_0001);
        for n in 0..b.len() {
            assert!(decode(op, &b[..n]).is_err(), "{kind:x} truncation{n}");
        }
        b.push(0);
        assert!(decode(op, &b).is_err());
    }
    assert!(decode(0x95, &[]).is_err());
}
#[test]
fn invalid_lengths_terminators_and_present_empty() {
    let base = fixture(1, false, 0);
    for size in [0, u32::MAX, 1] {
        let mut b = base.clone();
        b[25..29].copy_from_slice(&size.to_le_bytes());
        assert!(decode(SMSG_MESSAGECHAT, &b).is_err());
    }
    let mut b = base.clone();
    b[29] = 0;
    assert!(decode(SMSG_MESSAGECHAT, &b).is_err());
    let mut b = base.clone();
    let last = b.len() - 2;
    b[last] = 1;
    assert!(decode(SMSG_MESSAGECHAT, &b).is_err());
    let mut empty = base[..25].to_vec();
    sized(&mut empty, b"");
    empty.push(0);
    assert_eq!(
        decode(SMSG_MESSAGECHAT, &empty).unwrap().text,
        Vec::<u8>::new()
    );
}
#[test]
fn outgoing_all_types_exact_native_shape_and_rejections() {
    for kind in 0..=255 {
        let target = if matches!(kind, 7 | 0x11) {
            Some(b"recipient".to_vec())
        } else {
            None
        };
        let c = Command {
            chat_type: kind,
            language: u32::MAX,
            target,
            text: b"\xff\taddon".to_vec(),
        };
        let allowed = matches!(
            kind,
            1..=7 | 0x0a | 0x11 | 0x17 | 0x18 | 0x27 | 0x28 | 0x2c | 0x2d | 0x33
        );
        let result = encode(&c);
        assert_eq!(result.is_ok(), allowed, "{kind:x}");
        if let Ok((opcode, body)) = result {
            let mut expected = kind.to_le_bytes().to_vec();
            expected.extend([255; 4]);
            if matches!(kind, 7 | 0x11) {
                expected.extend(b"recipient\0");
            }
            expected.extend(b"\xff\taddon\0");
            assert_eq!((opcode, body), (0x95, expected));
        }
    }
    for (kind, target, text) in [
        (1, Some(vec![]), vec![]),
        (7, None, vec![]),
        (0x11, Some(vec![0]), vec![]),
        (1, None, vec![0]),
    ] {
        assert!(encode(&Command {
            chat_type: kind,
            language: 0,
            target,
            text
        })
        .is_err());
    }
}
