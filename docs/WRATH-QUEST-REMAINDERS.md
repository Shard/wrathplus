# Quest remainders

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `QueryHandler.cpp`
(`HandleQuestPOIQuery`), `PlayerQuest.cpp` (`SendQuestUpdateAddPlayer`), `QuestPackets.cpp`,
`QuestHandler.cpp`. No Ghidra round; the PvP leaderboard line's client placement is labelled below. Codec:
`crates/benilla-protocol/src/wrath_quest.rs`. The core quest flow (giver, details, rewards, log
updates, status-multiple replies) was already handled and is unchanged.

**Capture-only** (`NativeQuestPackets` FIFO, sole owner, cleared on Connected and Disconnected):
01E4 POI reply (per quest: POIs with objective, map, area, floor, two raw words and points;
counts bounded by the body). 0501 completed-quests reply keeps its existing capture
(`wrath_quests_completed_capture`); its row was stale.

**PvP kills.** 046F (quest, current, required) leaves the FIFO in its receiver
(`A/net/wrath_quest_capture.rs`) and re-asks the givers' markers, as 0199 `ADD_KILL` does. No
counter is synthesised from it. The quest template (005D) now keeps `RequiredPlayerKills`
(`WrathQuestInfo::players_slain`, `QuestDef.cpp:365`), and the quest log (`A/ui_quest_log.rs`
`player_line`) adds a `QUEST_PLAYERS_KILLED` "Players slain: %d/%d" leaderboard line. It reads
slot counter 0, which is `QUEST_PVP_KILL_SLOT` (`QuestDef.h:43`) and the one the server bumps
beside 046F (`PlayerQuest.cpp:2563`). The log diff's progress toast then reads as
`ERR_QUEST_ADD_PLAYER_KILL_SII`. Unverified against the client: the line's place (last) and its
`type` string `"player"`, which no stock frame reads.

Outbound:
- 0276 quest push result: the existing `ClientCommand::QuestPushResult` now carries the quest
  (this build writes sharer, quest, message; AC ignores the quest). Producers in `ui_quest` (decline,
  busy, walk-away) fill it from the open panel, or 0 for a greeting.
- Transport-only `ClientCommand::NativeQuest`: 0187 auto-launch (AC does nothing), 0190 giver
  cancel, 0193 log swap (two slots), 01E3 POI query (at most 25 ids, refused before send otherwise),
  0417 status-multiple query. 0500 completed-quests query keeps its existing writer; no producer.

Not actionable: 021E force remove (no AC sender), 0489 start quest (AC `Handle_NULL`).
