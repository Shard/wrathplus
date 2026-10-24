//! Installed: a monster line's `$` tokens expand against its receiver before the stock
//! `ChatFrame_MessageEventHandler` sees it (`0x509dd0` → `0x579590`), with the quest panels'
//! expander.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use wrathplus_protocol::wrath_chat::Message;
use wrathplus_ui::script::UiScript;

use super::edit::ChannelState;
use super::frames::ChatWindows;
use super::native_message::{deliver, NativeChatFeed};
use super::stock_print_tests::{printed, PRINTED};
use crate::app_core::session::ObjectStore;
use crate::app_core::state::WorldStates;
use crate::names::NameCache;
use crate::net::{GuidIndex, NetCommands};

const ME: u64 = 0x0000_0000_0000_0009;
const PEON: u64 = 0xF130_0029_3C00_47EB;

/// One `SMSG_MESSAGECHAT` from a Lazy Peon to the player `Locktest` of `gender`, through the
/// native feed and the stock chat frame: what `ChatFrame1` printed.
fn monster_line(chat_type: u8, text: &str, gender: u8) -> Vec<String> {
    let mut world = World::new();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let mut names = NameCache::default();
    // Orc (2) Warlock (9).
    names.insert_player(ME, "Locktest".into(), Some((2, 9, gender)));
    world.insert_resource(names);
    world.insert_resource(NetCommands(tx));
    world.init_resource::<GuidIndex>();
    world.init_resource::<WorldStates>();
    let script = super::tests::chat_vm();
    script.run(PRINTED).unwrap();
    world.insert_non_send_resource(script);
    let msg = Message {
        gm: false,
        chat_type,
        language: 0,
        sender_guid: PEON,
        flags: 0,
        sender_name: Some(b"Lazy Peon".to_vec()),
        receiver_guid: ME,
        receiver_name: None,
        channel: None,
        text: text.as_bytes().to_vec(),
        chat_tag: 0,
        achievement_id: None,
    };
    let delivered = world
        .run_system_once(
            move |mut script: NonSendMut<UiScript>,
                  guids: Res<GuidIndex>,
                  stores: Query<&ObjectStore>,
                  names: Res<NameCache>,
                  commands: Res<NetCommands>,
                  states: Res<WorldStates>| {
                deliver(
                    &mut script,
                    &mut ChatWindows::default(),
                    &mut ChannelState::default(),
                    &names,
                    &commands,
                    None,
                    &mut NativeChatFeed::default(),
                    &msg,
                    false,
                    &|text, guid| {
                        crate::npc_text::chat_macros(
                            text,
                            guid,
                            (&guids, &stores),
                            &names,
                            &commands,
                            &states,
                        )
                    },
                    &|_| None,
                )
            },
        )
        .expect("deliver");
    assert!(delivered, "the receiver's name is known, so nothing parks");
    let script = world.non_send_resource::<UiScript>();
    assert!(script.errors().is_empty(), "{:?}", script.errors());
    printed(script)
}

/// The playtest line: `$n` is the receiver's name. Skips without client data.
#[test]
fn a_monster_say_names_its_receiver() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let printed = monster_line(0x0c, "Ow!  OK, I'll get back to work, $n!", 0);
    assert_eq!(printed.len(), 1, "{printed:?}");
    assert!(
        printed[0].ends_with("says: Ow!  OK, I'll get back to work, Locktest!"),
        "{printed:?}"
    );
}

/// `$g` picks its arm by the receiver's gender: the first for male (0), else the second; a yell
/// is in the gate too. Skips without client data.
#[test]
fn a_gender_choice_follows_the_receivers_gender() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let line = "Well met, $gman:woman;!";
    assert!(monster_line(0x0c, line, 0)[0].ends_with("Well met, man!"));
    assert!(monster_line(0x0e, line, 1)[0].ends_with("Well met, woman!"));
}

/// A say from a player is outside `0x509dd0`'s gate and keeps its `$`. Skips without client data.
#[test]
fn a_player_say_is_not_expanded() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let printed = monster_line(0x01, "costs $5", 0);
    assert!(printed[0].ends_with("costs $5"), "{printed:?}");
}

/// The quest panels' call ([`crate::npc_text::substitute`], as `ui_quest_log` builds a detail)
/// and the chat line expand one text alike. Skips without client data.
#[test]
fn quest_text_and_chat_share_the_expander() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let text = "You look like the right $r for my task, $n. Go, $glad:lass;!";
    let subject = crate::npc_text::Subject {
        name: "Locktest".into(),
        race: 2,
        class: 9,
        gender: 1,
    };
    let states = WorldStates::default();
    let quest = crate::npc_text::substitute(
        text,
        &crate::npc_text::MacroContext {
            subject: Some(&subject),
            states: &states,
        },
    );
    assert_eq!(
        quest,
        "You look like the right orc for my task, Locktest. Go, lass!"
    );
    let printed = monster_line(0x0c, text, 1);
    assert!(
        printed[0].ends_with(&format!("says: {quest}")),
        "{printed:?}"
    );
}
