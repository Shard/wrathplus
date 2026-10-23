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
fn request_guid_order_and_exact_empty_bounds() {
    let body = [
        8, 7, 6, 5, 4, 3, 2, 1, 255, 255, 255, 255, 255, 255, 255, 128,
    ];
    assert_eq!(
        decode(0x167, &body).unwrap(),
        Packet::Requested {
            arbiter: 0x0102030405060708,
            challenger: 0x80ffffffffffffff
        }
    );
    strict(0x167, &body);
    assert_eq!(decode(0x168, &[]).unwrap(), Packet::OutOfBounds);
    assert_eq!(decode(0x169, &[]).unwrap(), Packet::InBounds);
    strict(0x168, &[]);
    strict(0x169, &[]);
    assert!(decode(0xffff, &[]).is_err());
}

#[test]
fn raw_completion_and_winner_modes_never_collapse_to_bool() {
    for byte in 0..=255 {
        assert_eq!(
            decode(0x16a, &[byte]).unwrap(),
            Packet::Complete { started: byte }
        );
        strict(0x16a, &[byte]);
        let body = [byte, 255, b'W', 0, 128, b'L', 0];
        assert_eq!(
            decode(0x16b, &body).unwrap(),
            Packet::Winner {
                mode: byte,
                winner: vec![255, b'W'],
                loser: vec![128, b'L']
            }
        );
        strict(0x16b, &body);
    }
    assert_eq!(
        decode(0x16b, &[0, 0, 0]).unwrap(),
        Packet::Winner {
            mode: 0,
            winner: vec![],
            loser: vec![]
        }
    );
    strict(0x16b, &[0, 0, 0]);
}

#[test]
fn countdown_is_raw_u32_including_subsecond_remainder_and_sentinels() {
    for (body, counter) in [
        ([0, 0, 0, 0], 0),
        ([184, 11, 0, 0], 3000),
        ([159, 15, 0, 0], 3999),
        ([255; 4], u32::MAX),
    ] {
        assert_eq!(decode(0x2b7, &body).unwrap(), Packet::Countdown { counter });
        strict(0x2b7, &body);
    }
}

#[test]
fn commands_encode_full_arbiter_not_challenger_guid() {
    for (cmd, opcode, body) in [
        (
            Command::Accepted {
                arbiter: 0x0102030405060708,
            },
            0x16c,
            vec![8, 7, 6, 5, 4, 3, 2, 1],
        ),
        (
            Command::Cancelled {
                arbiter: 0x80ffffffffffffff,
            },
            0x16d,
            vec![255, 255, 255, 255, 255, 255, 255, 128],
        ),
        (Command::Accepted { arbiter: 0 }, 0x16c, vec![0; 8]),
        (
            Command::Cancelled { arbiter: u64::MAX },
            0x16d,
            vec![255; 8],
        ),
    ] {
        assert_eq!(cmd.opcode(), opcode);
        assert_eq!(cmd.encode(), body);
    }
}
