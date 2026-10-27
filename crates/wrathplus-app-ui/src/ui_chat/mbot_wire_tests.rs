//! MultiBot's bridge traffic both ways, in `mod-multibot-bridge`'s own envelope: `MBOT` TAB the
//! payload, `LANG_ADDON`, the reply a whisper `ChatHandler::BuildChatPacket` builds with the
//! player as sender (`MultiBotBridge.cpp` `SendAddonPacket`, `Chat.cpp:273-349`).

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use wrathplus_protocol::wrath_chat::{decode, encode, Command, LANGUAGE_ADDON};
use wrathplus_ui::script::UiScript;

use super::native_message::{deliver, NativeChatFeed};
use crate::names::NameCache;
use crate::net::{ClientCommand, NetCommands};

/// `MultiBotComm.lua`'s solo send, `SendAddonMessage(prefix, message, "WHISPER", playerName)`,
/// reaches the wire as AzerothCore's `HandleMessagechatOpcode` reads it and as the bridge's
/// `TryExtractBridgePayload` accepts it: whisper, `LANG_ADDON`, the target, `MBOT\t...`.
#[test]
fn an_mbot_hello_leaves_as_a_lang_addon_whisper_to_self() {
    let mut s = UiScript::new().unwrap();
    // 12340 sends nothing without the player object (`0x500560`).
    s.set_unit_guids(&wrathplus_ui::script::UnitGuids {
        player: 0x2a,
        ..Default::default()
    });
    s.run("SendAddonMessage('MBOT', 'HELLO~4.0', 'WHISPER', 'Zed')")
        .unwrap();
    let mut world = World::new();
    world.insert_non_send_resource(s);
    let (tx, rx) = crossbeam_channel::unbounded();
    world.insert_resource(NetCommands(tx));
    world
        .run_system_once(super::input::drain_addon_message_sends)
        .expect("drain");
    let sent: Vec<ClientCommand> = rx.try_iter().collect();
    let [ClientCommand::AddonMessage {
        distribution,
        text,
        target,
    }] = &sent[..]
    else {
        panic!("{sent:?}");
    };
    let (opcode, body) = encode(&Command {
        chat_type: crate::net::wrath_addon_chat_type(*distribution),
        language: LANGUAGE_ADDON,
        target: target.clone().map(String::into_bytes),
        text: text.clone().into_bytes(),
    })
    .unwrap();
    assert_eq!(opcode, 0x95, "CMSG_MESSAGECHAT");
    let want: Vec<u8> = [7u32, u32::MAX]
        .map(u32::to_le_bytes)
        .concat()
        .into_iter()
        .chain(*b"Zed\0MBOT\tHELLO~4.0\0")
        .collect();
    assert_eq!(body, want);
}

/// `BuildChatPacket(data, CHAT_MSG_WHISPER, LANG_ADDON, player, nullptr, wire)`: a GM account's
/// reply is `SMSG_GM_MESSAGECHAT` with the sender's name inline, anyone else's the plain opcode.
fn bridge_reply(gm: bool, sender: u64, wire: &[u8]) -> (u16, Vec<u8>) {
    let mut body = vec![0x07];
    body.extend(LANGUAGE_ADDON.to_le_bytes());
    body.extend(sender.to_le_bytes());
    body.extend(0u32.to_le_bytes());
    if gm {
        body.extend(4u32.to_le_bytes());
        body.extend(b"Zed\0");
    }
    body.extend(0u64.to_le_bytes());
    body.extend((wire.len() as u32 + 1).to_le_bytes());
    body.extend(wire);
    body.extend([0, 0]);
    (if gm { 0x3b3 } else { 0x96 }, body)
}

/// The bridge's reply fires `CHAT_MSG_ADDON` as 12340 splits it: the prefix before the first TAB,
/// the payload after, the WHISPER token and the sender's name, under either opcode.
#[test]
fn an_mbot_reply_fires_chat_msg_addon_with_the_prefix_split() {
    let mut script = UiScript::new().unwrap();
    script
        .run(
            "GOT = {} local f = CreateFrame('Frame') f:RegisterEvent('CHAT_MSG_ADDON') \
             f:SetScript('OnEvent', function(_, _, p, m, d, s) \
               table.insert(GOT, table.concat({p, m, d, s}, '|')) end)",
        )
        .unwrap();
    let mut names = NameCache::default();
    names.insert_player(0x2a, "Zed".into(), None);
    let (tx, _rx) = crossbeam_channel::unbounded();
    for gm in [false, true] {
        let (opcode, body) = bridge_reply(gm, 0x2a, b"MBOT\tHELLO_ACK~1~ok");
        let msg = decode(opcode, &body).expect("AzerothCore's layout decodes");
        assert!(deliver(
            &mut script,
            &mut super::frames::ChatWindows::default(),
            &mut super::edit::ChannelState::default(),
            &names,
            &NetCommands(tx.clone()),
            None,
            &mut NativeChatFeed::default(),
            &msg,
            false,
            &|_, _| None,
            &|_| None,
            &mut |_, _, _| {},
        ));
    }
    let got: Vec<String> = script.eval("return GOT").unwrap();
    assert_eq!(got, ["MBOT|HELLO_ACK~1~ok|WHISPER|Zed"; 2]);
}
