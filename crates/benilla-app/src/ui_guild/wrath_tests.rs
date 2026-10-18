use super::*;
use crate::ui_guild::GuildState;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

fn dispatch(
    app: &mut App,
    adapter: &mut WrathEventAdapter,
    opcode: u16,
    body: &[u8],
) -> Vec<SessionEvent> {
    let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(opcode, body, 0).unwrap()
    else {
        panic!("{opcode:#x} unhandled")
    };
    crate::net::handlers::dispatch(app.world_mut(), session_events.clone());
    session_events
}

fn roster_body() -> Vec<u8> {
    let mut b = 1u32.to_le_bytes().to_vec();
    b.extend(b"motd\0info\0");
    b.extend(1u32.to_le_bytes());
    // Rank 0: flags, money/day, then six (rights, item limit) tabs.
    for word in [0xf_ffffu32, 50_000, 7, 10, 3, 5, 0, 0, 0, 0, 0, 0, 0, 0] {
        b.extend(word.to_le_bytes());
    }
    b.extend(0x99u64.to_le_bytes());
    b.push(1);
    b.extend(b"Bob\0");
    b.extend(0u32.to_le_bytes());
    b.extend([80, 1, 0]);
    b.extend(12u32.to_le_bytes());
    b.extend(b"pub\0off\0");
    b
}

#[test]
fn the_roster_reaches_the_shared_owner_and_keeps_its_bank_words_and_the_log_is_retained() {
    let mut app = App::new();
    app.init_resource::<GuildState>();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    let events = dispatch(&mut app, &mut adapter, 0x8a, &roster_body());
    let [SessionEvent::GuildRoster(roster), SessionEvent::WrathGuild { .. }] = &events[..] else {
        panic!("{events:?}")
    };
    assert_eq!(roster.rank_rights, [0xf_ffff]);
    assert_eq!(roster.members[0].name, "Bob");
    assert_eq!(roster.members[0].public_note, "pub");
    let native = app.world().resource::<NativeGuild>();
    let rank = native.rank(0).expect("rank 0");
    assert_eq!(
        (rank.money_limit, rank.tabs[0], rank.tabs[1]),
        (50_000, (7, 10), (3, 5))
    );
    assert!(native.rank(1).is_none());

    // An invite and a promote, newest first, as AC writes them.
    let mut log = vec![2u8];
    log.push(3);
    log.extend(0x10u64.to_le_bytes());
    log.extend(0x11u64.to_le_bytes());
    log.push(1);
    log.extend(60u32.to_le_bytes());
    log.push(2);
    log.extend(0x12u64.to_le_bytes());
    log.extend(3600u32.to_le_bytes());
    let events = dispatch(&mut app, &mut adapter, 0x3ff, &log);
    assert!(matches!(&events[..], [SessionEvent::WrathGuild { .. }]));
    let names = |guid: u64| (guid != 0x11).then(|| format!("P{guid:x}"));
    let rows = events_for(&app.world().resource::<NativeGuild>().log, names);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (
            rows[0].kind,
            rows[0].player1.as_deref(),
            rows[0].player2.as_deref(),
            rows[0].rank.as_deref()
        ),
        (3, Some("P10"), None, Some("R1"))
    );
    assert_eq!(
        (rows[1].kind, rows[1].player2.clone(), rows[1].seconds_ago),
        (2, None, 3600)
    );

    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    let native = app.world().resource::<NativeGuild>();
    assert!(native.rank(0).is_none() && native.log.is_empty());
}

fn events_for(log: &[EventLogEntry], name: impl Fn(u64) -> Option<String>) -> Vec<WrathGuildEvent> {
    events(log, name, |rank| format!("R{rank}"))
}

#[test]
fn built_client_has_one_native_owner() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world_mut()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathGuild];
    assert_eq!(owners.len(), 1);
    assert!(owners[0].contains("ui_guild::wrath::receive"));
    assert!(census[&SessionEventKind::GuildRoster][0].contains("ui_guild::net::on_roster"));
}
