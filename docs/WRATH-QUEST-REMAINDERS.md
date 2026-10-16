# Quest remainders

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `QueryHandler.cpp`
(`HandleQuestPOIQuery`), `PlayerQuest.cpp` (`SendQuestUpdateAddPlayer`), `QuestPackets.cpp`,
`QuestHandler.cpp`. No client-only behaviour is inferred; no Ghidra round. Codec:
`crates/benilla-protocol/src/wrath_quest.rs`. The core quest flow (giver, details, rewards, log
updates, status-multiple replies) was already handled and is unchanged.

**Capture-only** (`NativeQuestPackets` FIFO, sole owner, cleared on Connected and Disconnected):
01E4 POI reply (per quest: POIs with objective, map, area, floor, two raw words and points;
counts bounded by the body), 046F PvP-kill credit (quest, current, required). The quest log's
counters come from the descriptor, so no counter is synthesised from 046F. 0501 completed-quests
reply keeps its existing capture (`wrath_quests_completed_capture`); its row was stale.

Outbound:
- 0276 quest push result: the existing `ClientCommand::QuestPushResult` now carries the quest
  (this build writes sharer, quest, message; AC ignores the quest). Producers in `ui_quest` (decline,
  busy, walk-away) fill it from the open panel, or 0 for a greeting.
- Transport-only `ClientCommand::NativeQuest`: 0187 auto-launch (AC does nothing), 0190 giver
  cancel, 0193 log swap (two slots), 01E3 POI query (at most 25 ids, refused before send otherwise),
  0417 status-multiple query. 0500 completed-quests query keeps its existing writer; no producer.

Not actionable: 021E force remove (no AC sender), 0489 start quest (AC `Handle_NULL`).
