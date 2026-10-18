# Quest remainders

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `QueryHandler.cpp`
(`HandleQuestPOIQuery`), `PlayerQuest.cpp` (`SendQuestUpdateAddPlayer`), `QuestPackets.cpp`,
`QuestHandler.cpp`. No Ghidra round; the PvP leaderboard line's client placement is labelled below. Codec:
`crates/benilla-protocol/src/wrath_quest.rs`. The core quest flow (giver, details, rewards, log
updates, status-multiple replies) was already handled and is unchanged.

**POIs** (`A/net/wrath_quest_pois.rs`, sole owner of the quest receipts, `QuestPois` cleared on
Connected and Disconnected). As the reference's quest-log rebuild (`0x5e6940`, from Native UI's
trace) does, every logged quest without POI data is asked once with 01E3, at most 25 ids a packet.
An empty answer counts as data and is not asked again. Each 01E4 (per quest: POIs with objective,
map, area, floor, two raw words and points; counts bounded by the body) merges into the cache. The
logged quests' POIs, in log order, go to `set_wrath_quest_pois` whole, and `QUEST_POI_UPDATE`
(event `0x297`) fires, which WorldMapFrame and WatchFrame redraw on. Labelled: the event fires
once per response. Its native fire site is untraced: the only quest-module `push 0x297` is at
`0x5e7628`, unverified against the 01E4 handler that `0x54cd1d` registers. 0501 completed-quests reply keeps its existing
capture (`wrath_quests_completed_capture`); its row was stale.

**Special items.** `A/ui_quest_log.rs` `feed_quest_special_items` publishes each logged quest's
usable item to `set_wrath_quest_special_items` as the Lua `(bag, slot)` of its first copy in the
backpack or the four bags. `GetQuestLogSpecialItemInfo`, `UseQuestLogSpecialItem` and their kin
read it. Labelled inferred: the reference picks the item per log slot in `0x5e0da0(logIndex-1)`,
which all four bindings call (`0x5e52d0`, `0x5e53d0`, `0x5e54c0`, `0x5e5640`); its body is not
decompiled, so this takes the template's `SrcItemId`.

**PvP kills.** 046F (quest, current, required) is consumed in the same receiver and re-asks the givers' markers, as 0199 `ADD_KILL` does. No
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
  0417 status-multiple query. 01E3 now has a producer (POIs above). 0500 completed-quests query keeps its existing writer; no producer.

Not actionable: 021E force remove (no AC sender), 0489 start quest (AC `Handle_NULL`).
