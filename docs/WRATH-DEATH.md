# Death, corpse and summon

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `QueryHandler.cpp`
(`HandleCorpseQueryOpcode`, `HandleCorpseMapPositionQuery`), `Player.cpp` (`SendCorpseReclaimDelay`,
`SMSG_DEATH_RELEASE_LOC`, `SMSG_PRE_RESURRECT`), `SpellEffects.cpp` (`SMSG_SUMMON_REQUEST`),
`MiscHandler.cpp`, `MovementHandler.cpp` (`HandleSummonResponseOpcode`), `SpellHandler.cpp`
(`HandleSelfResOpcode`), `QueryPackets.cpp`. No client-only behaviour is inferred; no Ghidra round.
Codec: `crates/wrathplus-protocol/src/wrath_death.rs`. Resurrect request/response (015B/015C) are
Wire A's spell-control unit and unchanged.

**Shared (handled)**: 0216 corpse query (not found, or map, position, corpse map; AC's trailing zero
word is framed and dropped), 0269 corpse reclaim delay, 02AB summon request (summoner, zone,
auto-decline delay).

**Capture-only** (`NativeDeathPackets`, sole owner, cleared on Connected and Disconnected): 0378
release location (spirit healer map and position; map -1 removes it), 0494 pre-resurrect (packed
GUID), 04B7 corpse map position (AC answers four zero floats).

**Outbound**: the existing commands route natively (producers `death/`, `ui_summon`): 015A repop
(AC skips one byte), 0216 corpse query, 01D2 reclaim corpse, 02AC summon response (the shared
command is the accept, sent with agree 1; a decline sends nothing), 02B3 self-res. Transport-only
`ClientCommand::NativeDeath`: 049C hearth-and-resurrect, 04B6 corpse map position query. A
resurrect that teleports goes through Wire A's movement authority.

The release location (0378) now feeds the world map: `set_death_release_location((map, x, y))`,
projected by `GetDeathReleasePosition()` (WorldMapFrame.lua:908, previously undefined) through the
same world-to-map projection as `GetWorldLocMapPosition`; map -1 clears it, as do Connected and
Disconnected. 0494 pre-resurrect and 04B7 corpse map position (AC answers zeros) stay captured.
