use super::*;
use benilla_protocol::wrath_calendar::{Holiday, InviteRow};

const ME: u64 = 7;

fn net() -> NetCommands {
    let (tx, _rx) = crossbeam_channel::unbounded();
    NetCommands(tx)
}

fn packed(y: i32, m: u8, d: u8, h: u8, min: u8) -> u32 {
    WrathCalendarDate::new(y, m, d, h, min).packed()
}

fn events(fired: &[(&'static str, Vec<V>)]) -> Vec<&'static str> {
    fired.iter().map(|(e, _)| *e).collect()
}

/// 15 March 2010 12:00 UTC, the server two hours ahead.
const NOW: u32 = 1_268_654_400;

fn calendar_packet() -> Packet {
    let mut holiday = Holiday {
        id: 181,
        region: 0,
        looping: 0,
        priority: 0,
        filter_type: 0,
        dates: [0; 26],
        durations: [0; 10],
        flags: [0; 10],
        texture: "Calendar_Noblegarden".into(),
    };
    holiday.dates[0] = packed(2010, 3, 14, 10, 0);
    holiday.durations[0] = 48;
    Packet::Calendar(Calendar {
        invites: vec![InviteRow {
            event_id: 100,
            invite_id: 200,
            status: 0,
            rank: 0,
            guild_event: 0,
            creator: 9,
        }],
        events: vec![EventRow {
            event_id: 100,
            title: "Naxx".into(),
            kind: 0,
            packed_time: packed(2010, 3, 16, 19, 0),
            flags: 0,
            dungeon: -1,
            creator: 9,
        }],
        server_time: NOW,
        zone_packed_time: packed(2010, 3, 15, 14, 0),
        lockouts: vec![Lockout {
            map: 533,
            difficulty: 1,
            seconds_left: 86_400,
            instance: 55,
        }],
        relation_time: 1_135_814_400,
        raid_resets: vec![RaidReset {
            map: 533,
            period: 604_800,
            offset: 0,
        }],
        holidays: vec![holiday],
    })
}

/// The calendar lists the invite, the lockout in the server's zone and each holiday day.
#[test]
fn the_calendar_lists_events_lockouts_and_holiday_days() {
    let mut state = CalendarState::default();
    let mut fired = Vec::new();
    reduce(&mut state, calendar_packet(), ME, &mut fired);
    assert_eq!(events(&fired), ["CALENDAR_UPDATE_EVENT_LIST"]);
    assert_eq!(state.zone, 7200);
    let view = build(&state, &CalendarCatalog::default(), None, &net(), None, ME);
    let kinds: Vec<_> = view
        .entries
        .iter()
        .map(|e| (e.kind, e.date.day, e.date.hour, e.sequence.2))
        .collect();
    assert_eq!(
        kinds,
        [
            (WrathCalendarKind::Holiday, 14, 10, "START"),
            (WrathCalendarKind::Holiday, 15, 0, "ONGOING"),
            (WrathCalendarKind::Holiday, 16, 9, "END"),
            (WrathCalendarKind::RaidLockout, 16, 14, ""),
            (WrathCalendarKind::Player, 16, 19, ""),
        ]
    );
    let event = &view.entries[4];
    assert_eq!(
        (event.event_id, event.invite_id, event.invite),
        (100, 200, Some((0, 0)))
    );
    assert_eq!(view.today.map(|t| (t.day, t.hour)), Some((15, 14)));
    // The reset projects from the relation time in the server's zone.
    assert_eq!(view.raid_resets[0].first, 1_135_814_400 + 7200);
}

/// The asked-for event opens with its type; a resend refreshes it.
#[test]
fn an_asked_event_opens_then_refreshes() {
    let mut state = CalendarState::default();
    let sent = apply(&mut state, R::GetEvent { event_id: 100 }, ME, None);
    assert_eq!(sent, [Command::GetEvent { event_id: 100 }]);
    let event = Event {
        send_type: 0,
        creator: 9,
        event_id: 100,
        title: "Naxx".into(),
        description: String::new(),
        kind: 0,
        repeat: 0,
        max_invites: 25,
        dungeon: -1,
        flags: 0x400,
        packed_time: packed(2010, 3, 16, 19, 0),
        timezone_time: 0,
        guild_id: 1,
        invitees: vec![EventInvitee {
            guid: ME,
            level: 80,
            status: 6,
            rank: 0,
            guild_member: 1,
            invite_id: 201,
            status_time: 0,
            text: String::new(),
        }],
    };
    let mut fired = Vec::new();
    assert!(reduce(
        &mut state,
        Packet::Event(event.clone()),
        ME,
        &mut fired
    ));
    assert_eq!(
        fired[0],
        ("CALENDAR_OPEN_EVENT", vec![V::Str("GUILD_EVENT".into())])
    );
    let open = state.open.clone().unwrap();
    assert_eq!(
        (open.event_id, open.invitees[0].signup, open.revision),
        (100, true, 0)
    );
    fired.clear();
    reduce(&mut state, Packet::Event(event), ME, &mut fired);
    assert_eq!(
        events(&fired),
        ["CALENDAR_UPDATE_EVENT", "CALENDAR_UPDATE_INVITE_LIST"]
    );
    assert_eq!(state.open.as_ref().unwrap().revision, 1);
    // The event's removal closes it.
    fired.clear();
    reduce(
        &mut state,
        Packet::RemovedAlert {
            raw_byte: 0,
            event_id: 100,
            packed_time: 0,
        },
        ME,
        &mut fired,
    );
    assert_eq!(
        events(&fired),
        ["CALENDAR_CLOSE_EVENT", "CALENDAR_UPDATE_EVENT_LIST"]
    );
    assert!(state.open.is_none());
}

/// A draft lists its creator, takes a guild filter's members and adds them; the add's echo
/// closes it.
#[test]
fn a_draft_adds_with_its_invites() {
    let mut state = CalendarState::default();
    apply(
        &mut state,
        R::NewDraft {
            flags: 0,
            arena_slot: None,
        },
        ME,
        None,
    );
    let mut fired = Vec::new();
    reduce(
        &mut state,
        Packet::FilterGuild {
            members: vec![(20, 0), (ME, 0)],
        },
        ME,
        &mut fired,
    );
    assert_eq!(events(&fired), ["CALENDAR_UPDATE_INVITE_LIST"]);
    assert_eq!(state.open.as_ref().unwrap().invitees.len(), 2);
    let fields = WrathCalendarFields {
        title: "Raid".into(),
        event_type: 0,
        dungeon: -1,
        date: WrathCalendarDate::new(2010, 3, 20, 20, 0),
        ..Default::default()
    };
    let sent = apply(&mut state, R::AddEvent(fields.clone()), ME, None);
    let [Command::AddEvent {
        title,
        packed_time,
        invites: Some(invites),
        ..
    }] = sent.as_slice()
    else {
        panic!("{sent:?}");
    };
    assert_eq!(
        (title.as_str(), *packed_time, invites.len(), invites[0].guid),
        ("Raid", fields.date.packed(), 1, 20)
    );
    // A draft's moderator and removal stay local.
    assert!(apply(
        &mut state,
        R::RemoveInvite {
            event_id: 0,
            invitee: 20,
            invite_id: 0,
            owner_invite_id: 0,
        },
        ME,
        None,
    )
    .is_empty());
    assert_eq!(state.open.as_ref().unwrap().invitees.len(), 1);
    let mut fired = Vec::new();
    let mut echo = Event {
        send_type: SEND_ADD,
        creator: ME,
        event_id: 300,
        title: "Raid".into(),
        description: String::new(),
        kind: 0,
        repeat: 0,
        max_invites: 0,
        dungeon: -1,
        flags: 0,
        packed_time: fields.date.packed(),
        timezone_time: 0,
        guild_id: 0,
        invitees: vec![],
    };
    echo.invitees.push(EventInvitee {
        guid: ME,
        level: 80,
        status: CONFIRMED,
        rank: OWNER,
        guild_member: 0,
        invite_id: 301,
        status_time: 0,
        text: String::new(),
    });
    assert!(reduce(&mut state, Packet::Event(echo), ME, &mut fired));
    assert_eq!(
        events(&fired),
        ["CALENDAR_NEW_EVENT", "CALENDAR_UPDATE_EVENT_LIST"]
    );
    assert!(state.open.is_none());
    assert_eq!(state.events[0].invite, Some((301, CONFIRMED, OWNER)));
}

/// A result's error raises its line with the name; a lockout update moves its reset.
#[test]
fn errors_name_their_player_and_lockouts_update() {
    let mut state = CalendarState::default();
    reduce(&mut state, calendar_packet(), ME, &mut Vec::new());
    let mut fired = Vec::new();
    assert!(reduce(
        &mut state,
        Packet::CommandResult {
            raw_word: 0,
            raw_byte: 0,
            param: "Bob".into(),
            error: 10,
        },
        ME,
        &mut fired,
    ));
    assert_eq!(
        fired,
        [(
            "CALENDAR_UPDATE_ERROR",
            vec![V::Str(
                "CALENDAR_ERROR_ALREADY_INVITED_TO_EVENT_S\0Bob".into()
            )]
        )]
    );
    assert_eq!(error_key(15), None);
    reduce(
        &mut state,
        Packet::RaidLockoutUpdated {
            packed_time: 0,
            map: 533,
            difficulty: 1,
            old_seconds: 86_400,
            new_seconds: 2 * 86_400,
        },
        ME,
        &mut Vec::new(),
    );
    assert_eq!(state.lockouts[0].0.seconds_left, 2 * 86_400);
    reduce(&mut state, Packet::NumPending { count: 3 }, ME, &mut fired);
    assert_eq!(state.pending, 3);
}
