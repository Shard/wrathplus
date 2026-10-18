use super::*;
use std::collections::VecDeque;

/// The newest `0x147` warning sets the lock on the `GetTime` clock and leaves the other receipts.
#[test]
fn the_lock_warning_is_taken_and_the_rest_stay() {
    let mut fifo = VecDeque::from([
        Packet::Reset { map: 33 },
        Packet::LockWarning {
            timeout_ms: 60_000,
            encounter_mask: 0b1011,
            raw_flag: 1,
        },
        Packet::LastInstance { map: 34 },
    ]);
    let lock = take_lock_warning(&mut fifo, 100.0).expect("a warning");
    assert_eq!(fifo.len(), 2);
    let view = lock.view();
    assert_eq!(
        (view.expires_at, view.extending, view.encounters_completed),
        (160.0, true, 3)
    );
    assert_eq!(take_lock_warning(&mut fifo, 100.0), None);
}

/// The active group's set glyph slots, 1-based.
#[test]
fn filled_sockets_are_the_active_groups_glyphs() {
    let mut progress = crate::net::wrath_notices::WrathProgress::default();
    let group = |glyphs: Vec<u16>| benilla_protocol::events::TalentGroup {
        talents: Vec::new(),
        glyphs,
    };
    progress.talents.insert(
        false,
        (
            0,
            1,
            vec![group(vec![5, 0, 0]), group(vec![0, 7, 0, 9, 0, 0])],
        ),
    );
    assert_eq!(filled_glyph_sockets(Some(&progress)), [2, 4]);
    assert!(filled_glyph_sockets(None).is_empty());
}

/// Each answer's send; `CancelSummon` needs a latched summoner and a refund a resolved item.
#[test]
fn answers_map_to_their_sends() {
    let team = |t: u8| u32::from(t) * 10;
    let none = |_: i64, _: u32, _: bool| None;
    let item = |bag: i64, slot: u32, equipped: bool| {
        Some(bag as u64 * 100 + u64::from(slot) + u64::from(equipped))
    };
    assert!(matches!(
        answer(&WrathPopupRequest::CancelSummon, &team, Some(7), &none),
        Some(ClientCommand::NativeDeath(
            benilla_protocol::wrath_death::Command::SummonResponse {
                summoner: 7,
                agree: false
            }
        ))
    ));
    assert!(answer(&WrathPopupRequest::CancelSummon, &team, None, &none).is_none());
    assert!(matches!(
        answer(&WrathPopupRequest::DeclineInvite, &team, None, &none),
        Some(ClientCommand::GroupDecline)
    ));
    assert!(matches!(
        answer(
            &WrathPopupRequest::ArenaTeamDisband { team: 2 },
            &team,
            None,
            &none
        ),
        Some(ClientCommand::ArenaTeamCommand(
            benilla_protocol::wrath_arena_team::Command::Disband { team_id: 20 }
        ))
    ));
    assert!(matches!(
        answer(
            &WrathPopupRequest::RespondInstanceLock { accept: true },
            &team,
            None,
            &none
        ),
        Some(ClientCommand::NativeInstanceControl(
            benilla_protocol::wrath_instance_control::Command::LockResponse { raw_accept: 1 }
        ))
    ));
    assert!(matches!(
        answer(
            &WrathPopupRequest::RefundItemPurchase {
                bag: 1,
                slot: 4,
                equipped: false
            },
            &team,
            None,
            &item
        ),
        Some(ClientCommand::NativeInventory(
            benilla_protocol::wrath_inventory::Command::Refund { item_guid: 104 }
        ))
    ));
    assert!(answer(
        &WrathPopupRequest::RefundItemPurchase {
            bag: 1,
            slot: 4,
            equipped: false
        },
        &team,
        None,
        &none
    )
    .is_none());
    assert!(answer(&WrathPopupRequest::GmResponseResolve, &team, None, &none).is_none());
}
