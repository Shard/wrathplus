# Wrath raid browser (build 12340)

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03` `Handlers/LFGHandler.cpp`
(`HandleLfrSearchJoinOpcode`, `HandleLfrSearchLeaveOpcode`, `SMSG_LFG_UPDATE_SEARCH`) and
`DungeonFinding/LFGMgr.cpp` (`RBPacketBuildFull`/`Difference`, `RBPacketAppendGroup`/`Player`,
`LFG_UPDATE_FLAG_*`). Codec: `P/wrath_raid_browser.rs`.

- 035E/035F search join/leave: a `uint32` dungeon (AC masks the type byte off the join), as
  `ClientCommand::NativeRaidBrowser(Join/Leave { dungeon })`, from Native UI's
  `take_wrath_raid_browser_requests`.
- 0369 searching flag (`uint8`) and 0360 list, decoded raw and flag-driven: type, dungeon, full or
  difference (a difference first lists removed GUIDs), groups (GUID, flags; comment, three role
  bytes, the bound instance and encounter mask when flagged) and players (GUID, flags; the
  character block — level, class, race, talents, armor, spell power, crits, mp5, attack power,
  agility, health, mana, the online marker, average item level, defense/dodge/block/parry/haste/
  expertise — comment, leader byte, group GUID, role byte, area, status, bound instance). Every
  optional field is set exactly when its flag bit is. `A/net/wrath_raid_browser_feed.rs` applies
  them: a full list (or a new dungeon) replaces, a difference removes its GUIDs then merges. The
  view (groups with their members, leader first; players listed alone; names from the name cache,
  class from the class table, zone from AreaTable) goes to Native UI's `set_wrath_raid_browser`
  whole, then `UPDATE_LFG_LIST`; a late name republishes. The searching flag is dropped (the
  joined entry is the UI's). Labelled: no member shows ignored or a friend, `unknown_64` and
  `flag_8c` read 0, and the totals are the listed counts.
