# Battlegrounds, arenas and the battlefield manager

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `BattlegroundMgr.cpp`,
`Battleground.cpp`, `Arena.cpp`, the zone `BuildObjectivesBlock`s, `BattleGroundHandler.cpp`,
`ArenaTeamHandler.cpp`, `ArenaTeam.cpp`, `Object.cpp`, `MiscHandler.cpp`, `Player.cpp`,
`NPCHandler.cpp`, `BattlefieldHandler.cpp`. No client-only behaviour is inferred; no Ghidra round.
Codec: `crates/wrathplus-protocol/src/wrath_battleground.rs`; arena-team leave/remove/disband/leader
added to `wrath_arena_team.rs`.

This build keys queues on the BattlemasterList bg type (1.12 used the map), and adds arena type,
level range, rated flag, random-battleground rewards, arena ratings and team names. Those replies
do not mean what the 1.12 shared events mean, so they are **capture-only** (`NativeBattlegroundPackets`,
sole owner, cleared on Connected and Disconnected): 02D4 status (cleared slot, queued, invited,
in progress), 023D list (with random rewards), 02E0 score log (battleground and arena rows,
per-battleground objectives, arena ratings/names), 02D6 honor inspect, 02E8 group joined (raw
result and AC's zero GUID), 02E9 positions, 0349/0357/0376/0377 arena results, events, errors and
inspect, 04C7 arena unit destroyed, 04DE/04E0/04E1/04E4/04E6 battlefield-manager invites, entry,
queue reply and eject. 04F7 world-state UI timer keeps its existing capture (stale row).

**Shared (handled)**, 1.12 shapes unchanged: 02EC/02ED player joined/left, 02E4 spirit-healer time,
028C PvP credit.

**Arena lines** (`A/ui_battlefield_arena_lines.rs`, drained from `ui_battlefield::NativeBattlegroundRest`
after Experience's `drain_native`): 0349 command result → its action's line on error 0
(`ERR_ARENA_TEAM_CREATE_S`, `_INVITE_SS`, `_QUIT_S`, `_FOUNDER_S`), else the error code's
`ERR_ARENA_TEAM_*`/`ERR_*_ARENA_TEAM*` line; 0357 event 3..8 → `ERR_ARENA_TEAM_JOIN_SS` ..
`_DISBANDED_S` with its strings as they arrived, and the team (the last string) pushed to
`ui_arena_team::ArenaRosterRefresh`; 0376 with a leading 0 → `ERR_ARENA_NO_TEAM_II` (the team type
twice). Each is a `UiErrorKeys` catalog line. Labelled: the client's arena handlers are untraced.
The tables mirror the guild command-result and event handlers, whose codes AC's arena enums copy.
A `_S` code takes the player name, else the team name. 04C7 stays in the rest FIFO, capture-only:
no arena-opponent model consumes it.

**Since then (2026-10-23 audit):** the FIFO is no longer terminal. `ui_battlefield::drain_native`
consumes 02D4, 023D, 02E0, 02E8, 02E9, 0377, 02D6 and the battlefield-manager family 04DE, 04E0,
04E1, 04E4, 04E6 into the queue, list, scoreboard, positions, inspect and `WorldPvp` tables; the
coverage rows say handled. Only 04C7 still ends in `NativeBattlegroundRest`.

**Outbound**:
- Existing commands whose 3.3.5 bodies are unchanged now route natively: 02D7 battlemaster hello,
  02E2/02E3 area spirit healer query/queue, 021C spirit healer, 02D6 honor inspect, 0253 toggle
  PvP (empty body toggles), 02E0 score request, 02E9 positions request, 02E1 leave (AC reads none
  of the body).
- Transport-only `ClientCommand::NativeBattleground` (no UI producer; the shared list/join/port
  commands carry 1.12 map ids, which this build does not take): 023C list, 02EE join, 02D5 port,
  0358 join arena, 03E4 report AFK, 0377 inspect arena teams, 04DF/04E2 battlefield invite
  responses, 04E7 battlefield exit. Arena team 0353..0356 via `ArenaTeamCommand`. 04F6 UI timer query
  keeps its existing writer (stale row).
- 02D3 status query stays a deferred no-op: AC sends status unprompted and it is captured.

Not actionable: no AC sender for 014B port denied, 0170 removed from queue, 0222 spirit-healer confirm,
038A joined queue, 03E5 report-AFK result, 04A6 info throttled, 04C8 change failed, 04DC queue stats,
04E5 eject pending, 04E8 state change; AC `Handle_NULL` for 023E, 028B, 0348, 04E3; GM/cheat rating rows.

02D3 `CMSG_BATTLEFIELD_STATUS` (empty; AC answers with every queue's status) is
`NativeBattleground(Status)`, sent by the world-enter `BattlefieldStatusRequest` (12340 `0x54ca50`,
Experience's trace). 04E3 `CMSG_BATTLEFIELD_MGR_QUEUE_REQUEST` (u32 battle id, `0x54d590`) is
`NativeBattleground(BfQueueRequest)` for Experience's battlefield reducer; AC registers it
`STATUS_NEVER`/`Handle_NULL`, so the server ignores it.
