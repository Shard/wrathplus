//! Typed chat to the wire, on the installed stock 3.3.5 chat frame: the ENTER binding's
//! `ChatFrame_OpenChat("")`, the typed line, ENTER, then the drain's `ClientCommand::Chat`.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use wrathplus_ui::script::UiScript;

use crate::net::{ClientCommand, NetCommands};

/// The stock chat frame, as `ui_script::chat_tests` seats it, for a Horde character.
fn chat_vm() -> UiScript {
    let mut s = UiScript::new().unwrap();
    for f in [
        "Interface\\FrameXML\\GlobalStrings.lua",
        "Interface\\FrameXML\\Fonts.xml",
        "Interface\\FrameXML\\BasicControls.xml",
        r"Interface\FrameXML\UIParent.xml",
        r"Interface\FrameXML\MoneyFrame.lua",
        r"Interface\FrameXML\MoneyFrame.xml",
        "Interface\\FrameXML\\GameTooltip.xml",
        "Interface\\FrameXML\\UIMenu.xml",
        "Interface\\FrameXML\\UIDropDownMenu.xml",
        "Interface\\FrameXML\\UIPanelTemplates.lua",
        "Interface\\FrameXML\\UIPanelTemplates.xml",
        r"Interface\FrameXML\StaticPopup.xml",
        "Interface\\FrameXML\\TextStatusBar.lua",
        "Interface\\FrameXML\\TextStatusBar.xml",
        "Interface\\FrameXML\\MainMenuBar.xml",
        "Interface\\FrameXML\\Cooldown.xml",
        "Interface\\FrameXML\\ActionButtonTemplate.xml",
        "Interface\\FrameXML\\ChatFrame.xml",
        "Interface\\FrameXML\\FloatingChatFrame.xml",
    ] {
        crate::ui_script::test_ui::load_ui(&mut s, f);
    }
    s.set_language_table(vec![(1, "Orcish".into()), (7, "Common".into())]);
    s.set_known_languages(vec!["Orcish".into()]);
    s.set_default_language(Some("Orcish".into()));
    crate::ui_script::fire_chat_login(&mut s);
    s.set_screen_size(1600.0, 900.0);
    s.resolve();
    s
}

/// The drain's world, holding `vm`.
fn world(vm: UiScript) -> (World, crossbeam_channel::Receiver<ClientCommand>) {
    let mut world = World::new();
    world.insert_non_send_resource(vm);
    let (tx, rx) = crossbeam_channel::unbounded();
    world.insert_resource(NetCommands(tx));
    world.init_resource::<super::feed::ChatLog>();
    world.init_resource::<super::away::AfkMirror>();
    world.init_resource::<crate::cvars::Cvars>();
    world.init_resource::<super::edit::ChannelState>();
    (world, rx)
}

/// One line typed into the box the ENTER binding opens.
fn type_line(world: &mut World, line: &str) {
    let mut s = world.non_send_resource_mut::<UiScript>();
    s.run(r#"ChatFrame_OpenChat("")"#).unwrap();
    assert!(s.has_keyboard_focus(), "the binding opens the box");
    s.char_input(line);
    assert!(s.key_input("ENTER"), "the box takes ENTER");
    assert!(s.errors().is_empty(), "{line}: {:?}", s.errors());
}

/// AzerothCore's `HandleMessagechatOpcode` read (`ChatHandler.cpp:66-284`): `u32 type, u32 lang`,
/// then the whisper's or channel's target and the text, each NUL-ended; 12340's type ids.
fn ac_bytes(chat_type: u32, language: u32, target: Option<&str>, text: &str) -> Vec<u8> {
    let mut body = [chat_type, language].map(u32::to_le_bytes).concat();
    if let Some(target) = target {
        body.extend(target.bytes().chain([0]));
    }
    body.extend(text.bytes().chain([0]));
    body
}

/// The drained command as `NativeChatLine` writes it (`net::io::wrath::chat_line`).
fn wire(command: &ClientCommand) -> Vec<u8> {
    let ClientCommand::Chat {
        kind,
        target,
        text,
        language,
    } = command
    else {
        panic!("not a chat line: {command:?}");
    };
    let chat_type = kind.wrath_chat_type();
    let line = wrathplus_protocol::wrath_chat::Command {
        chat_type,
        language: language.expect("the box speaks a language"),
        target: matches!(chat_type, 0x07 | 0x11)
            .then(|| target.clone().unwrap_or_default().into_bytes()),
        text: text.clone().into_bytes(),
    };
    let (opcode, body) = wrathplus_protocol::wrath_chat::encode(&line).expect("encodes");
    assert_eq!(opcode, 0x95, "CMSG_MESSAGECHAT");
    body
}

/// Typed into the box the ENTER binding opens, every chat-bar type reaches the wire in
/// AzerothCore's layout, Orcish (1) for this Horde character. The channel line is the
/// binding's own call: a typed `/1` also needs the VM's channel roster.
#[test]
fn typed_lines_reach_the_wire_per_chat_type() {
    wrathplus_formats::wow_data_or_skip!();
    let mut vm = chat_vm();
    // In a party: with none, `ChatFrame_OpenChat` drops a sticky PARTY back to SAY.
    vm.set_party(wrathplus_ui::script::PartyState {
        own_guid: 1,
        leader_guid: 1,
        members: vec![wrathplus_ui::script::PartyMemberInfo {
            guid: 2,
            name: "Alpha".into(),
        }],
        ..Default::default()
    });
    let (mut world, rx) = world(vm);
    {
        let mut channels = world.resource_mut::<super::edit::ChannelState>();
        channels.claim_slot("General - Orgrimmar");
        channels.confirm_slot("General - Orgrimmar");
    }
    // Server commands are ordinary lines: `ChatEdit_ParseText` returns at once unless a line starts
    // with "/", and AzerothCore parses the dot itself. The box keeps its type, sticky.
    const DOT: &str = ".playerbots bot addclass warrior";
    for (line, chat_type, target, text) in [
        ("hello", 0x01, None, "hello"),
        (DOT, 0x01, None, DOT),
        ("#tag", 0x01, None, "#tag"),
        ("!bang", 0x01, None, "!bang"),
        ("/s hi", 0x01, None, "hi"),
        ("/y loud", 0x06, None, "loud"),
        ("/e waves", 0x0a, None, "waves"),
        ("/p group", 0x02, None, "group"),
        (DOT, 0x02, None, DOT),
        ("/p .playerbots bot addclass warrior", 0x02, None, DOT),
        ("/g guild", 0x04, None, "guild"),
        ("/w Thrall psst", 0x07, Some("Thrall"), "psst"),
        ("/w Thrall .help", 0x07, Some("Thrall"), ".help"),
        ("CHANNEL", 0x11, Some("General - Orgrimmar"), "lfg"),
    ] {
        if line == "CHANNEL" {
            let s = world.non_send_resource::<UiScript>();
            s.run(r#"SendChatMessage("lfg", "CHANNEL", "Orcish", "1")"#)
                .unwrap();
        } else {
            type_line(&mut world, line);
        }
        world
            .run_system_once(super::input::drain_addon_chat_sends)
            .expect("drain");
        let sent: Vec<ClientCommand> = rx.try_iter().collect();
        let [command] = &sent[..] else {
            panic!("{line:?}: {sent:?}");
        };
        assert_eq!(
            wire(command),
            ac_bytes(chat_type, 1, target, text),
            "{line:?}"
        );
    }
}
