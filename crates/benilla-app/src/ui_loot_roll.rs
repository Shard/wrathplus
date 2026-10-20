//! The group loot roll: build 12340's native feed, presentation and outcomes ([`wrath`]), and the
//! drain that sends the Lua votes.

mod wrath;

use benilla_ui::script::UiScript;
use bevy::prelude::*;

use crate::net::NetCommands;
use crate::ui_script::UiInput;

pub(crate) struct UiLootRollPlugin;

impl Plugin for UiLootRollPlugin {
    fn build(&self, app: &mut App) {
        wrath::register(app);
        // After the input pass, so a vote goes out the frame it was cast.
        app.add_systems(Update, drain_loot_rolls.after(UiInput));
    }
}

/// Drain the Lua votes and confirms through the native presentation.
fn drain_loot_rolls(
    script: Option<NonSendMut<UiScript>>,
    mut native: ResMut<wrath::feed::Presentation>,
    commands: Res<NetCommands>,
) {
    let Some(mut script) = script else {
        return;
    };
    wrath::feed::drain(&mut script, &mut native, &commands);
}
