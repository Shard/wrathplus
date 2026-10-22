//! Whole-client tests of `crate::net::handlers`, from `net/handlers.rs` (`tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::net::handlers::*;
use wrathplus_protocol::SessionEvent;
use wrathplus_protocol::SessionEventKind;

/// A kind with no handler is a decoded packet dropped in silence, as the reference does.
#[test]
fn every_session_event_kind_has_one_owner() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let table = app.world_mut().resource::<NetHandlers>().census();
    let unowned: Vec<String> = SessionEventKind::all()
        .filter(|k| !table.contains_key(k))
        .map(|k| format!("{k:?}: decoded and dropped — no handler in the table"))
        .collect();
    eprintln!(
        "session event kinds: {} — {} in the handler table ({} handlers)",
        SessionEventKind::all().count(),
        table.len(),
        table.values().map(Vec::len).sum::<usize>(),
    );
    assert!(unowned.is_empty(), "{}", unowned.join("\n"));
}

/// A handler's parameters are fetched only when its packet arrives, which for a rare packet is
/// never in a smoke run; this fetches them all once, with an event each handler ignores.
#[test]
fn every_registered_handler_can_run_on_the_built_client() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let world = app.world_mut();
    let handlers = world
        .remove_resource::<NetHandlers>()
        .expect("the built client registers handlers");
    let failed = handlers.probe(world, &SessionEvent::LoggedOut);
    let count: usize = handlers.census().values().map(Vec::len).sum();
    eprintln!(
        "net handlers probed on the built client: {count}, {} failed",
        failed.len()
    );
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// A handler registered under the wrong kind compiles and silently ignores its packet, so each
/// handler must name `SessionEvent::<Kind>` in its body; a session-end listener is exempt.
#[test]
fn every_handler_names_the_kind_it_registered_for() {
    // The whole app: every layer crate after the split.
    let files: Vec<_> = crate::app_core::source_tree::rust_files()
        .into_iter()
        .map(|(_, path)| path)
        .collect();
    let (mut seen, mut problems) = (0, Vec::new());
    for path in files {
        if path.ends_with("net/handlers.rs") || path.ends_with("client_tests/net_handlers.rs") {
            continue; // the registry's and this test's own fixtures and prose
        }
        let text = regex_lite_strip_comments(&std::fs::read_to_string(&path).expect("source"));
        for call in text.split("net_handler(").skip(1) {
            let Some((args, _)) = call.split_once(')') else {
                continue;
            };
            let Some((kind, handler)) = args.split_once(',') else {
                continue;
            };
            let kind = kind.trim().rsplit("::").next().unwrap_or_default();
            // rustfmt breaks a long call one argument a line, with a trailing comma.
            let handler = handler.trim().trim_end_matches(',').trim_end();
            seen += 1;
            if kind == "Disconnected" {
                continue;
            }
            let Some(at) = text.find(&format!("fn {handler}(")) else {
                problems.push(format!(
                    "{}: `{handler}` is not in the file that registers it",
                    path.display()
                ));
                continue;
            };
            // The handler's own lines: from its `fn` to the next item's.
            let body: Vec<&str> = text[at..]
                .lines()
                .enumerate()
                .take_while(|(i, l)| {
                    let l = l.trim_start();
                    *i == 0
                        || !(l.starts_with("fn ") || l.starts_with("pub") || l.starts_with("#["))
                })
                .map(|(_, l)| l)
                .collect();
            // The whole variant name: `Chat` is also a prefix of `ChatPlayerNotFound`.
            let names_kind = {
                let body = body.join("\n");
                let needle = format!("SessionEvent::{kind}");
                body.match_indices(&needle).any(|(at, hit)| {
                    !body[at + hit.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
                })
            };
            if !names_kind {
                problems.push(format!(
                    "{}: `{handler}` is registered for {kind} and never names it",
                    path.display()
                ));
            }
        }
    }
    let registered: usize = crate::game_plugins::schedule_tests::headless_client()
        .world()
        .resource::<NetHandlers>()
        .census()
        .values()
        .map(Vec::len)
        .sum();
    assert_eq!(
        seen, registered,
        "a registration this scan cannot read — spell it `net_handler(K::Kind, handler)`"
    );
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// `//` comments out, so a variant named in prose does not count as an arm.
fn regex_lite_strip_comments(s: &str) -> String {
    s.lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
