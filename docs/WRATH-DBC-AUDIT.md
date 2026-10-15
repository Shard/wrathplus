# Mapped Wrath DBC audit inventory

Snapshot: `native/assets-dbc-audit`, including the world-map known-row audit (2026-10-15).
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
| WorldMapArea, WorldMapContinent | `src/wrath_layouts_installed_tests.rs`: four-continent order, Elwynn/Stormwind/Hellfire/Borean rects; all four continent sheet/taxi bounds | Area display-map/floor/parent and continent WorldMapID remain unexposed; no live projection certification |
| SpellItemEnchantment | `tests/wrath_enchantments.rs`: Rockbiter, Fiery Weapon, Crusader, Mongoose, Berserking and selected stat enchants; names/visuals/bind flags | Selected rows only; effect/skill/gem columns and live glow rendering not certified |
| SpellRange | `tests/wrath_ranges.rs`: self, melee, ranged, minimum-distance and long-range pairs; flags and missing ID | Hostile pair only; friendly pair remains unexposed; not runtime reach certification |

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

## LFG catalog native evidence (build 12340)

The four LFG metadata readers are separate from the mapped Vanilla projections.
They preserve source order, duplicate rows, every raw file word, and selected
archive-locale text without substituting another language. They implement no
queue state, eligibility, choice ordering, reward, or enabled-state policy.

| Table | Native loader / raw decoder | File shape and mapping |
| --- | --- | --- |
| LFGDungeons | `0x647240` / `0x8b1ff0` via `0x65d8a0` | 49 words / 196 bytes: ID 0; name 1–16, mask 17; min/max/target/target-min/target-max 18–22; map 23; difficulty 24; flags 25; type 26; faction 27; texture 28; expansion 29; order 30; group 31; description 32–47, mask 48. |
| LFGDungeonGroup | `0x646ff0` / `0x8b1d00` via `0x65d790` | 21 words / 84 bytes: ID 0; name 1–16, mask 17; signed group sort priority 18; unlabeled header word 19; type 20. |
| LFGDungeonExpansion | `0x646da0` / `0x8bcfa0` via `0x65c760` | Eight sequential words / 32 bytes. ID 0, dungeon 1, expansion 2; preserve unlabeled 3; min/max/target-min/target-max 4–7. |
| MapDifficulty | `0x648770` / `0x8b44a0` via `0x65e130` | 23 words / 92 bytes: ID 0, map 1, difficulty 2, message 3–18, mask 19, unlabeled 20, maximum players 21, unlabeled string 22. Name accessor `0x8b4490`. |

Dependency identity is established by initialized executable globals, not a guessed
constructor: Expansion object `0xad3ff8` contains primary vtable `0xa281e0`
(loader slot `0x646da0`) and secondary `0xa281d8`; MapDifficulty object
`0xad4184` contains primary `0xa28398` (loader slot `0x648770`) and secondary
`0xa28390`. Their counts/storage at object +8/+0x1c match the globals used by
`0x6349f0` and `0x634950`. These helpers scan only the first contiguous run
matching dungeon or map respectively, then return the first matching secondary
key; an absent match remains absent. No default player count is synthesized.

Native UI evidence (`0x558350`, comparator `0x5536a0`) establishes signed group
priority and dungeon order. Group column 19 is copied into a header but ignored
by that comparator for negative header IDs; it remains deliberately unlabeled.
The bounded Group lookup consumers (`0x5536a0`, `0x554e60`, `0x556ad0`)
do not establish another meaning, so that unknown does not block a lossless DTO.
Signed accessors are bit-preserving views, not eligibility rules. All Expansion
words remain available even when a consumer does not use them.

`lfg::load_lfg_catalogs(&mut Chain)` uses only the supplied, boot-resolved chain;
all four tables are required and all read/decode failures propagate. No alternate
filesystem lookup, independent archive handle, or process-global cache is added.
App owns the Startup-after-Open immutable resource and per-VM feed. Byte readers
validate exact WDBC shape/length and every referenced string (including inactive
locale offsets); malformed overrides do not cause silent lower-priority fallback.
Installed known-row tests certify only the effective enUS chain, not stock-table
completeness, native Lua behavior, or live dungeon finder operation.

LFG catalog unit gates: formats **531 passed, 0 failed**; installed LFG
**1 passed, 0 failed**; app **4487 passed, 0 failed, 12 ignored**.
All exit codes zero. These counts certify this bounded metadata unit only.

## ItemLimitCategory native reader

Native build-12340 binding: object `0xad3e48` primary vtable `0xa28000`,
loader `0x6451c0` validates WDBC **20 columns / 80-byte rows**, name accessor
`0x8b0430` identifies `ItemLimitCategory.dbc`. Vtable slot +12 invokes raw
loader `0x6617c0`, which calls decoder `0x8ba2d0`: file column 0 is ID,
1–16 localized name, 17 locale mask, 18 quantity, 19 flags/raw mode word.
The compact runtime row is ID/name/quantity/flags at +0/+4/+8/+12, **not**
the file layout. Raw loader writes the ID pointer index in source order,
so duplicate-ID lookup is last-row-wins; all source rows remain available.

Roll consumer `0x754dd0` indexes this object (`0x754e49`–`0x754e65`), reads
quantity at `0x754e93` and name at `0x754ea2`; it does not inspect flags.
The formats reader gives flags no inventory/equipped-mode meaning. Missing
category lookup stays `None`, not zero. Loading uses the supplied resolved
Chain only, requires Wrath, and propagates read/decode failures. All words,
locale offsets/mask and selected-locale text are retained, without language
fallback. No App resource, inventory traversal, roll policy or live check
is included in this unit.

Read-only effective installed enUS inspection: **83 rows / 20 fields / 80
bytes**. Known rows: 2 Jeweler's Gems quantity 3 flags 1; 3 Healthstone 1/0;
4 Mana Gem 1/0; 6 Stormjewel 1/1. These checks are effective-chain examples,
not pristine stock-table or live eligibility certification.

ItemLimitCategory unit validation: formats library **534 passed / 0 failed**;
installed known-row regression **1 passed / 0 failed**; app library **4,487
passed / 0 failed / 12 ignored**. Rustfmt and diff whitespace checks passed.

## AnimationData fallback audit

Native filename accessor `0x8a93d0`, vtable `0xa27130` and object `0xad30c8`
bind loader `0x637550` (8 fields / 32 bytes) and raw loader `0x6590c0`.
Decoder `0x8a93e0` reads each disk word into the same runtime offset (name
is relocated at +4). The fallback walk at `0x7177d5..0x717823` resolves
an ID through that object's pointer index, then follows row `+0x14` at
`0x717814` when the requested clip cannot be used, with a visited/self guard.
The distinct lookup at `0x7176b0` returns `+0x18`; it is not the fallback.

Correct the Wrath mapping of the reader's legacy fallback slot 6 to disk
column 5. Vanilla column 6 is unchanged. The unused legacy slot 5 is absent
rather than aliased. Previously ordinary Wrath fallbacks read their own ID
and were suppressed; fly variants could incorrectly fall back to ground IDs.
`tests/wrath_animation_data.rs` asserts independently inspected raw words and
production names, weapon flags and fallback chains (including FlyAttack2H
247 → 246, not column-6 value 18). Synthetic 7/8-column fixtures protect both
dialects. Effective installed chain: 506 rows; not live animation parity.

AnimationData unit gates: formats **535 passed, 0 failed**; installed **1 passed,
0 failed**; app **4487 passed, 0 failed, 12 ignored**. Rustfmt and diff checks pass.
