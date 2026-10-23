# Instance control family

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`.
`Server/Packets/InstancePackets.{h,cpp}` establishes direction-specific difficulty,
reset and lock response layouts. `Player.cpp`, `PlayerStorage.cpp`, `Map.cpp`,
`MapMgr.cpp`, `CalendarHandler.cpp` and `InstanceScript.{h,cpp}` establish remaining
emitted shapes. No client-only behavior is inferred; no Ghidra round required.

Receive opcodes: 0147 lock warning, 0214 encounter, 02CB save created, 02FA raid
warning/welcome, 031E reset, 031F reset failed, 0320 last instance, 0329 dungeon
difficulty, 032B ownership, 033B instance difficulty, 0396 reset notify, 04EB raid
difficulty, 0506 corpse outside instance. Existing 02CC saved-instance snapshot
and 02CD request remain under their existing owner.

All new receive rows are **capture-only**: ordered typed `NativeInstanceControlPackets`
FIFO, sole registered owner, cleared on Connected and Disconnected. No timers,
lockouts, group authority, Lua events, difficulty changes or UI state are fabricated.
Raw flags/reasons/unknown fields and duplicate receipts are preserved. Encounter
packed GUIDs are decoded losslessly as GUID values; raw parameter bytes remain raw.
Strict framing validates AC emitted shapes (including welcome kind 4's two flags).

**Since then (2026-10-23 audit):** `ui_instance::take_native` consumes 0147 lock warning and 02FA
raid instance message among others; their coverage rows say handled. The difficulty replies 0329
and 04EB go to `ui_native::wrath_difficulty` (12340 `0x526530`): own and group stores per kind,
the group's answering while grouped, a "difficulty set to" chat line when the answer moves. It now
owns `GetDungeonDifficulty`/`GetRaidDifficulty`, solo included; the group list no longer sets them.

Outbound typed commands: 013F lock response (u8), 0292 saved-instance extension
(u32 map/u32 difficulty/u8), 031D reset (empty), 0329/04EB difficulty (u32 mode).
Routes require the ordinary live epoch-bound writer; they do not assert leadership,
permission or server acceptance. No production UI initiator is claimed.

Out of scope registry entries: 01FD/020E obsolete change-player-difficulty,
0415 expire-raid-instance (AC Handle_NULL/STATUS_NEVER), and GM reset limits.
These are not advertised as implemented. Calendar family owns calendar raid-lockout
notifications. Coverage-table statuses must not mistake transport for visible UI.
