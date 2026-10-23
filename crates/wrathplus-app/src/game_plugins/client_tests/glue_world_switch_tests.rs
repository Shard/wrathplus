//! The stock glue's systems across the world switch. Shannon's 10-24 crash: the screen follower,
//! gated only on the stock glue being in use (`stock_glue_active`, true all session), ran in the
//! world against the in-world VM, which has no glue API, and its accessor panicked. The accessors
//! no longer panic (`wrathplus_ui`'s `a_vm_without_the_glue_api_answers_nothing`); this keeps the
//! systems that call them off the world's frames.

use bevy::prelude::*;

use crate::game_plugins::schedule_tests::{census, headless_client, SyncPoints};

/// Every `Update` system that reads or writes the glue VM's state runs only while a glue screen
/// is up: `stock_glue_up` (Login, CharSelect, CharCreate under the stock glue) or the create
/// screen's narrower `stock_create_up`. `stock_glue_active` alone is no gate: it never turns off.
#[test]
fn the_glue_systems_run_only_on_the_glue_screens() {
    const GLUE_CALLERS: [&str; 3] = [
        "::follow_glue_screen",
        "::stock_glue",
        "::drive_stock_create",
    ];
    let mut app = headless_client();
    let update = census(&mut app, Update, SyncPoints::Declared);
    for name in GLUE_CALLERS {
        let found: Vec<_> = update
            .systems
            .values()
            .filter(|s| s.name.ends_with(name))
            .collect();
        assert!(!found.is_empty(), "{name} is scheduled in Update");
        for system in found {
            assert!(
                system
                    .conditions
                    .iter()
                    .any(|c| c.ends_with("::stock_glue_up") || c.ends_with("::stock_create_up")),
                "{name} must be gated to the glue screens: {system:#?}"
            );
        }
    }
}
