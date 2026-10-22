# Session metadata

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `WorldSession.cpp`
(`SendTutorialsData`, `SendAccountDataTimes`, `SendAddonsInfo`), `MiscHandler.cpp`
(`HandleUpdateAccountData`, `HandleRequestAccountData`, `HandleAreaTriggerOpcode`,
`HandleZoneUpdateOpcode`, `HandleSetActionBarToggles`), `CharacterHandler.cpp` (tutorial handlers),
`CharacterPackets.cpp` (`PlayedTime`), `MiscPackets.cpp` (`LevelUpInfo`, `Weather`), `Player.cpp`
(`SendCinematicStart`). No client-only behaviour is inferred; no Ghidra round. Codec:
`crates/benilla-protocol/src/wrath_session_meta.rs`.

**Shared (handled)**: 00FD tutorial flags (32 bytes; the login bootstrap's queued copy now reaches
the tutorial system), 00FA cinematic, 01CD played time (the echoed trigger byte dropped), 01D4
level-up (this build sends seven power deltas; runes and runic power are dropped), 02F4 weather
(id, intensity, abrupt; no sound id, so 0).

**Capture-only** (`NativeSessionMetaPackets`): 0209 account-data times (server time, raw byte, type
mask, one time per set bit; at most eight types), 020C account data (player, type, time, size and the
zlib blob kept compressed), 0463 account data saved.

**Outbound**: existing commands now native (producers `tutorial`, `ui_chat`, `cinematic`,
`area_trigger`, `ui_unit`): 00FE/00FF/0100 tutorial flag/clear/reset, 01CC played time (trigger 1),
00FB/00FC cinematic camera/complete, 00B4 area trigger (a teleport it causes is the movement
authority's), 02BF action-bar toggles. Transport-only `ClientCommand::NativeSessionMeta`: 01F4 zone
update, 020A request account data, 020B update account data (caller-compressed), 04FF ready for
account-data times, 0374 set title (`SetTitle`: an int32 `CharTitles` bit index, -1 clears; App
wires Native UI's title verb).

Not actionable: 02EF addon info (not self-describing, so it is decoded against the 23 stock
secure addons our auth block names; Connected carries the ones it marked secure), 00F9 opening cinematic and 0154/0156 death bind
point (AC `Handle_NULL`).
