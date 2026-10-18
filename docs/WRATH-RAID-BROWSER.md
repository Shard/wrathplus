# Wrath raid browser (build 12340)

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03` `Handlers/LFGHandler.cpp`
(`HandleLfrSearchJoinOpcode`, `HandleLfrSearchLeaveOpcode`, `SMSG_LFG_UPDATE_SEARCH`) and
`DungeonFinding/LFGMgr.cpp` (`RBPacketBuildFull`/`Difference`, `RBPacketAppendGroup`/`Player`,
`LFG_UPDATE_FLAG_*`). Codec: `P/wrath_raid_browser.rs`.

- 035E/035F search join/leave: a `uint32` dungeon (AC masks the type byte off the join), as
  `ClientCommand::NativeRaidBrowser(Join/Leave { dungeon })`; transport-only until Native UI's
  LFRFrame bindings queue them.
- 0369 searching flag (`uint8`) and 0360 list, decoded raw and flag-driven: type, dungeon, full or
  difference (a difference first lists removed GUIDs), groups (GUID, flags; comment, three role
  bytes, the bound instance and encounter mask when flagged) and players (GUID, flags; the
  character block — level, class, race, talents, armor, spell power, crits, mp5, attack power,
  agility, health, mana, the online marker, average item level, defense/dodge/block/parry/haste/
  expertise — comment, leader byte, group GUID, role byte, area, status, bound instance). Every
  optional field is set exactly when its flag bit is. They are capture-only
  (`NativeRaidBrowser` FIFO, `A/net/wrath_social_lists_capture.rs`) until Native UI publishes its
  raid-browser setter (after its own Wow.exe round on the SearchLFG* bindings).
