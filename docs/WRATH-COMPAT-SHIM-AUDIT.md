# `wrath_compat.lua` shim audit (VANILLA-REMOVAL-PLAN step 8)

Every global `crates/benilla-ui/src/script/wrath_compat.lua` defined on 2026-10-19, checked against
build 12340's `Wow.exe` Lua binding table (Ghidra 12.0.4 headless, read-only;
`/tmp/azb-app-mig/ghidra/{shim1,shim2,opt1-5,audit1,audit2}.log`) and the stock 3.3.5 FrameXML.
Addresses are the bound C functions.

Verdicts:

- **native now**: a Rust binding replaces the shim; the shim is deleted.
- **dead**: a native binding already wins (installed after the shim, or the shim is
  `X = X or …`); the line is deleted.
- **absent**: no binding in `Wow.exe` and no caller in stock 3.3.5 Lua or in benilla; deleted.
- **FrameXML**: stock FrameXML defines it; goes with the file.
- **real**: a real build-12340 binding still served by the shim; each needs a native binding
  (the suggested owner in brackets), or a recorded no-op where the client has no such service.

## Native now (45)

| Global | Address | Native behaviour |
|---|---|---|
| `GetQuestResetTime` | `0x5e6de0` | the daily reset (`0xc23aec`) less `time(NULL)`; stamped from `SMSG_QUERY_TIME_RESPONSE` |
| `GetMirrorTimerInfo` | `0x51cc10` | slot `i-1` of the 3-slot table `0xbd0b80`: name, value, max, scale, paused, label |
| `GetMirrorTimerProgress` | `0x517aa0` | `(now - start) * scale + value` for the named slot |
| `IsPVPTimerRunning` | `0x51bd60` | `PLAYER_FLAGS & 0x40000` |
| `GetPVPTimer` | `0x51bd00` → `0x6de7a0` | 301000 with the flag clear, ms left + 1000 running, 999 past the end |
| `GetInstanceInfo` | `0x51a8c0` | `Map.dbc` name/type/`Flags & 0x100`, difficulty + 1, `MapDifficulty` string and max players, dynamic difficulty |
| `IsModifiedClick` | `0x55fcc0` → `0x55f940` | any modifier / modifier mask / an action's chord (binding set 3) |
| `GetModifiedClick`, `SetModifiedClick` | `0x55fc20`, `0x55fb90` | an action's chord; unknown action or binding raises |
| `GetModifiedClickAction`, `GetNumModifiedClickActions` | `0x55ea70`, `0x55dc60` | the action list, from `Bindings.xml` `<ModifiedClick>` |
| `SecureCmdOptionParse` | `0x564ae0` → `0x5f0df0` | the macro option grammar over the 32 conditions at `0xad14d8` |
| `GetMaxDailyQuests` | `0x58c470` | 25 (`0xa13578`) |
| `GetMultiCastBarOffset` | `0x5b71e0` | 6 (`0xa0b708`); the shim answered 0 |
| `GetNumMapDebugObjects` | `0x608560` | 0 |
| `HasDebugZoneMap` | `0x8e5250` | no results |
| `GetQuestWorldMapAreaID` | `0x5e56c0` → `0x5e2b60` | the marked POI blob on the shown map and floor, else the shown continent's nearest floor then nearest the player, else the first; no blob: the header's top zone's `WorldMapArea` row, floor 0 |
| `GetQuestLogSpellLink`, `GetQuestSpellLink` | `0x5e4840`, `0x58dab0` | the selected quest-log quest's, or the quest panel's (`0xc0d680`), reward spell as a `spell:` link (`0x61e4b0`); nil without one |
| `UnitIsPossessed` | `0x60d860` | `UNIT_FIELD_FLAGS & 0x1000000`, 1 or nil |
| `IsStackableAction` | `0x5a8720` → `0x5a8470` | 1 when action slot 1-144 holds an item whose template stacks past one, else nil (labelled: template `+0x5c` taken as the stack size) |
| `SpellCanTargetItem`, `SpellTargetItem` | `0x7fdd10`, `0x8007e0` | 1 while the targeting spell takes an item (`& 0x4010`); while armed, the first carried slot holding the item (id, name or link) binds as a bag click (labelled: worn items and the `0x5191c0(2)` gate) |
| `EquipItemByName` | `0x51cdb0` | the first carried unlocked slot holding the item auto-equips, or moves to a slot (1-19 or an `INVSLOT` name); a bad slot raises |
| `SetMultiCastSpell` | `0x5ab8a0` | totem-bar slots 133-144 take a spell action, or spell 0 clears, through the `PlaceAction` queue (labelled: the totem check `0x5a7bc0` and its message `0x299`) |
| `PickupItem` | `0x522fb0` | an item by id, link or cached name onto the cursor as mode 7 (`GetCursorInfo` "item", id, link; `PlaceAction` puts it on a bar) |
| `PickupEquipmentSet` | `0x5af2c0` → `0x520dc0` | the index-th set onto the cursor as mode 0xd (`GetCursorInfo` "equipmentset", name); labelled: `PlaceAction` keeps it held, the set action word being untraced |
| `ClearOverrideBindings`, `SetOverrideBinding`, `SetOverrideBindingClick`, `SetOverrideBindingItem`, `SetOverrideBindingMacro`, `SetOverrideBindingSpell` | `0x560560`, `0x563a90`, `0x563f10`, `0x563cb0`, `0x563de0`, `0x563b80` | an owner frame's key layer over the live table (priority over non-priority over normal), never saved; the command variants write `CLICK button:mouse`, `ITEM`, `MACRO` and `SPELL` forms, which a press runs through the matching verb (labelled: the priority gate `0x5191c0(0xd)` and the native form runner) |
| `UnitSelectionColor` | `0x60da00` → `0x521bf0` | r, g, b, a: a player-controlled unit red while it can be attacked, its reaction colour while PvP-flagged, else blue; other units their reaction colour (`0xbd0c94`); labelled: `can_attack` answers both directions, folding the one-way yellow and blue, and the sanctuary terms |
| `GetNumFriends` | `0x6b4060` | the friend count and how many are online |
| `FillLocalizedClassList` | `0x60a510` (bound at `0xad2718`, outside the main table) | every `ChrClasses` file name keyed to its male or female name, falling back as `0x7159e0` does; the VM is seeded before `Constants.lua` loads |
| `GetActionBarPage`, `ChangeActionBarPage` | `0x5a7fd0`, `0x5a7f60` | the page engine-side (1-6, an error out of range, `ACTIONBAR_PAGE_CHANGED` on a change); labelled: `0xc1e5a0` and the `0x5191c0(0xe)` gate |
| `IsPetAttackAction` | `0x5d5a10` | a boolean: the pet slot holds the Attack command (labelled: read as its `PET_ACTION_ATTACK` token) |
| `GetCVarBool` | `0x5100c0` | 1 for an existing CVar whose value reads non-zero, else nil (labelled: flag `0x40`) |
| `FocusUnit`, `ClearFocus` | `0x522480`, `0x5225e0` | the unit a token names, or none, into the focus guid (`0x51ff20`): the `"focus"` token and `PLAYER_FOCUS_CHANGED` (event `0x9e`); labelled: the setter's protected-state refusal and its `0x2000` render flag |
| `IsPossessBarVisible` | `0x5a8820` | a boolean: the possess spell `GetPossessInfo` reads is present (labelled: its `AttributesEx7 & 0x1000`, attribute `0xec`, is not loaded) |
| `GetLFGDeserterExpiration`, `GetLFGRandomCooldownExpiration` | `0x5580e0`, `0x558060` | the player's Dungeon Deserter (71041) or Dungeon Cooldown (71328) aura's expiry in `GetTime` seconds; nothing without it |

Unresolved, labelled in code: the writers of the reset, the mirror table and the PvP end; the chord
matcher `0x55d6b0`; the condition compiler `0x5f0040` and its handlers (each asks the native Lua
reader of the same state); `GetQuestWorldMapAreaID`'s blob point (taken as the centroid), header-zone
row order (lowest id) and its pre-pass `0x547170(0)`.

## Dead lines deleted (18)

`CanShowAchievementUI`, `GetActionBarToggles`, `GetBonusBarOffset`, `GetDungeonDifficulty`,
`GetRaidDifficulty`, `GetReadyCheckStatus`, `HasCompletedAnyAchievement`, `QueryGuildEventLog`,
`SaveBindings`, `UnitDetailedThreatSituation`, `UnitThreatSituation`, `UnitGroupRolesAssigned`,
`debuginfo`, and on 2026-10-20 `GetAchievementInfo`, `GetTrackedAchievements`,
`GetNumTrackedAchievements`, `GetTotalAchievementPoints`, `GetPossessInfo`. (`IsStackableAction` was not dead:
its line aliased the native `IsConsumableAction`, which the served-list census read as native;
it is native now, and `the_compat_chunk_aliases_no_other_native` bans the alias pattern.)

`debuginfo` was deleted as dead with no native to replace it on Wrath; the `debug*` family is now
the Wrath stdlib's (`0x8e5250`, 13672e61). That gap is why the list below is test-enforced.

## Absent, deleted (3)

`GetCurrentCombatTextEventInfo`, `GetNumWorldPVPAreas`, `MovieRecording_IsSupported`: no binding
string in `Wow.exe`, no caller in stock `Blizzard_CombatText.lua`, `WorldStateFrame.lua`,
`WorldMapFrame.lua`, or anywhere in benilla.

## FrameXML

The `COMBATLOG_OBJECT_*` block (masks, `RAIDTARGET1..8`): stock `Constants.lua` defines all 33.
It goes with the file.

## Still on the shim (45), 2026-10-20

Enforced by `wrath_surface::tests::the_compat_chunk_serves_exactly_the_listed_globals`, which lists
every global function the compat chunk still serves in a live Wrath VM. A native binding installed
after the chunk drops its name there; a line a native binding already replaces is dead and goes.

| Family | Globals | Owner |
|---|---|---|
| Session, client, tutorial | `GetAccountExpansionLevel`, `GetExpansionLevel`, `GetExistingLocales`, `GetCVarMin`, `GetCVarMax`, `GetCVarAbsoluteMin`, `GetCVarAbsoluteMax`, `RegisterStaticConstants`, `TriggerTutorial`, `CombatTextSetActiveUnit` | App Migration (moved from Native UI) |
| Offline services, the intended end state | the 16 `BN*` rows, `IsVoiceChatEnabled`, `IsVoiceChatAllowed`, `IsVoiceChatAllowedByServer`, `VoiceIsDisabledByClient`, `GetNumVoiceSessions`, `GetVoiceCurrentSessionID`, `GetVoiceSessionInfo`, `GetVoiceStatus`, `GetActiveVoiceChannel`, `GetNumVoiceSessionMembersBySessionID`, `UnitIsTalking`, the six `Sound_*` driver rows, `IsStereoVideoAvailable`, `IsPlayerResolutionAvailable` | — |

Natives already landed since the 2026-10-16 audit: the App Migration quest, unit, item and area rows
(23a38832, 3b00e835, 4d80da91, b05eeb9b), Wire A's world map (f0fe2fbe, b37c0919), Native UI's chat-window geometry (2c5c9581), and the rows
in "Native now" above. The 10-16 per-row evidence (addresses and native behaviour) is in git history.
