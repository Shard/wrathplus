use super::*;

fn packed(guid: u64) -> Vec<u8> {
    let mut b = Vec::new();
    write_packed_guid(guid, &mut b).unwrap();
    b
}

#[test]
fn receipts_decode_strictly_and_dispatch() {
    let c = |packet| SessionEvent::WrathReferFriend { packet };
    let guid = 0x0700_0000_0000_002a;
    let fixtures = vec![
        (
            0x41f,
            packed(guid),
            c(Packet::ProposeLevelGrant { granter: guid }),
        ),
        (
            0x421,
            2u32.to_le_bytes().to_vec(),
            c(Packet::Failure {
                error: 2,
                name: None,
            }),
        ),
        (
            0x421,
            [9u32.to_le_bytes().to_vec(), b"Bob\0".to_vec()].concat(),
            c(Packet::Failure {
                error: 9,
                name: Some("Bob".into()),
            }),
        ),
    ];
    let covered: std::collections::BTreeSet<_> = fixtures.iter().map(|f| f.0).collect();
    assert_eq!(covered, OPCODES.into_iter().collect());
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in fixtures {
        assert_eq!(
            format!("{:?}", decode(opcode, &body).unwrap()),
            format!("{expected:?}"),
            "{opcode:#x}"
        );
        for end in 0..body.len() {
            assert!(
                decode(opcode, &body[..end]).is_err(),
                "{opcode:#x} truncated at {end}"
            );
        }
        assert!(
            decode(opcode, &[body.as_slice(), &[0]].concat()).is_err(),
            "{opcode:#x} trailing"
        );
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(
                    format!("{session_events:?}"),
                    format!("{:?}", vec![expected])
                )
            }
            other => panic!("{opcode:#x}: {other:?}"),
        }
    }
    // Only the not-in-group failure carries a name.
    assert!(decode(
        0x421,
        &[2u32.to_le_bytes().to_vec(), b"Bob\0".to_vec()].concat()
    )
    .is_err());
}

#[test]
fn commands_encode_packed_guids() {
    let guid = 0x0700_0000_0000_002a;
    assert_eq!(
        encode(&Command::GrantLevel { target: guid }),
        (0x40d, packed(guid))
    );
    assert_eq!(
        encode(&Command::AcceptLevelGrant { granter: guid }),
        (0x420, packed(guid))
    );
    assert_eq!(encode(&Command::AcceptLevelGrant { granter: 0 }).1, vec![0]);
}
