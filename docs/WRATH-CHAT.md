# Chat, channels and emotes

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `SharedDefines.h` (`ChatMsg`),
`Chat.cpp` (`BuildChatPacket`, `SendNotification`), `ChatHandler.cpp`, `ChatPackets.cpp`,
`Channel.{h,cpp}` (`Make*`), `ChannelHandler.cpp`, `WorldSession.cpp`, `Creature.cpp`,
`MotdMgr.cpp`, `MiscPackets.cpp`. No client-only behaviour is inferred; no Ghidra round. Codecs:
`wrath_chat.rs` (existing lossless chat codec, now with `shared_message`), `wrath_chat_notices.rs`.
Agreed with App Migration (2026-10-16).

**Received chat** (0096, GM 03B3): every line stays in `ui_native_chat`'s `NativeChatPackets` as
raw bytes with sender GUID and chat type, and that FIFO is the only input of App Migration's native
feed (`ui_chat::native_message`), which fires the stock 12340 `CHAT_MSG_*` events; addon and
achievement lines stay deferred there for App's units. The wire emits **no** shared
`SessionEvent::Chat` for these opcodes: 3de4d931 did, and every line was delivered twice (native and
1.12-shaped); removed, with regression tests on both sides of the seam (one SAY packet fires exactly
one `CHAT_MSG_SAY` through the real feed).

**Shared notices (handled)**: 0099 every channel notice (you-joined/left keep `wrath_channel`'s
path; the other tails match 1.12, plus 0x20/0x21 empty and 0x22/0x23 voice on/off with the actor),
0103 emote, 0105 text emote (AC writes the name length without its NUL and a lone NUL for a name of
one byte or none; the emote number is dropped), 01CB notification, 0219 wrong faction, 0254 zone
under attack, 0291 server message, 02A9 player not found, 02B8 area-trigger message (length-checked),
02D2 play sound, 02FD restricted (the restriction byte is dropped), 033D message of the day (one
shared system line per line). **Capture-only**: 032D player ambiguous (`NativeChatNoticePackets`).

**Outbound** (existing producers in `ui_chat`; no server acceptance claimed): 0095 chat lines in
this build's `ChatMsg` numbering, speaking the character's own tongue (`faction_language`, which now
counts blood elves as Horde) when none is named, whisper/channel naming their target; addon lines in
`LANG_ADDON` on party/raid/guild/battleground. 0097 join carries the ChatChannels.dbc id of a zone or
recruitment channel (resolved from the catalog), 0 for custom, manual and saved joins; 0098 leave;
009A list; 009C..00A8 password, owner, moderator, mute, invite, kick, ban, announcements, moderate;
0104 text emote (emote number 0, which AC only echoes); 0225 ignore (with AC's unused trailing byte).

Not actionable: 0102 `CMSG_EMOTE` (no producer or command), 033A defense message (no AC sender),
0331 chat filtered, voice rows (out of scope).

## Channel members (`P/wrath_channel_members.rs`)

AC `Chat/Channels/Channel.cpp`, `Handlers/ChannelHandler.cpp`: 03F0/03F2 userlist add/update (GUID,
member flags, channel flags, member count, channel name) and 03F1 remove (GUID, channel flags,
count, name), sent to a channel's watchers, and 03D5 member count (name, flags, count) are decoded
and captured (`NativeChannelMembers`). The roster feed (`A/ui_chat/native_feed.rs`) applies the
deltas through Native UI's `apply_wrath_channel_roster_delta` after the full lists, resolving an
added member's name by revision as for a full list; the UI fires the roster events. No 12340
binding sends the member-count request (Wow.exe has no `0x3D4` sender), so its answer is dropped.
Writers on the channel route: `ClearChannelWatch` 03F3 from `ClearChannelWatch()` (`0x4ffc50`),
bodyless as 12340 sends it, and `DeclineChannelInvite` 0410 with the channel name from
`DeclineInvite("channel")` (`0x4ffcc0`, the invite popup's `OnHide`; AC reads nothing from it).
`SetChannelWatch("channel")` (`0x4ffc30`) feeds the same watch request as a roster selection.
