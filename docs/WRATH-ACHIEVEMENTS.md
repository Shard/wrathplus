# Achievement family

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `AchievementMgr.cpp`
(`CompletedAchievement`, `SendAllAchievementData`, `SendRespondInspectAchievements`,
`BuildAllDataPacket`, `Reset`) and `MiscHandler.cpp` (`HandleQueryInspectAchievements`). Client-only behaviour of the consumer
below comes from one Ghidra round. Codec: `crates/wrathplus-protocol/src/wrath_achievement.rs`.

All receive rows are **capture-only** (`NativeAchievementPackets` FIFO, sole owner, cleared on
Connected and Disconnected, duplicates kept): 0468 earned (packed player GUID, id, packed date, raw
word), 0498 server first (name, GUID, id, raw link word), 047D login snapshot and 046C inspect
snapshot (packed GUID first), 049F achievement deleted, 049E criteria deleted. Snapshots are the
earned `(id, packed date)` list and the criteria rows (`SMSG_CRITERIA_UPDATE`'s row layout), each
ended by `int32(-1)`; raw flags and counters are kept. No achievement state, points, toasts,
guild/chat lines or Lua events are produced: there is no shared event or 3.3.5 achievement API
consumer yet (WS3/WS2 own those).

`SMSG_CRITERIA_UPDATE` (046A) keeps its existing `wrath_progress` decoder and retained-progress owner.

Outbound: 046B inspect query (packed GUID) as `ClientCommand::NativeInspectAchievements`; producer
`SetAchievementComparisonUnit`.

## Consumer (`A/net/wrath_achievements.rs`, Lua in `U/script/wrath_achievement_progress.rs`)

The player's own receipts leave the FIFO: 047D seeds the earned list and criteria counters, 046A
counters (retained by `wrath_notices`) override them, the player's own 0468 adds an earn and fires
`ACHIEVEMENT_EARNED(id)` (once per achievement), 049F/049E remove rows, and the asked player's 046C
becomes the comparison (`INSPECT_ACHIEVEMENT_READY`). A 0498 server first becomes an achievement
chat line, as the reference's handler (`0x50b010`, from Native UI's chat Ghidra log) builds it.
The template is `SERVER_FIRST_ACHIEVEMENT`, or `PLAYER_SERVER_FIRST_ACHIEVEMENT` when the link
word is set. It is queued on the native chat FIFO as chat type `0x30` with the name as sender and
the achievement id, so `CHAT_MSG_ACHIEVEMENT` fires with `$a` expanded. Labelled: the template's
`%s` are filled with the name. The handler hands the name to its text lookup `0x819d40`, and the
player variant's two `%s` would fail `ChatFrame`'s one-argument `format` otherwise, but that the
lookup fills them is inferred. Other players' 0468 stay capture-only. Changed counters fire `CRITERIA_UPDATE`. The static metadata (`ui_achievements.rs`)
now carries points, description, icon (`SpellIcon`), reward, faction, the series' previous step and
the categories.

The Lua bindings follow one Ghidra round over build 12340 (`GetCategoryList` `0x5b1390` ..
`RemoveTrackedAchievement` `0x5b48d0`, registration table `0xacfc70`): `GetCategoryList` and
`GetStatisticsCategoryList` split the categories by whether they sit under Statistics (category 1),
whose children report a root parent in `GetCategoryInfo`; `GetCategoryNumAchievements(id[, all])`
counts a series by its first unearned step, Feats of Strength (81) and flag-`0x800` rows only once
earned, and the compared player's earns; `GetAchievementInfo(category, index)` lists earned by both,
by the player, by the compared player, then the rest in file order, and returns the 11 values
(name, points, completed, 1-based month/day, year since 2000, description, flags, icon, reward);
`GetAchievementInfoFromCriteria` returns its seven; the earned list is newest first for
`GetLatestCompletedAchievements` (5); points, next/previous, comparison info/points/counts and
tracking (`TRACKED_ACHIEVEMENT_UPDATE(id, tracked)`) are as the client's.
`GetAchievementCriteriaInfo` now reports the counter and completion.

Labelled (untraced): "superseded" is a later earned step of the series (`0x5b37c0`); the completed
count's filter is "not a statistic" (`0x5b0d50`); `GetStatistic` text (`0x5b0ea0`) is the sum,
maximum, met-count or last counter by the achievement's flags, money-formatted for money criteria,
`--` while zero; the categories are ordered by their `order` column (the catalog does not keep file
order); the earned list sorts by packed date; the tracked list is not persisted and has no limit
of its own (the window enforces it); `INSPECT_ACHIEVEMENT_READY` carries the GUID string.

Not in scope: 046E/0470/04C4/04C5/04C6 cheat and GM rows, 04A2 check login criteria (AC
`Handle_NULL`).
