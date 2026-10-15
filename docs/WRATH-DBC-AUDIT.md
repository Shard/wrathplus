# Mapped Wrath DBC audit inventory

Snapshot: `native/assets-dbc-audit`, including the TaxiPathNode known-row audit (2026-10-15).
Scope is the 29 table entries in `src/wrath_layouts.rs` in `benilla-formats`,
not every table accepted by the generic DBC parser. This is a coverage inventory,
**not certification of every column or native runtime behavior**.

Installed reference: read-only effective Wrath chain, enUS, build-12340 layouts.
An effective installed chain may include later expansion archives; no claim that
this is an untouched stock 3.3.5 installation is intended. No proprietary fixture
is checked in. Run installed tests with `WOW_CLIENT_PROFILE=wrath WOW_LOCALE=enUS
WOW_DATA=/Users/shannon/Games/Client/Data`; ordinary library gates unset the profile
and locale and set `WOW_DATA=`. A passing skipped installed test is not evidence.

## Explicit installed Wrath known-row assertions

Paths below are relative to `crates/benilla-formats`.

| Mapped tables | Evidence | Boundary |
| --- | --- | --- |
| AreaPOI | `tests/wrath_remaining_dbc.rs`: Aerie Peak 792, position/icon/name/faction/area/flags | Selected fields only |
| CreatureSoundData, CreatureModelData | Same: model 1 voice join, death/fidgets/custom attacks | Does not validate every model-data column; only four fidgets exposed |
| PetPersonality | Same: thresholds, damage multipliers, removed loyalty zeroes | Wrath loyalty intentionally absent |
| Spell | `src/wrath_layouts_installed_tests.rs`: Fireball 133 name/rank/icon/school/family/effect dice and Pick Lock 1804 | Not all 234 columns |
| SkillLine, SkillLineAbility | Same: Blacksmithing 164, Frost 6, ability 2018 and successor 3100 | Selected ordinary rows |
| TalentTab, Talent | Same: mage tab order/masks/background, Impact 27 ranks/grid/prerequisites | Does not certify pet talent handling |
| CreatureFamily | `src/wrath_layouts_installed_tests.rs`: Wolf 1 and Imp 23 names/icons/diet/scale | Selected ordinary pet families |
| SpellShapeshiftForm | Same: Cat Form name/bar/flags/type/icon | Only fields exposed by existing reader |
| Faction | Same: Stormwind 72 localized name/description, rep index/team/base/flags | Human/wolf template reaction also asserted; no runtime PvP/reputation override claim |
| ChrClasses | Same: all ten class family/pet-token/relic results, selected damage values | See confirmed fix below |
| ChrRaces, CreatureDisplayInfo | `tests/wrath_render_dbc.rs`: Human male display/model path join | Indirect selected fields; not full race/display layout coverage |
| AreaTable | `src/area_table.rs::tests::installed_wrath_area_table_schema_loads`: Elwynn/Northshire names and parent | Despite its name this is more than a parse smoke test |
| SpellVisual, SpellVisualKit, SpellVisualEffectName | `tests/wrath_spell_visuals.rs`: common spell stage/attachment/effect/path routes | Weak kit-count smoke in mapped-layout tests is not the primary evidence |
| SpellDispelType | `src/wrath_layouts_installed_tests.rs`: four native aura tokens, withheld combined category and empty Frenzy token | `1a355d64` fixes ImmunityPossible column 19 and uses InternalName column 20; Vanilla unchanged |
| TaxiPathNode | Same: Stormwind/Sentinel Hill routes 6/7, transport routes 285/302, exact endpoints/order/map/stop flags/delay | Arrival/departure event columns remain unexposed; not a live-flight certification |

Character geoset tables are not entries in `wrath_layouts.rs`; their dedicated
Wrath readers are additionally checked in `tests/wrath_render_dbc.rs` using exact
Human male hair-style 1 / facial-style 1 selection. This does not prove complete
rendering, equipment or helmet behavior. Character skin and starting-item checks
in that test still establish existence/joins rather than exact all-field parity.

## Remaining explicit Wrath known-row gaps

These mapped entries have no dedicated explicit installed-Wrath known-row
assertions identified by this audit. Existing `real_*` tests using the shared
`wow_data_or_skip!` helper and synthetic map/algorithm tests are useful but were
not run as a complete Wrath-installed suite here. Do not count a vanilla-derived
expected row, generic parse success, or an in-range/injective map as certification.

| Table | Existing reader/test location to extend |
| --- | --- |
| SpellItemEnchantment | Existing enchantment reader and its real-data tests |
| WorldMapArea | `src/world_map_area.rs` |
| WorldMapContinent | `src/world_map_continent.rs` |
| SpellRange | `src/spells/ranges.rs` |
| AnimationData | `src/anim_data.rs` |
| SoundEntries | `src/sound_entries.rs` |
| Material | `src/material.rs` |
| GroundEffectTexture | `src/ground_effects.rs` |

Next bounded units should independently inspect native rows and layout sources,
then assert values through the production reader; do not simply copy current
reader output as the specification. Keep each confirmed layout fix separate.

## Confirmed class-flags fix

`a47f08d8` changes only Wrath relic interpretation from nonzero flags to `flags &
0x8 != 0`; existing Vanilla behavior remains unchanged. WoWDBDefs build-12340
`ChrClasses` names file column 57 `Flags`. Native `UnitHasRelicSlot` at `0x611330`
(binding string `0xa1f190`, binding table reference `0xad0440`) resolves the class
row through `0xad3424`, then tests bit `0x8` at row offset `0x24` (`0x6113a9`).
Other flags made Warrior/Hunter/Warlock falsely report relics before this fix.
Death Knight is included by the native mask. Its damage-bonus-stat value 9 is
preserved, not silently normalized to a guessed Strength enum.

## Review units and gates

- `5d2957a7`: skill/talent/family/form assertions; formats 518 passed,
  installed mapped 6 passed, app 4487 passed / 12 ignored, zero failures.
- `a47f08d8`: class flag fix and regressions; formats 520 passed,
  installed mapped 7 passed, app 4487 passed / 12 ignored, zero failures.
- `4563c192`: faction/geoset assertions; formats 520 passed,
  installed mapped 7 plus render 1 passed, app 4487 passed / 12 ignored,
  zero failures. All exit codes zero.

These counts are for these exact units, not later integration or live validation.
The manager owns live checks. Second-stage transparency-weight work remains
parked; no shader/material changes are part of this audit.
