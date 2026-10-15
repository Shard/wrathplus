use super::*;
use crate::{
    wrath_events::{WrathDispatch, WrathEventAdapter},
    SessionEvent,
};

// Hand-authored native row: full GUID, unknown difficulty and nonboolean raw flags.
const ROW: &[u8] = &[
    0x3b, 0x02, 0, 0, 0xef, 0xbe, 0xad, 0xde, 8, 7, 6, 5, 4, 3, 2, 1, 0xfe, 0x80, 0xff, 0xff, 0xff,
    0xff,
];
fn body() -> Vec<u8> {
    let mut b = vec![2, 0, 0, 0];
    b.extend(ROW);
    b.extend(ROW); // Duplicates are not deduplicated.
    b
}
#[test]
fn lossless_rows_and_empty_snapshots_dispatch_once_without_legacy_projection() {
    let b = body();
    let expected = SavedInstance {
        map: 571,
        difficulty: 0xdeadbeef,
        instance_guid: 0x0102030405060708,
        raw_flag: 0xfe,
        extended: 0x80,
        reset_seconds: u32::MAX,
    };
    let info = decode(&b).unwrap();
    assert_eq!(info.entries, vec![expected.clone(), expected]);
    let mut adapter = WrathEventAdapter::new();
    for bytes in [b.as_slice(), &[0, 0, 0, 0]] {
        let expected = decode(bytes).unwrap();
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(SMSG_RAID_INSTANCE_INFO, bytes, 0).unwrap()
        else {
            panic!("unhandled")
        };
        assert_eq!(session_events.len(), 1);
        assert!(
            matches!(&session_events[0], SessionEvent::WrathRaidInstanceInfo { info } if info == &expected)
        );
    }
}
#[test]
fn strict_prefix_count_and_tail_validation_never_emits_a_partial_snapshot() {
    let b = body();
    let mut malformed: Vec<Vec<u8>> = (0..b.len()).map(|n| b[..n].to_vec()).collect();
    let mut tail = b.clone();
    tail.push(0);
    malformed.push(tail);
    for count in [0u32, 1, 3, u32::MAX] {
        let mut wrong = b.clone();
        wrong[..4].copy_from_slice(&count.to_le_bytes());
        malformed.push(wrong);
    }
    let mut adapter = WrathEventAdapter::new();
    for bytes in malformed {
        assert!(decode(&bytes).is_err());
        assert!(matches!(
            adapter
                .dispatch(SMSG_RAID_INSTANCE_INFO, &bytes, 0)
                .unwrap(),
            WrathDispatch::Unhandled {
                opcode: SMSG_RAID_INSTANCE_INFO
            }
        ));
    }
}
#[test]
fn row_order_and_zero_words_are_not_interpreted() {
    let mut b = body();
    b[26..].fill(0);
    let rows = decode(&b).unwrap().entries;
    assert_eq!(rows[0].map, 571);
    assert_eq!(
        rows[1],
        SavedInstance {
            map: 0,
            difficulty: 0,
            instance_guid: 0,
            raw_flag: 0,
            extended: 0,
            reset_seconds: 0
        }
    );
}
