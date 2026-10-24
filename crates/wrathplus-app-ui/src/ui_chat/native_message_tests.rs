//! The native chat feed, packet by packet through the stock event names.
use super::*;
fn message(kind: u8) -> Message {
    Message {
        gm: false,
        chat_type: kind,
        language: 0,
        sender_guid: 0,
        flags: 0,
        sender_name: Some(b"Speaker".to_vec()),
        receiver_guid: 0,
        receiver_name: Some(b"Receiver".to_vec()),
        channel: None,
        text: b"hello".to_vec(),
        chat_tag: 0,
        achievement_id: None,
    }
}
#[test]
fn native_tag_is_not_vanilla_enum_or_invented_bit_precedence() {
    assert_eq!(native_tag(1), "AFK");
    assert_eq!(native_tag(2), "DND");
    assert_eq!(native_tag(4), "GM");
    for tag in [0, 3, 5, 7, 8, 16, 255] {
        assert_eq!(native_tag(tag), "");
    }
}

#[test]
fn the_drain_keeps_every_packet_in_order() {
    let mut addon = message(4);
    addon.language = u32::MAX;
    addon.text = vec![0, 255, b'\t', 128];
    let mut achievement = message(0x30);
    achievement.text = b"$a earned %s".to_vec();
    achievement.achievement_id = Some(42);
    let mut fifo = std::collections::VecDeque::from([
        addon.clone(),
        message(1),
        achievement.clone(),
        message(6),
        message(0x31),
    ]);
    let visible = take_visible(&mut fifo);
    assert_eq!(
        visible.iter().map(|m| m.chat_type).collect::<Vec<_>>(),
        vec![4, 1, 0x30, 6, 0x31]
    );
    assert_eq!(visible[0].text, addon.text, "addon lines arrive losslessly");
    assert_eq!(visible[2].achievement_id, Some(42));
    assert!(fifo.is_empty());
}

/// `0x578f20`'s `$a`: the earner's dated, completed link for a known row, nothing for an
/// unknown one; other `$` text and the sender's `%s` stay.
#[test]
fn achievement_lines_expand_the_native_link() {
    let known = |id| (id == 6).then(|| ("Level 10".to_string(), (10, 10, 16)));
    assert_eq!(
        expand_achievement(
            "%s has earned the achievement $a!",
            6,
            0x0700_0000_0012_3456,
            &known
        ),
        "%s has earned the achievement |cffffff00|Hachievement:6:0700000000123456:1:10:16:10:\
         4294967295:4294967295:4294967295:4294967295|h[Level 10]|h|r!"
    );
    assert_eq!(expand_achievement("got $A, $5", 7, 1, &known), "got , $5");
}

/// `0x509dd0`: the C string ends at its first NUL, the first TAB splits prefix from body
/// (later TABs stay), bytes arrive undecoded, the type picks the token, and an unnamed sender
/// waits while a zero GUID is an empty sender.
#[test]
fn addon_lines_fire_chat_msg_addon_with_lossless_bytes() {
    let mut script = UiScript::new().unwrap();
    script
        .run(
            "GOT = {} local function hex(s) \
               return (s:gsub('.', function(c) return string.format('%02x', c:byte()) end)) end \
             local f = CreateFrame('Frame') f:RegisterEvent('CHAT_MSG_ADDON') \
             f:SetScript('OnEvent', function(_, _, p, b, d, s) \
               table.insert(GOT, hex(p) .. '|' .. hex(b) .. '|' .. d .. '|' .. s) end)",
        )
        .unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let send = |script: &mut UiScript, kind: u8, text: &[u8], guid: u64| {
        let mut msg = message(kind);
        msg.language = u32::MAX;
        msg.text = text.to_vec();
        msg.sender_name = None;
        msg.sender_guid = guid;
        deliver(
            script,
            &mut ChatWindows::default(),
            &mut ChannelState::default(),
            &NameCache::default(),
            &NetCommands(tx.clone()),
            None,
            &mut NativeChatFeed::default(),
            &msg,
            false,
            &|_, _| None,
            &|_| None,
            &mut |_, _, _| {},
        )
    };
    assert!(send(&mut script, 2, b"Pfx\tbo\tdy\0tail", 0));
    assert!(send(&mut script, 0x2f, b"\xff\x80", 0));
    assert!(send(&mut script, 0x2c, b"x\ty", 0));
    assert!(send(&mut script, 0x11, b"c\td", 0));
    assert!(
        !send(&mut script, 7, b"w\tz", 0x55),
        "an unnamed sender waits"
    );
    let got: Vec<String> = script.eval("return GOT").unwrap();
    assert_eq!(
        got,
        [
            // "Pfx" | "bo\tdy": the second TAB stays, the NUL ends the text.
            "506678|626f096479|PARTY|",
            "ff80||WHISPER|",
            "78|79|BATTLEGROUND|",
            "63|64|UNKNOWN|",
        ]
    );
}

#[test]
fn basic_visible_types_dispatch_once_with_native_event_names() {
    let mut script = UiScript::new().unwrap();
    script
        .lua()
        .load(
            r#"
        received = {}
        local f = CreateFrame('Frame')
        f:RegisterAllEvents()
        f:SetScript('OnEvent', function(self, event, ...)
            table.insert(received, event)
        end)
    "#,
        )
        .exec()
        .unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let mut state = NativeChatFeed::default();
    let types = [
        0, 1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 17, 27, 28, 39, 40, 44, 45, 51,
    ];
    for kind in types {
        let mut msg = message(kind);
        // Preserve the native GM-opcode discriminator without changing the event type.
        msg.gm = kind == 7;
        assert!(deliver(
            &mut script,
            &mut ChatWindows::default(),
            &mut ChannelState::default(),
            &NameCache::default(),
            &NetCommands(tx.clone()),
            None,
            &mut state,
            &msg,
            false,
            &|_, _| None,
            &|_| None,
            &mut |_, _, _| {}
        ));
    }
    let received: Vec<String> = script.eval("return received").unwrap();
    assert_eq!(
        received,
        types.map(|kind| format!("CHAT_MSG_{}", token(kind).unwrap()))
    );
}

#[test]
fn native_types_do_not_use_vanilla_numbers() {
    assert_eq!(token(0), Some("SYSTEM"));
    assert_eq!(token(6), Some("YELL"));
    assert_eq!(token(7), Some("WHISPER"));
    assert_eq!(token(0x0c), Some("MONSTER_SAY"));
    assert_eq!(token(0x11), Some("CHANNEL"));
    assert_eq!(token(0x30), Some("ACHIEVEMENT"));
    for t in 0..0x34 {
        assert!(token(t).is_some());
    }
    assert_eq!(token(0xff), None);
}
#[test]
fn named_monster_receives_native_event_receiver_tag_guid_and_line_id() {
    let mut script = UiScript::new().unwrap();
    script
        .lua()
        .load(
            r#"
        received = {}
        local f = CreateFrame('Frame')
        f:RegisterEvent('CHAT_MSG_MONSTER_SAY')
        f:SetScript('OnEvent', function(self, event, ...)
            table.insert(received, {event, ...})
        end)
    "#,
        )
        .exec()
        .unwrap();
    let mut msg = message(0x0c);
    msg.sender_guid = 0xf130000001000001;
    msg.chat_tag = 4;
    let (tx, _rx) = crossbeam_channel::unbounded();
    let mut state = NativeChatFeed::default();
    for _ in 0..2 {
        assert!(deliver(
            &mut script,
            &mut ChatWindows::default(),
            &mut ChannelState::default(),
            &NameCache::default(),
            &NetCommands(tx.clone()),
            None,
            &mut state,
            &msg,
            false,
            &|_, _| None,
            &|_| None,
            &mut |_, _, _| {}
        ));
    }
    assert!(script
        .eval::<bool>(
            r#"return #received == 2 and
        received[1][1] == 'CHAT_MSG_MONSTER_SAY' and received[1][2] == 'hello' and
        received[1][3] == 'Speaker' and received[1][6] == 'Receiver' and
        received[1][7] == 'GM' and received[1][12] == 1 and
        received[2][12] == 2 and received[1][13] == '0xF130000001000001'"#
        )
        .unwrap());
}
#[test]
fn absent_player_name_parks_original_until_bounded_expiry() {
    let mut script = UiScript::new().unwrap();
    let mut msg = message(7);
    msg.sender_name = None;
    msg.sender_guid = 123;
    let (tx, _rx) = crossbeam_channel::unbounded();
    let commands = NetCommands(tx);
    let mut state = NativeChatFeed::default();
    let mut windows = ChatWindows::default();
    let mut channels = ChannelState::default();
    let names = NameCache::default();
    assert!(!deliver(
        &mut script,
        &mut windows,
        &mut channels,
        &names,
        &commands,
        None,
        &mut state,
        &msg,
        false,
        &|_, _| None,
        &|_| None,
        &mut |_, _, _| {}
    ));
    assert_eq!(state.line, 0);
    assert!(deliver(
        &mut script,
        &mut windows,
        &mut channels,
        &names,
        &commands,
        None,
        &mut state,
        &msg,
        true,
        &|_, _| None,
        &|_| None,
        &mut |_, _, _| {}
    ));
    assert_eq!(state.line, 1);
}

/// `0x509dd0` hands each delivered line to `0x720010`, which bubbles say, party, yell, monster
/// say and yell and party leader; a monster line reads `%%` as `%`.
#[test]
fn bubbling_types_reach_the_bubble_queue_with_their_text() {
    let mut script = UiScript::new().unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let mut state = NativeChatFeed::default();
    let mut bubbles = Vec::new();
    for (kind, text) in [
        (1u8, "hello"),
        (2, "group"),
        (6, "LOUD"),
        (7, "psst"),
        (0x0c, "100%% sure"),
        (0x0e, "die"),
        (0x10, "grunts"),
        (0x33, "lead"),
    ] {
        let mut msg = message(kind);
        msg.sender_guid = 0xF130_0029_3C00_47EB;
        msg.text = text.as_bytes().to_vec();
        assert!(deliver(
            &mut script,
            &mut ChatWindows::default(),
            &mut ChannelState::default(),
            &NameCache::default(),
            &NetCommands(tx.clone()),
            None,
            &mut state,
            &msg,
            false,
            &|_, _| None,
            &|_| None,
            &mut |guid, kind, text| bubbles.push((guid, kind, text.to_string())),
        ));
    }
    use ChatEventKind as K;
    let got: Vec<(ChatEventKind, &str)> =
        bubbles.iter().map(|(_, k, t)| (*k, t.as_str())).collect();
    assert_eq!(
        got,
        [
            (K::Say, "hello"),
            (K::Party, "group"),
            (K::Yell, "LOUD"),
            (K::MonsterSay, "100% sure"),
            (K::MonsterYell, "die"),
            (K::Party, "lead"),
        ]
    );
    assert!(bubbles.iter().all(|(g, ..)| *g == 0xF130_0029_3C00_47EB));
}
