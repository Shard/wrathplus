# Chat, channels and emotes

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `SharedDefines.h` (`ChatMsg`),
`Chat.cpp` (`BuildChatPacket`, `SendNotification`), `ChatHandler.cpp`, `ChatPackets.cpp`,
`Channel.{h,cpp}` (`Make*`), `ChannelHandler.cpp`, `WorldSession.cpp`, `Creature.cpp`,
`MotdMgr.cpp`, `MiscPackets.cpp`. No client-only behaviour is inferred; no Ghidra round. Codecs:
`wrath_chat.rs` (existing lossless chat codec, now with `shared_message`), `wrath_chat_notices.rs`.
Agreed with App Migration (2026-10-16).

**Received chat** (0096, GM 03B3): every line stays in `ui_native_chat`'s `NativeChatPackets` as
raw bytes with sender GUID and chat type (unchanged). A line whose 3.3.5 `ChatMsg` has a 1.12
counterpart is also emitted as the shared `SessionEvent::Chat` the chat feed prints: types are
renumbered (system 0x00→0x0A, say 0x01→0x00, …, channel 0x11→0x0E, raid warning 0x28→0x58,
BG 0x2C→0x5C; party leader joins party), the tag bits (1 AFK, 2 DND, 4 GM) become 1.12's single
tag, strings are lossy UTF-8. **`LANG_ADDON` lines never become shared events**; addon delivery is
App Migration's consumer of the raw FIFO. Line kinds 1.12 lacks (monster party, skill/loot/money/
combat lanes, restricted, Battle.net, achievements, arena points) stay raw only.

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
