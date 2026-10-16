# Inspect and talents

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `MiscHandler.cpp`
(`HandleInspectOpcode`), `Player.cpp` (`BuildPlayerTalentsInfoData`, `BuildEnchantmentsInfoData`,
`SendTalentWipeConfirm`), `SkillHandler.cpp` (`HandleLearnTalentOpcode`, `HandleLearnPreviewTalents`,
`HandleTalentWipeConfirmOpcode`). No client-only behaviour is inferred; no Ghidra round. Codec:
`crates/benilla-protocol/src/wrath_inspect.rs`. The player's own talents (`SMSG_TALENTS_INFO`)
keep `wrath_progress`.

**Shared (handled)**: 02AA talent-wipe confirm (trainer, cost; AC's zero pair is "no talents").

**Capture-only** (`NativeInspectPackets`, sole owner, cleared on Connected and Disconnected): 03F4
inspect reply: the packed GUID, unspent points, active group, up to two groups of (talent, rank)
and glyph ids (zero groups when the realm disables talent inspection), then the equipped items by
slot mask with entry, per-slot enchant ids, random property, packed creator and suffix factor.

**Outbound**: the existing commands now native (producers `ui_inspect`, `ui_talent`,
`ui_talent_wipe`): 0114 inspect, 0251 learn talent, 02AA wipe confirm. Transport-only
`ClientCommand::NativeInspect`: 04C1 learn preview talents. Not actionable: 0213 unlearn talents
(AC `Handle_NULL`).
