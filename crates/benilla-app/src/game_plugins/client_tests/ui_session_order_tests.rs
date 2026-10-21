//! The schedule seat of `crate::ui_session::feed_interact_npc`, from `ui_session.rs`. It assembles
//! the net, motion and session plugins and reads the root's `test_support`, so it lives in the
//! root layer.

use bevy::prelude::*;

use crate::test_support::runs_before;
use crate::ui_session::{feed_interact_npc, UiSessionPlugin};

#[test]
fn the_interact_npc_writer_is_seated_between_the_net_apply_and_the_facing_chain() {
    let mut app = App::new();
    app.add_plugins((
        crate::net::NetPlugin { connect: false },
        crate::motion::MotionPlugin,
        UiSessionPlugin,
    ));
    assert!(
        runs_before(&mut app, crate::net::apply_net_updates, feed_interact_npc),
        "feed_interact_npc must run after apply_net_updates"
    );
    assert!(
        runs_before(
            &mut app,
            feed_interact_npc,
            crate::motion::drive_display_facing
        ),
        "drive_display_facing must run after feed_interact_npc"
    );
}
