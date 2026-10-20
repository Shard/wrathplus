//! Whole-client tests of `crate::net`, from `net/capture_replay_tests.rs`.
//! They build the client with `headless_client`, so they live in the root layer.
//!
//! A recorded capture through the real app handler drain on the headless client: the second half
//! of the replay harness (`cargo run -p benilla-protocol --example replay_capture` is the first).
//!
//! ```sh
//! WOW_REPLAY_CAPTURE=<capture.azbwrpk> cargo test -p benilla-app --lib capture_replay -- \
//!     --ignored --nocapture --test-threads 1
//! ```
//!
//! Reports each handler panic with the event that caused it, and the event kinds no handler owns
//! (dropped silently in the live client). The headless client runs no schedule, so per-frame
//! feeds are not exercised; this proves the drain accepts every event the capture produces.

use benilla_protocol::wrath_capture::{self, Record};
use benilla_protocol::{SessionEvent, SessionEventKind};
use std::collections::BTreeMap;

/// Each event through the drain alone, with its panic caught: `(event kind, panic)` per failure,
/// and the produced kinds no handler owns.
fn drain(events: Vec<SessionEvent>) -> (Vec<(String, String)>, BTreeMap<String, usize>) {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let owned: std::collections::BTreeSet<SessionEventKind> = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census()
        .into_keys()
        .collect();
    let mut panics = Vec::new();
    let mut orphans = BTreeMap::new();
    for event in events {
        let kind = SessionEventKind::from(&event);
        if !owned.contains(&kind) {
            *orphans.entry(format!("{kind:?}")).or_default() += 1;
            continue;
        }
        let world = app.world_mut();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::net::handlers::dispatch(world, vec![event]);
        }));
        if let Err(cause) = result {
            let text = cause
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| cause.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            panics.push((format!("{kind:?}"), text));
        }
    }
    (panics, orphans)
}

#[test]
#[ignore = "replays WOW_REPLAY_CAPTURE through the app drain"]
fn capture_replay_through_the_app_drain() {
    let Some(path) = std::env::var_os("WOW_REPLAY_CAPTURE") else {
        eprintln!("set WOW_REPLAY_CAPTURE=<capture.azbwrpk>");
        return;
    };
    let bytes = std::fs::read(&path).expect("read capture");
    let capture = wrath_capture::read(&bytes).expect("an AZBWRPK1 capture");
    let (census, events) = wrath_capture::census(&capture.records);
    eprintln!(
        "{} records, {} packets, {} session events{}",
        capture.records.len(),
        census.packets,
        events.len(),
        if capture.truncated_tail {
            ", final record cut off"
        } else {
            ""
        }
    );
    let (panics, orphans) = drain(events);
    eprintln!("handler panics: {}", panics.len());
    for (kind, text) in &panics {
        eprintln!("  {kind}: {text}");
    }
    eprintln!("event kinds no handler owns:");
    for (kind, n) in &orphans {
        eprintln!("  {kind:<40} {n:>6}");
    }
    assert!(panics.is_empty(), "{} handler panics", panics.len());
}

/// The harness on a synthetic capture: shared events reach their owners without a panic.
#[test]
fn a_synthetic_capture_drains_cleanly() {
    let records = vec![
        Record {
            elapsed_us: 0,
            opcode: 0x1c8,
            body: vec![],
        },
        Record {
            elapsed_us: 1,
            opcode: 0x277,
            body: 11u32.to_le_bytes().to_vec(),
        },
        Record {
            elapsed_us: 2,
            opcode: 0x3c9,
            body: vec![2, 0],
        },
    ];
    let (census, events) = wrath_capture::census(&records);
    assert_eq!(census.packets, 3);
    assert_eq!(events.len(), 3);
    let (panics, orphans) = drain(events);
    assert!(panics.is_empty(), "{panics:?}");
    assert!(orphans.is_empty(), "{orphans:?}");
}
