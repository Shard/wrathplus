use super::*;

#[test]
fn visible_feeds_the_shared_event_and_forced_reactions_are_captured() {
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    let visible = 42u32.to_le_bytes().to_vec();
    let forced = [2u32, 72, 4, 1037, 7]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();
    for (opcode, body) in [
        (0x123u16, visible.clone()),
        (0x2a5, forced.clone()),
        (0x2a5, vec![0; 4]),
    ] {
        assert!(matches!(
            adapter.dispatch(opcode, &body, 0).unwrap(),
            crate::wrath_events::WrathDispatch::Handled { .. }
        ));
        for end in 0..body.len() {
            assert!(
                decode(opcode, &body[..end]).is_err(),
                "{opcode:#x} truncated at {end}"
            );
        }
        assert!(decode(opcode, &[body.as_slice(), &[0]].concat()).is_err());
    }
    assert!(matches!(
        decode(0x123, &visible).unwrap(),
        SessionEvent::ReputationVisible { list_id: 42 }
    ));
    assert!(matches!(
        decode(0x2a5, &forced).unwrap(),
        SessionEvent::WrathFaction { packet: Packet::ForcedReactions { reactions } } if reactions == vec![(72, 4), (1037, 7)]
    ));
    assert!(decode(0x2a5, &u32::MAX.to_le_bytes()).is_err());
}

#[test]
fn commands_encode_ac_read_order() {
    assert_eq!(encode_at_war(7, true), (0x125, vec![7, 0, 0, 0, 1]));
    assert_eq!(encode_inactive(7, false), (0x317, vec![7, 0, 0, 0, 0]));
    assert_eq!(encode_watched(-1), (0x318, vec![0xff; 4]));
}
