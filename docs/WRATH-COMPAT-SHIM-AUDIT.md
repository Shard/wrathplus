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

## Native now (16)

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

Unresolved, labelled in code: the writers of the reset, the mirror table and the PvP end; the chord
matcher `0x55d6b0`; the condition compiler `0x5f0040` and its handlers (each asks the native Lua
reader of the same state).

## Dead lines deleted (13)

`CanShowAchievementUI`, `GetActionBarToggles`, `GetBonusBarOffset`, `GetDungeonDifficulty`,
`GetRaidDifficulty`, `GetReadyCheckStatus`, `HasCompletedAnyAchievement`, `QueryGuildEventLog`,
`SaveBindings`, `UnitDetailedThreatSituation`, `UnitThreatSituation`, `UnitGroupRolesAssigned`,
`debuginfo`.

## Absent, deleted (3)

`GetCurrentCombatTextEventInfo`, `GetNumWorldPVPAreas`, `MovieRecording_IsSupported`: no binding
string in `Wow.exe`, no caller in stock `Blizzard_CombatText.lua`, `WorldStateFrame.lua`,
`WorldMapFrame.lua`, or anywhere in benilla.

## FrameXML

The `COMBATLOG_OBJECT_*` block (masks, `RAIDTARGET1..8`): stock `Constants.lua` defines all 33.
It goes with the file.

## Real bindings still on the shim (61)

### Quests and gossip [App Migration]

| Global | Address | Native behaviour |
|---|---|---|
| `GetDailyQuestsCompleted` | `0x58db30` | counts the player's daily-quest descriptor array (`+0x1008` block, `+0x11b4`) |
| `GetNumQuestItemDrops`, `GetQuestLogItemDrop` | `0x5e4260`, `0x5e60d0` | the selected quest's item drops off the quest cache |
| `GetQuestLink` | `0x5e51d0` | a quest-log row's `quest:` link off the quest cache (`0x67de90`) |
| `GetNumQuestLogRewardFactions`, `GetQuestLogRewardFactionInfo`, `ProcessQuestLogRewardFactions` | `0x5df910`, `0x5df940`, `0x5df8a0` | the selected quest's reputation rewards (cache `+0x28b8`), staged at `0xc0173c`/`0xc01840` |
| `GetQuestSortIndex` | `0x5dfa10` | a quest-log row's sort index + 1 (`0x5deae0`) |
| `GetQuestWorldMapAreaID` | `0x5e56c0` | a quest's POI map area and floor (`0x5e2b60`) |
| `QuestFlagsPVP` | `0x58c3e0` | the detail quest's flags `& 0x2000`, gated on `0xb2f998` |
| `ForceGossip` | `0x58ab50` | whether the gossip NPC forces the gossip window (`0x715e10`) |
| `GetContainerItemQuestInfo` | `0x5d9400` | a bag item's quest-starter fields off its item template |

### Units and the player [App Migration]

| Global | Address | Native behaviour |
|---|---|---|
| `UnitIsAFK`, `UnitIsDND` | (not decompiled) | `PLAYER_FLAGS` AFK/DND; the shim answers nil |
| `IsXPUserDisabled` | `0x614140` | `PLAYER_FLAGS & 0x2000000` |
| `UnitIsTappedByAllThreatList` | `0x611230` | `UNIT_DYNAMIC_FLAGS & 0x80` |
| `UnitInBattleground` | `0x60caa0` | the unit's battleground raid index, when in a battleground |
| `UnitIsRaidOfficer` | `0x60c8a0` | the raid member's rank > 0 |
| `UnitIsSameServer` | `0x612670` | the unit's realm against ours |
| `PlayerIsPVPInactive` | `0x612e20` | the battleground's inactive mark for the unit |
| `CanSummonFriend`, `IsReferAFriendLinked`, `GetSummonFriendCooldown` | `0x51d2b0`, `0x51d1b0`, `0x511d30` | Recruit-a-Friend link, summon gate and cooldown |
| `GetAvailableRoles` | `0x5548f0` | the class's `ChrClasses` role bits |
| `IsThreatWarningEnabled` | `0x517d30` | the flag at `0xbd08c0` |
| `GetNumLanguages` | `0x500760` | the languages the player knows (`Languages.dbc` against skills) |

### Actions [Native UI]

`IsStackableAction` (`0x5a8720`), `IsPetAttackAction` (`0x5d5a10`), `GetPossessInfo` (`0x5d5820`).
`GetActionBarPage`/`ChangeActionBarPage`: the shim keeps the page in Lua and replaces the native
`ChangeActionBarPage` (`action.rs`, installed earlier), so the native one never runs. Retire both
together.

### World map [Experience or Native UI]

`InitWorldMapPing` (`0x545ff0`), `GetCurrentMapDungeonLevel` (`0x544fc0`),
`GetNumDungeonMapLevels` (`0x546290`), `SetDungeonMapLevel` (`0x547b80`),
`DungeonUsesTerrainMap` (`0x544c40`), `IsZoomOutAvailable` (`0x545050`): the `WorldMapArea` and
dungeon-map floor state (`0xaccf0c`/`0xaccf10`).

### Items and areas [App Migration]

`GetContainerNumFreeSlots` (`0x5d7590`, free slots and the bag family),
`IsEquippableItem` (`0x511150`), `CanHearthAndResurrectFromArea` (`0x54c810`, the current area's
`AreaTable` flag `0x8000000`).

### Session, client and tutorial [Native UI]

`GetAccountExpansionLevel`/`GetExpansionLevel` (`0x4ddc20`, the realm/account byte `+0x2f2d`),
`GetExistingLocales` (`0x511c80`, the installed locales of 12), `GetCVarBool` (`0x4ddd90`),
`RegisterStaticConstants` (`0x5ac320`, the `STATIC_CONSTANTS` table), `TriggerTutorial`
(`0x5308d0`, a tutorial index < 60), `CombatTextSetActiveUnit` (`0x74d580`, the combat-text unit
guid).

### LFG [App Migration, with the LFG L1 interface]

`IsLFGDungeonJoinable` (`0x55a040`, the dungeon's lock check `0x559c90`), `RefreshLFGList`
(`0x55d280`).

### Services the client has none of (recorded no-ops)

Battle.net, voice chat and sound-device enumeration have no backing in benilla. The native
answers with the service offline are the target: `BNGetNumFriends` (`0x534590`),
`BNGetMaxPlayersInConversation` (`0x537a00`, 12 when online), `BNGetNumConversationMembers`
(`0x536220`), `VoiceIsDisabledByClient` (`0x7dc910`, the flag `0xd37d18`), `GetNumVoiceSessions`
(`0x7db030`), `UnitIsTalking` (`0x7df0b0`), `Sound_ChatSystem_GetInputDriverNameByIndex`
(`0x985a50`), `Sound_ChatSystem_GetOutputDriverNameByIndex` (`0x985b10`),
`Sound_GameSystem_GetNumOutputDrivers` (`0x985c70`),
`Sound_GameSystem_GetOutputDriverNameByIndex` (`0x985ca0`), `IsStereoVideoAvailable`
(`0x54ef90`), `IsPlayerResolutionAvailable` (`0x54ebc0`, more than one display mode).

The file also carries non-global shims: frame methods and the action-bar page state. They are
outside this census and go with the file.
