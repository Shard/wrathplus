use super::*;
use benilla_protocol::wrath_lfg::proposal::Member;
use benilla_protocol::wrath_lfg::queue::{PartyExtra, PlayerExtra};
use benilla_protocol::wrath_lfg::Packet as P;

const ME: u64 = 7;
const MATE: u64 = 8;

fn name(guid: u64) -> Option<String> {
    match guid {
        ME => Some("Me".into()),
        MATE => Some("Mate".into()),
        _ => None,
    }
}

/// Dungeon 30 is a raid list (type 2); every other entry a dungeon.
fn dungeon(entry: u32) -> Option<(String, u32, u32)> {
    let id = entry & 0xff_ffff;
    Some((format!("D{id}"), id, if id == 30 { 2 } else { 1 }))
}

fn inputs(in_group: bool) -> Inputs<'static> {
    Inputs {
        me: ME,
        in_group,
        now: 100.0,
        name: &name,
        dungeon: &dungeon,
    }
}

fn events(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .map(|e| match e {
            Effect::Line(k, _) | Effect::Error(k) | Effect::Event(k, _) => *k,
        })
        .collect()
}

fn player(update_type: u8, queued: Option<u8>, dungeons: Vec<u32>) -> P {
    P::Queue(Queue::UpdatePlayer {
        update_type,
        extra_flag: u8::from(queued.is_some()),
        extra: queued.map(|queued| PlayerExtra {
            queued,
            reserved: [0; 2],
            dungeons,
            comment: String::new(),
        }),
    })
}

/// `0x365`: the wait begins `queued_time` seconds back; three zero needs read one damager.
#[test]
fn queue_status_stamps_its_start_and_reads_one_damager_for_no_needs() {
    let mut q = LfgQueue::default();
    let fx = q
        .apply(
            &P::Queue(Queue::QueueStatus {
                dungeon: 261,
                average_wait: -1,
                wait: 30,
                tank_wait: 5,
                healer_wait: 6,
                dps_wait: 7,
                tanks: 0,
                healers: 0,
                dps: 0,
                queued_time: 40,
            }),
            &inputs(false),
        )
        .unwrap();
    assert_eq!(events(&fx), ["LFG_QUEUE_STATUS_UPDATE"]);
    let (status, at) = q.queue.clone().unwrap();
    assert_eq!(at, 60.0);
    assert!(matches!(
        status,
        Queue::QueueStatus {
            dps: 1,
            average_wait: -1,
            ..
        }
    ));
}

/// `0x363`: a beginning check announces itself and replays the ready members, then updates and
/// shows the frame while our own row is unready in state 2; state 5 aborts and hides.
#[test]
fn a_role_check_begins_shows_and_aborts() {
    let mut q = LfgQueue::default();
    let member = |guid, ready| RoleMember {
        guid,
        ready,
        roles: 0b1010,
        level: 80,
    };
    let fx = q
        .apply(
            &P::Queue(Queue::RoleCheck {
                state: 2,
                initializing: 1,
                dungeons: vec![261],
                members: vec![member(MATE, 1), member(ME, 0)],
            }),
            &inputs(true),
        )
        .unwrap();
    assert_eq!(
        events(&fx),
        [
            "ERR_LFG_ROLE_CHECK_INITIATED",
            "LFG_ROLE_CHECK_ROLE_CHOSEN",
            "LFG_ROLE_CHECK_UPDATE",
            "LFG_ROLE_CHECK_SHOW"
        ]
    );
    assert_eq!(
        fx[1],
        Effect::Event(
            "LFG_ROLE_CHECK_ROLE_CHOSEN",
            vec![
                V::Str("Mate".into()),
                V::Bool(true),
                V::Bool(false),
                V::Bool(true)
            ]
        )
    );
    let fx = q
        .apply(
            &P::Queue(Queue::RoleCheck {
                state: 5,
                initializing: 0,
                dungeons: vec![],
                members: vec![member(ME, 0)],
            }),
            &inputs(true),
        )
        .unwrap();
    assert_eq!(
        events(&fx),
        [
            "LFG_ROLE_CHECK_UPDATE",
            "ERR_LFG_ROLE_CHECK_ABORTED",
            "LFG_ROLE_CHECK_HIDE"
        ]
    );
    // A declined choice prints the member's line instead of the event.
    let fx = q
        .apply(
            &P::Queue(Queue::RoleChosen {
                guid: MATE,
                ready: 0,
                roles: 0,
            }),
            &inputs(true),
        )
        .unwrap();
    assert_eq!(events(&fx), ["ERR_LFG_PLAYER_DECLINED_ROLE_CHECK"]);
}

/// `0x361`: a new id updates and shows (unless silent), the same id only updates, and a party
/// member's decline drops it with its line.
#[test]
fn proposals_show_once_and_fail_on_a_decline() {
    let mut q = LfgQueue::default();
    let row = |is_self, answered, accepted| Member {
        roles: 2,
        is_self,
        in_dungeon: 0,
        same_group: 1,
        answered,
        accepted,
    };
    let proposal = |members| {
        P::Proposal(Proposal {
            entry: 261,
            state: 0,
            id: 9,
            encounter_mask: 0,
            silent: 0,
            members,
        })
    };
    let fx = q
        .apply(&proposal(vec![row(1, 0, 0)]), &inputs(false))
        .unwrap();
    assert_eq!(events(&fx), ["LFG_PROPOSAL_UPDATE", "LFG_PROPOSAL_SHOW"]);
    let fx = q
        .apply(&proposal(vec![row(1, 1, 1)]), &inputs(false))
        .unwrap();
    assert_eq!(events(&fx), ["LFG_PROPOSAL_UPDATE"]);
    let fx = q
        .apply(&proposal(vec![row(1, 1, 1), row(0, 1, 0)]), &inputs(false))
        .unwrap();
    assert_eq!(
        events(&fx),
        [
            "LFG_PROPOSAL_UPDATE",
            "ERR_LFG_PROPOSAL_DECLINED_PARTY",
            "LFG_PROPOSAL_FAILED"
        ]
    );
    assert!(q.proposal.is_none());
}

/// `0x367` solo: joining prints the queue or list line and `LFG_UPDATE`; a repeat changes
/// nothing; leaving on update type 7 prints the left line and clears the wait; a proposal up
/// when the queue is left with type 10 succeeds.
#[test]
fn player_updates_join_repeat_and_leave() {
    let mut q = LfgQueue::default();
    let fx = q
        .apply(&player(5, Some(1), vec![261]), &inputs(false))
        .unwrap();
    assert_eq!(events(&fx), ["ERR_LFG_JOINED_QUEUE", "LFG_UPDATE"]);
    assert!(q
        .apply(&player(5, Some(1), vec![261]), &inputs(false))
        .unwrap()
        .is_empty());
    q.proposal = Some(Proposal {
        entry: 261,
        state: 0,
        id: 9,
        encounter_mask: 0,
        silent: 0,
        members: vec![],
    });
    q.queue = Some((
        Queue::JoinResult {
            result: 0,
            state: 0,
            locks: None,
        },
        0.0,
    ));
    let fx = q.apply(&player(10, None, vec![]), &inputs(false)).unwrap();
    assert_eq!(events(&fx), ["LFG_PROPOSAL_SUCCEEDED", "LFG_UPDATE"]);
    assert!(q.queue.is_none() && q.proposal.is_none());
    let mut list = LfgQueue::default();
    let fx = list
        .apply(&player(5, Some(0), vec![30]), &inputs(false))
        .unwrap();
    assert_eq!(events(&fx), ["ERR_LFG_JOINED_LIST", "LFG_UPDATE"]);
    let fx = list
        .apply(&player(7, None, vec![]), &inputs(false))
        .unwrap();
    assert_eq!(events(&fx), ["ERR_LFG_LEFT_LIST", "LFG_UPDATE"]);
    // In a group the player's update stores without its event.
    let mut grouped = LfgQueue::default();
    let fx = grouped
        .apply(&player(5, Some(1), vec![261]), &inputs(true))
        .unwrap();
    assert_eq!(events(&fx), ["ERR_LFG_JOINED_QUEUE"]);
}

/// `0x368` in a group: the join byte, not the extra block, gates the join line, which also needs
/// a dungeon.
#[test]
fn party_updates_read_the_join_byte() {
    let party = |join, dungeons: Vec<u32>| {
        P::Queue(Queue::UpdateParty {
            update_type: 5,
            extra_flag: 1,
            extra: Some(PartyExtra {
                join,
                queued: 1,
                reserved: [0; 5],
                dungeons,
                comment: String::new(),
            }),
        })
    };
    let mut q = LfgQueue::default();
    let fx = q.apply(&party(0, vec![261]), &inputs(true)).unwrap();
    assert_eq!(events(&fx), ["LFG_UPDATE"]);
    let fx = q.apply(&party(1, vec![261]), &inputs(true)).unwrap();
    assert_eq!(events(&fx), ["ERR_LFG_JOINED_QUEUE", "LFG_UPDATE"]);
    // Solo, the party's update stores without its event.
    let mut solo = LfgQueue::default();
    assert!(solo
        .apply(&party(1, vec![]), &inputs(false))
        .unwrap()
        .is_empty());
}

/// The other receipts: join results by the native table, a finished boot vote, the completion
/// lines and the offer, and `LFG_DISABLED` clearing everything.
#[test]
fn results_boots_rewards_offers_and_disabled() {
    let mut q = LfgQueue::default();
    let join = |result, state| {
        P::Queue(Queue::JoinResult {
            result,
            state,
            locks: None,
        })
    };
    assert_eq!(
        events(&q.apply(&join(1, 3), &inputs(false)).unwrap()),
        ["ERR_LFG_ROLE_CHECK_FAILED_TIMEOUT"]
    );
    assert_eq!(
        events(&q.apply(&join(12, 0), &inputs(false)).unwrap()),
        ["ERR_LFG_DESERTER_PLAYER"]
    );
    assert!(q.apply(&join(0, 0), &inputs(false)).unwrap().is_empty());
    let boot = P::Boot(Boot {
        in_progress: 0,
        did_vote: 1,
        agree: 1,
        victim: MATE,
        votes: 4,
        agrees: 2,
        seconds_left: 0,
        votes_needed: 3,
        reason: String::new(),
    });
    assert_eq!(
        events(&q.apply(&boot, &inputs(true)).unwrap()),
        ["ERR_PARTY_LFG_BOOT_VOTE_FAILED", "LFG_BOOT_PROPOSAL_UPDATE"]
    );
    let offer = P::Dungeon(wrath_lfg::dungeon::Packet::OfferContinue {
        entry: 1 << 24 | 261,
    });
    assert_eq!(
        q.apply(&offer, &inputs(true)).unwrap(),
        [Effect::Event(
            "LFG_OFFER_CONTINUE",
            vec![V::Str("D261".into()), V::Int(261), V::Int(1)]
        )]
    );
    q.player.joined = true;
    let fx = q
        .apply(
            &P::Dungeon(wrath_lfg::dungeon::Packet::Disabled),
            &inputs(false),
        )
        .unwrap();
    assert_eq!(events(&fx), ["LFG_UPDATE", "ERR_SYSTEM_DISABLED"]);
    assert!(q.disabled && !q.player.joined && q.boot.is_none());
}
