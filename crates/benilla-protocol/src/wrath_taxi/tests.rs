use super::*;
use crate::wrath_events::{WrathDispatch, WrathEventAdapter};

#[test]
fn wrath_menu_retains_high_nodes_and_rejects_vanilla_length() {
    let guid = 0xf130_0012_3400_5678u64;
    let mut body = 1u32.to_le_bytes().to_vec();
    body.extend(guid.to_le_bytes());
    body.extend(300u32.to_le_bytes());
    let mut mask = TaxiMask::default();
    mask.0[0] = 2;
    mask.0[9] = 1 << 11; // node 300
    mask.0[13] = 1 << 31; // node 448
    for word in mask.0 {
        body.extend(word.to_le_bytes());
    }
    let WrathDispatch::Handled {
        session_events,
        replies,
        ..
    } = WrathEventAdapter::new()
        .dispatch(SMSG_SHOWTAXINODES, &body, 0)
        .unwrap()
    else {
        panic!("menu unhandled")
    };
    assert!(replies.is_empty());
    assert_eq!(session_events.len(), 1);
    let SessionEvent::TaxiNodesShown {
        flightmaster,
        nearest_node,
        known_mask,
    } = &session_events[0]
    else {
        panic!("wrong menu event")
    };
    assert_eq!(
        (*flightmaster, *nearest_node, *known_mask),
        (guid, 300, mask)
    );
    for id in [2, 300, 448] {
        assert!(known_mask.is_known(id));
    }
    for id in [0, 3, 449] {
        assert!(!known_mask.is_known(id));
    }
    assert!(decode(SMSG_SHOWTAXINODES, &body[..48]).is_err());
    rejects_bad_lengths(SMSG_SHOWTAXINODES, &body);
    assert!(decode(SMSG_SHOWTAXINODES, &0u32.to_le_bytes())
        .unwrap()
        .is_empty());
}

#[test]
fn learning_and_activation_verdicts_are_server_owned() {
    let guid = 0xf130_0012_3400_5678u64;
    for known in [false, true] {
        let mut body = guid.to_le_bytes().to_vec();
        body.push(u8::from(known));
        assert!(
            matches!(decode(SMSG_TAXINODE_STATUS, &body).unwrap()[0], SessionEvent::TaxiNodeStatus { guid: g, known: k } if g == guid && k == known)
        );
        rejects_bad_lengths(SMSG_TAXINODE_STATUS, &body);
    }
    for code in 0..=12u32 {
        assert!(
            matches!(decode(SMSG_ACTIVATETAXIREPLY, &code.to_le_bytes()).unwrap()[0], SessionEvent::ActivateTaxiReply { code: c } if c == code)
        );
    }
    rejects_bad_lengths(SMSG_ACTIVATETAXIREPLY, &3u32.to_le_bytes());
    assert!(matches!(
        decode(SMSG_NEW_TAXI_PATH, &[]).unwrap()[0],
        SessionEvent::NewTaxiPath
    ));
    rejects_bad_lengths(SMSG_NEW_TAXI_PATH, &[]);
    assert!(!handles(0xffff));
    assert!(decode(0xffff, &[]).is_err());
}

fn rejects_bad_lengths(opcode: u16, body: &[u8]) {
    for end in 0..body.len() {
        assert!(decode(opcode, &body[..end]).is_err());
    }
    let mut extra = body.to_vec();
    extra.push(0);
    assert!(decode(opcode, &extra).is_err());
}
