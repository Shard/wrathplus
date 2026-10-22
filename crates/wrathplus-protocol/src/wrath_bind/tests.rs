use super::*;
use crate::wrath_events::{WrathDispatch, WrathEventAdapter};

#[test]
fn binding_packets_reach_existing_session_consumers() {
    let guid = 0xf130_0012_3400_5678u64;
    let area = 87u32;
    let mut bound = guid.to_le_bytes().to_vec();
    bound.extend(area.to_le_bytes());
    let mut point = Vec::new();
    for pos in [12.5f32, -17.25, 100.0] {
        point.extend(pos.to_le_bytes());
    }
    point.extend(1u32.to_le_bytes());
    point.extend(area.to_le_bytes());
    for (opcode, body, expected) in [
        (
            SMSG_BINDER_CONFIRM,
            guid.to_le_bytes().to_vec(),
            SessionEvent::BinderConfirm { binder: guid },
        ),
        (
            SMSG_BINDPOINTUPDATE,
            point,
            SessionEvent::BindPoint { area },
        ),
        (
            SMSG_PLAYERBOUND,
            bound,
            SessionEvent::PlayerBound { binder: guid, area },
        ),
    ] {
        assert!(handles(opcode));
        assert_binding(decode(opcode, &body).unwrap(), &expected);
        let WrathDispatch::Handled {
            session_events,
            replies,
            ..
        } = WrathEventAdapter::new().dispatch(opcode, &body, 0).unwrap()
        else {
            panic!("unhandled bind")
        };
        assert_binding(session_events, &expected);
        assert!(
            replies.is_empty(),
            "only explicit binder acceptance may send a command"
        );
        for cut in 0..body.len() {
            assert!(decode(opcode, &body[..cut]).is_err());
        }
        let mut extra = body;
        extra.push(0);
        assert!(decode(opcode, &extra).is_err());
    }
    assert_eq!(encode_activate(guid), guid.to_le_bytes());
    assert!(!handles(0xffff));
    assert!(decode(0xffff, &[]).is_err());
}

fn assert_binding(events: Vec<SessionEvent>, expected: &SessionEvent) {
    assert_eq!(events.len(), 1);
    match (&events[0], expected) {
        (SessionEvent::BinderConfirm { binder: a }, SessionEvent::BinderConfirm { binder: b }) => {
            assert_eq!(a, b)
        }
        (SessionEvent::BindPoint { area: a }, SessionEvent::BindPoint { area: b }) => {
            assert_eq!(a, b)
        }
        (
            SessionEvent::PlayerBound { binder: a, area: x },
            SessionEvent::PlayerBound { binder: b, area: y },
        ) => assert_eq!((a, x), (b, y)),
        _ => panic!("wrong binding event"),
    }
}
