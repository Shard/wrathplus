# AzerothBench Wrath client plan

Target: AzerothCore WotLK 3.3.5a / build 12340, stock FrameXML, normal human play first, then the
agent bench (observation/control contract in `AZEROTHBENCH.md`).

## Principles

- **All UI code is trusted.** No taint, no protected frames, no secure-execution emulation:
  `issecure`/`securecall`/`hooksecurefunc` are pass-throughs and secure templates are ordinary
  frames. Agents are meant to automate the interface.
- **Load the real stock UI** (`FrameXML.toc`, the archive's `Blizzard_*` addons) and fix what it
  actually trips over. No allowlists, no bounded slices.
- **Missing data is not fatal.** An API with no backing system answers like a client with nothing
  in it; a command not yet encoded for 12340 is logged and dropped, never a session teardown.
- **Measure with the installed data.** `ui_script::wrath_framexml_tests` (ignored; run with
  `WOW_CLIENT_PROFILE=wrath WOW_DATA=…`) loads the installed UI the way the game does and prints
  every load error, missing method, global and engine constant. `AZB_FRAMEXML_DUMP=<dir>` writes
  the stock sources out for reading (never into the repo).

## Done (2026-10-13)

- Stock HUD renders in-world: player/target frames, minimap, action bars, micro menu, bags, chat.
- Trusted secure environment; frame attributes, `HookScript`, `OnAttributeChanged`.
- Loader: comma-separated `inherits`, `parentKey`, `function=""`, `SetPoint(point)`, Wrath
  handler formals, `Cooldown`/`QuestPOIFrame` kinds, ten chat windows, Wrath unit tokens.
- Wrath stdlib (`bit`, `newproxy`, string/table/math aliases), Wrath `UnitAura`, compat layer
  (`script/wrath_compat.lua`) for ~250 engine functions, stock CVars and engine constants.
- Archive `Blizzard_*` addons load on demand; addon-info reply recorded on the Wrath route.
- Layout: non-finite offsets read as 0; fixpoint capped at 100 rounds; a bailed solve no longer
  re-solves on every read. Wrath instruction budget counts instructions, not calls.
- The XP pair seeds into the VM before the entry load (stock `MainMenuExpBar` hides itself for good
  when it loads with max 0). Factions decode (`SMSG_INITIALIZE_FACTIONS`, `SET_FACTION_STANDING`).
- Casting: `UseAction(action, unit, button)` in its 3.3.5 shape (1.12 read `button` as self-cast,
  so every press targeted the player); `GetActionInfo`; a cast of ours starts its `Spell.dbc`
  recovery on success, since a 3.3.5 server sends `SMSG_SPELL_COOLDOWN` only for modified ones.
- Auras: `SMSG_AURA_UPDATE(_ALL)` map onto 1.12's aura descriptor block (`wrath_auras.rs`), our
  own with durations.
- Key bindings dispatch through the VM (`Bindings.xml` over `WTF\DefaultBindings.wtf`).
- DBCs: a 1.12 schema reads a 3.3.5 file whose only change is wider locale blocks automatically
  (`benilla-dbc`), and `benilla-formats/src/wrath_layouts.rs` maps the restructured tables
  (Spell, SkillLine/Ability, Talent/Tab, Faction, ChrRaces/Classes, AreaTable, SpellRange, …)
  from AzerothCore's `DBCStructure.h`. Checked on installed data by `wrath_layouts_installed_tests`.
  Still unmapped: AreaPOI, Cfg_Categories, ChatProfanity, CreatureSoundData, PetPersonality,
  SpellVisualKit.

## Next passes

1. **Wrath protocol pass.** The Wrath route encodes only movement, item query, melee, loot and
   logout; every other command is dropped. Needed for play: name/creature/GO queries (target
   names read "Unknown"), spell cast, action buttons and initial spells (empty action bars),
   quest giver/gossip/vendor/trainer, chat, group. Reuse the codecs on `park/secure-runtime`
   (`wrath_names`, `wrath_group`, `wrath_instance`, `wrath_threat`, `wrath_trade`).
2. **Unit data.** Power bars, casting info (`UnitCastingInfo`), threat, focus. Other units' aura
   durations and casters (target debuff timers, `PLAYER` filter). `GetSpellInfo` and the other
   2.0+ spell readers (the VM has no spell catalog). `SMSG_GAMEOBJECT_QUERY_RESPONSE` (0x5F).
   Wrath M2s load without their lights, ribbon and particle emitters and blend-map overrides
   (no candle flames, brazier fire or trails yet); a few still fail on a skin-batch section check
   (`generalweaponrack01`, the zeppelin). Melee spells: no player swing animation on a
   next-swing spell's `SPELL_GO`, and no damage/miss text for it (`SMSG_SPELLNONMELEEDAMAGELOG`,
   `SPELL_GO` misses), so a Heroic Strike swing reads as a dropped swing.
   Unhandled server messages seen in a melee session (2026-10-14), by frequency:
   `SMSG_GAMEOBJECT_QUERY_RESPONSE`, `SMSG_POWER_UPDATE`, `SMSG_CRITERIA_UPDATE`,
   `SMSG_UPDATE_WORLD_STATE`, `SMSG_SET_PROFICIENCY`, `SMSG_THREAT_UPDATE`/`HIGHEST_THREAT_UPDATE`/
   `THREAT_CLEAR`, `SMSG_TALENTS_INFO`, `SMSG_QUESTGIVER_STATUS_MULTIPLE`, `SMSG_LOOT_LIST`,
   `SMSG_PARTYKILLLOG`, `SMSG_LOG_XPGAIN`, `SMSG_SPELLNONMELEEDAMAGELOG` (spell damage in the
   combat log and floating text), spline hover/gravity toggles. `WOW_CAST_TRACE=1` lists them.
3. **Remaining DBCs** listed above.
4. **Remaining UI surface:** animations (`AnimationGroup`), cooldown sweep, `OnEnable`/`OnDisable`,
   `PreClick`/`PostClick`, `OnTooltipSet*`, quest POI blobs, chat dock order.
5. **Agent bench:** observation/control contract, receipts, episodes.

## Server issues to revisit (world fidelity)

Workarounds applied to the local dev realm; each needs a real fix so the world matches retail.

| Issue | Workaround | Restore |
|---|---|---|
| `worldserver` segfaults daily at 20:00 in `GameEventMgr::RunSmartAIScripts` starting game event 89 "Leprithus" (Westfall rare spawn) | `game_event.end_time` for 89 set to `2016-10-28 19:00:00` (2026-10-13) | original row in `azerothcore-playerbots/logs/game_event_89_before.tsv` (`end_time` `2030-12-30 23:00:00`) |
