//! The slash-command table on the live route, off the shipped data: built once a VM has
//! `GlobalStrings.lua`, never from one without it.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

use super::commands::{build_slash_commands, Command, SlashCommands};
use crate::app_core::state::EmoteSounds;

/// The client's first VM is the glue screens', which runs no `GlobalStrings.lua`: the build waits,
/// and once the stock interface has loaded `/wave` resolves to WAVE's `EmotesText` id (playtest
/// 073348 logged "NO emote aliases" when it was built from the first VM alone). Skips without
/// client data.
#[test]
fn the_table_waits_for_the_strings_then_wave_resolves_to_its_emote() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
    let emotes = wrathplus_formats::load_emote_sound_catalog(&mut chain).expect("Emotes");
    let mut world = World::new();
    world.insert_resource(EmoteSounds(emotes));
    world.init_resource::<SlashCommands>();

    world.insert_non_send_resource(wrathplus_ui::script::UiScript::new().expect("a VM"));
    world.run_system_once(build_slash_commands).unwrap();
    assert_eq!(
        world.resource::<SlashCommands>().lookup("wave"),
        None,
        "no strings yet"
    );

    let mut script = wrathplus_ui::script::UiScript::new().expect("a VM");
    let _failures = crate::ui_script::load_default_ui(&mut script);
    world.insert_non_send_resource(script);
    world.run_system_once(build_slash_commands).unwrap();
    let want = world
        .resource::<EmoteSounds>()
        .text_id("WAVE")
        .expect("EmotesText names WAVE");
    let table = world.resource::<SlashCommands>();
    assert_eq!(table.lookup("wave"), Some(Command::Emote { text_id: want }));
    assert_eq!(table.lookup("WAVE"), Some(Command::Emote { text_id: want }));
    assert!(
        table.lookup("help").is_some(),
        "the SLASH_ commands come with it"
    );
}
