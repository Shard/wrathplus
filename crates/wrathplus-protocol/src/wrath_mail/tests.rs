use super::*;
use crate::wrath_events::{WrathDispatch, WrathEventAdapter};

fn preview() -> Vec<u8> {
    [
        42u64.to_le_bytes().as_slice(),
        0u32.to_le_bytes().as_slice(),
        0u32.to_le_bytes().as_slice(),
        41u32.to_le_bytes().as_slice(),
        (-1.0f32).to_le_bytes().as_slice(),
    ]
    .concat()
}
fn query(seconds: f32, count: u32) -> Vec<u8> {
    let mut body = [seconds.to_le_bytes(), count.to_le_bytes()].concat();
    for _ in 0..count {
        body.extend(preview());
    }
    body
}

#[test]
fn next_mail_parses_empty_and_two_sender_native_tails() {
    for (seconds, count) in [(-86400.0, 0), (0.0, 0), (0.0, 1), (0.0, 2)] {
        let body = query(seconds, count);
        assert!(
            matches!(&decode(MSG_QUERY_NEXT_MAIL_TIME, &body).unwrap()[0],
            SessionEvent::NextMailTime { seconds: actual, previews } if *actual == seconds && previews.len() == count as usize)
        );
        for end in 0..body.len() {
            assert!(decode(MSG_QUERY_NEXT_MAIL_TIME, &body[..end]).is_err());
        }
    }
}

#[test]
fn notifications_and_native_next_mail_are_dispatched() {
    let mut adapter = WrathEventAdapter::default();
    for (opcode, body) in [
        (SMSG_RECEIVED_MAIL, 0f32.to_le_bytes().to_vec()),
        (MSG_QUERY_NEXT_MAIL_TIME, query(-86400.0, 0)),
    ] {
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(session_events.len(), 1);
                match &session_events[0] {
                    SessionEvent::ReceivedMail { seconds } => assert_eq!(*seconds, 0.0),
                    SessionEvent::NextMailTime { seconds, .. } => assert_eq!(*seconds, -86400.0),
                    other => panic!("unexpected event {other:?}"),
                }
            }
            other => panic!("unexpected dispatch {other:?}"),
        }
    }
    assert!(encode_query_next_mail_time().is_empty());
}

#[test]
fn rejects_trailing_truncated_oversized_and_nonfinite_packets() {
    assert!(decode(MSG_QUERY_NEXT_MAIL_TIME, &query(0.0, 3)).is_err());
    assert!(decode(MSG_QUERY_NEXT_MAIL_TIME, &query(f32::NAN, 0)).is_err());
    let mut invalid_preview = query(0.0, 1);
    invalid_preview[28..32].copy_from_slice(&f32::INFINITY.to_le_bytes());
    assert!(decode(MSG_QUERY_NEXT_MAIL_TIME, &invalid_preview).is_err());
    for opcode in [SMSG_RECEIVED_MAIL, MSG_QUERY_NEXT_MAIL_TIME] {
        let body = if opcode == SMSG_RECEIVED_MAIL {
            0f32.to_le_bytes().to_vec()
        } else {
            query(0.0, 0)
        };
        let mut trailing = body.clone();
        trailing.push(0);
        assert!(decode(opcode, &trailing).is_err());
        assert!(decode(opcode, &body[..body.len() - 1]).is_err());
    }
    assert!(decode(SMSG_RECEIVED_MAIL, &f32::NAN.to_le_bytes()).is_err());
}

#[test]
fn next_mail_keeps_sender_previews_in_wire_order() {
    let mut body = query(0.0, 0);
    body[4..8].copy_from_slice(&2u32.to_le_bytes());
    body.extend(preview());
    body.extend(
        [
            0u64.to_le_bytes().as_slice(),
            &7u32.to_le_bytes(),
            &3u32.to_le_bytes(),
            &62u32.to_le_bytes(),
            &5.0f32.to_le_bytes(),
        ]
        .concat(),
    );
    match &decode(MSG_QUERY_NEXT_MAIL_TIME, &body).unwrap()[0] {
        SessionEvent::NextMailTime { previews, .. } => assert_eq!(
            previews,
            &vec![
                MailPreview {
                    sender: 42,
                    entry: 0,
                    kind: 0,
                    stationery: 41,
                    delay: -1.0
                },
                MailPreview {
                    sender: 0,
                    entry: 7,
                    kind: 3,
                    stationery: 62,
                    delay: 5.0
                },
            ]
        ),
        other => panic!("{other:?}"),
    }
}
