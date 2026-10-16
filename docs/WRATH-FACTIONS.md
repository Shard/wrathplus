# Faction remainders

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `ReputationMgr.cpp`
(`SendForceReactions`, `SendVisible`) and `CharacterHandler.cpp` (`HandleSetFactionAtWar`,
`HandleSetFactionInactiveOpcode`, `HandleSetWatchedFactionOpcode`). No client-only behaviour is
inferred; no Ghidra round. Codec: `crates/benilla-protocol/src/wrath_faction.rs`. The initial faction
list (0122) and standing updates (0124) keep their existing decoders.

**Shared (handled)**: 0123 faction visible (the reputation list id) → `ReputationVisible`.
**Capture-only** (`NativeFactionPackets`): 02A5 forced reactions (Faction.dbc id, rank); no unit
colour or hostility override is applied.
**Outbound** (producer `ui_reputation`): 0125 at war (list id, flag), 0317 inactive (list id, flag),
0318 watched faction (list id, -1 none).

Not actionable: 0313 `SMSG_SET_FACTION_ATWAR` (registry only), 0126 faction cheat (GM).
