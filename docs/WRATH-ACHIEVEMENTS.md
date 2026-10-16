# Achievement family

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `AchievementMgr.cpp`
(`CompletedAchievement`, `SendAllAchievementData`, `SendRespondInspectAchievements`,
`BuildAllDataPacket`, `Reset`) and `MiscHandler.cpp` (`HandleQueryInspectAchievements`). No
client-only behaviour is inferred; no Ghidra round. Codec: `crates/benilla-protocol/src/wrath_achievement.rs`.

All receive rows are **capture-only** (`NativeAchievementPackets` FIFO, sole owner, cleared on
Connected and Disconnected, duplicates kept): 0468 earned (packed player GUID, id, packed date, raw
word), 0498 server first (name, GUID, id, raw link word), 047D login snapshot and 046C inspect
snapshot (packed GUID first), 049F achievement deleted, 049E criteria deleted. Snapshots are the
earned `(id, packed date)` list and the criteria rows (`SMSG_CRITERIA_UPDATE`'s row layout), each
ended by `int32(-1)`; raw flags and counters are kept. No achievement state, points, toasts,
guild/chat lines or Lua events are produced: there is no shared event or 3.3.5 achievement API
consumer yet (WS3/WS2 own those).

`SMSG_CRITERIA_UPDATE` (046A) keeps its existing `wrath_progress` decoder and retained-progress owner.

Outbound: 046B inspect query (packed GUID) as transport-only `ClientCommand::NativeInspectAchievements`.

Not in scope: 046E/0470/04C4/04C5/04C6 cheat and GM rows, 04A2 check login criteria (AC
`Handle_NULL`).
