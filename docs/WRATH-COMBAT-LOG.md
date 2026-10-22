# Wrath combat log (build 12340)

Status: evidence for the "Wrath combat log" family. App builds entries and unit flags; Native UI
owns the store, the `CombatLog*` getters and filters, and event firing. One headless Ghidra round
on the pinned Wow.exe (SHA256 `c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c`);
logs `/tmp/azb-app-mig/ghidra/combatlog-{bindings,entry,tables,builders,flags,alloc,unitflags}.log`.
No executable bytes are committed.

## Store and events

- The entry list is `0x00ca1390` (current) / `0x00ca1394` (pending tail). `CombatLogGetCurrentEntry`
  (`0x74f2b0`) pushes the current entry through `0x74e290`; `CombatLogAdvanceEntry` is `0x74fc20`;
  `CombatLogAddFilter` is `0x74ff70`.
- Each new entry (`0x74f910`): its arguments are pushed; if the active filter list (`0xadb970`,
  tested by `0x74e1a0` through `0x74f6c0`) admits it, `COMBAT_LOG_EVENT` (event `0x236`) fires;
  then, when `COMBAT_LOG_EVENT_UNFILTERED` (`0x237`) has a registrant, that fires with the same
  arguments.
- The allocator `0x750400` recycles entries older than the retention CVar (seconds), fills the
  event index, the source and destination GUIDs, names (`0x74fd40`) and flags (`0x74dcb0`), and the
  spell id. A spell with `AttributesEx4 & 0x2000` (record `+0x20`) logs no source.
- `0x74d210`: a creature whose template has flag `0x4000` logs its GUID with the low counter
  cleared.

## Arguments (`0x74e290`)

Base eight: `timestamp, event, srcGUID, srcName, srcFlags, dstGUID, dstName, dstFlags`. Native
`timestamp` is `(entry ms − base ms) × 0.001 + base` with base `0xca138c` in Unix seconds; App
stamps local Unix time with milliseconds at the packet's arrival (the base's origin is untraced). GUIDs are
`0x%016X` strings (`0x74d0d0`); a name is nil when absent, `name-realm` for another realm.

An entry with a spell id adds `spellId, spellName, spellSchool`. Then each set bit of the entry's
group word (`+0x54`) adds, in this order:

| Bit | Arguments |
| --- | --- |
| `0x1` | one integer |
| `0x2` | one string (`SPELL_CAST_FAILED`'s failed type) |
| `0x4` | miss type, table `0xadbfac`: NONE, MISS, RESIST, DODGE, PARRY, BLOCK, EVADE, IMMUNE, IMMUNE, DEFLECT, ABSORB, REFLECT |
| `0x8` | environmental type, table `0xad671c`: FATIGUE, DROWNING, FALLING, LAVA, SLIME, FIRE |
| `0x10` | damage: amount, overkill, school, resisted, blocked, absorbed (each nil when 0), critical, glancing, crushing (1 or nil) |
| `0x2000` / `0x4000` / `0x8000` | one integer each (`+0x68`, `+0x6c`, `+0x70`) |
| `0x20` | heal: amount, overhealing, absorbed, critical (1 or nil) |
| `0x40` | energize: amount, power type |
| `0x80` | drain/leech: amount, power type, extra amount (nil when 0) |
| `0x100` | extra spell: id, name, school |
| `0x1000` | one string |
| `0x200` | item id, item name |
| `0x400` | aura type, `BUFF` or `DEBUFF` |
| `0x800` | one integer (dose amount, extra attacks) |

Events (table `0xadb758`, indices 0-49): ENVIRONMENTAL_DAMAGE, SWING_DAMAGE, SWING_MISSED,
RANGE_DAMAGE, RANGE_MISSED, SPELL_CAST_START, SPELL_CAST_SUCCESS, SPELL_CAST_FAILED, SPELL_MISSED,
SPELL_DAMAGE, SPELL_HEAL, SPELL_ENERGIZE, SPELL_DRAIN, SPELL_LEECH, SPELL_INSTAKILL, SPELL_SUMMON,
SPELL_CREATE, SPELL_INTERRUPT, SPELL_EXTRA_ATTACKS, SPELL_DURABILITY_DAMAGE,
SPELL_DURABILITY_DAMAGE_ALL, SPELL_AURA_APPLIED, SPELL_AURA_APPLIED_DOSE, SPELL_AURA_REMOVED_DOSE,
SPELL_AURA_REMOVED, SPELL_AURA_REFRESH, SPELL_DISPEL, SPELL_STOLEN, SPELL_AURA_BROKEN,
SPELL_AURA_BROKEN_SPELL, DAMAGE_AURA_BROKEN, ENCHANT_APPLIED, ENCHANT_REMOVED,
SPELL_PERIODIC_MISSED, SPELL_PERIODIC_DAMAGE, SPELL_PERIODIC_HEAL, SPELL_PERIODIC_ENERGIZE,
SPELL_PERIODIC_DRAIN, SPELL_PERIODIC_LEECH, SPELL_DISPEL_FAILED, DAMAGE_SHIELD,
DAMAGE_SHIELD_MISSED, DAMAGE_SPLIT, PARTY_KILL, UNIT_DIED, UNIT_DESTROYED, SPELL_RESURRECT,
SPELL_BUILDING_DAMAGE, SPELL_BUILDING_HEAL, UNIT_DISSIPATES.

The melee builder `0x7512b0` (attacker state update) is the template: damage sums its two school
rows, resisted and absorbed are the rows' sums, blocked the packet's, and the hit-info bits
`0x200`/`0x10000`/`0x20000` give critical, glancing and crushing.

## Unit flags (`0x74dcb0`)

- No GUID: `0x80000000` (NONE).
- Controller: a unit's CHARMEDBY, else CREATEDBY; a game object's creator; one more hop when that
  owner is itself owned.
- Type: the unit itself controls itself: player GUID `0x400` PLAYER, pet GUID (`0xF040`) `0x1000`
  PET, otherwise `0x800` NPC. Controlled by another: `0x1000` PET with CHARMEDBY or SUMMONEDBY set,
  else `0x2000` GUARDIAN. Not a unit: `0x4000` OBJECT.
- Control: the controller GUID is a player's: `0x100`, else `0x200`.
- Affiliation and reaction: controlled by the local player: `0x11` (MINE, FRIENDLY). Otherwise the
  local player's reaction (`0x7251c0`): above 3 FRIENDLY `0x10`, 2-3 neutral, below 2 HOSTILE
  `0x40`; in a group, a party member's `0x12` (PARTY, FRIENDLY), a raid member's `0x14`. No
  affiliation reads OUTSIDER `0x8`; no reaction reads NEUTRAL `0x20`.
- Markers: target `0x10000`, focus `0x20000`; a player in the group, main tank `0x40000` or main
  assist `0x80000`; in a group, raid icon `n` (0-7) sets `0x100000 << n`.

## App producers (`crates/wrathplus-app/src/combat_log/native.rs`)

The combat-log handlers record each packet; `feed` flags (`native_flags.rs`, `0x74dcb0`) and names
the units and pushes into Native UI's store. Every spell entry needs a named spell not hidden from
the log (`Attributes & 0x180`, the gate each builder applies). Built:

- Part 1: SWING_DAMAGE/MISSED, SPELL_DAMAGE, SPELL_PERIODIC_DAMAGE/HEAL/ENERGIZE/LEECH, SPELL_HEAL,
  SPELL_ENERGIZE, DAMAGE_SHIELD, SPELL_INSTAKILL, SPELL_DISPEL, SPELL_DISPEL_FAILED,
  PARTY_KILL, ENVIRONMENTAL_DAMAGE.
- Casts: SPELL_CAST_START (`0x751920`, no destination), SPELL_CAST_SUCCESS (`0x7519e0`, the unit
  target), SPELL_CAST_FAILED (`0x751ad0`, our own result; group `0x2` the reason's text).
- Auras (`0x752710`, `0x752860`, `0x752ba0`), from the packet aura snapshots: SPELL_AURA_APPLIED,
  SPELL_AURA_REMOVED (only while the unit lives), SPELL_AURA_APPLIED_DOSE/REMOVED_DOSE (a spell
  with `StackAmount > 1`; the new count), SPELL_AURA_REFRESH; applied, removed and refresh skip
  passive spells. Group `0x400` is `BUFF` for a zero byte (`0x74e290`).
- Dispel and steal (`0x752930`, from the `0x27B`/`0x333` handler `0x6ce2b0`): the removed aura is
  the extra spell, the per-aura wire byte the aura type; SPELL_STOLEN.
- Execute log rows: SPELL_DRAIN (effect 8), SPELL_EXTRA_ATTACKS (`0x752520`), SPELL_INTERRUPT
  (`0x7523d0`, the interrupted spell as the extra spell), SPELL_DURABILITY_DAMAGE(_ALL)
  (`0x7525e0`/`0x752680`), SPELL_RESURRECT (`0x752340`), SPELL_SUMMON or SPELL_CREATE by the
  destination guid's kind (`0x7522a0`).
- UNIT_DIED (`0x752ed0`): a unit's health reaching 0 after a living reading. UNIT_DISSIPATES
  instead when the unit's creature template has type 13 (gas cloud); UNIT_DESTROYED when its
  `UNIT_CREATED_BY_SPELL`'s first effect is SUMMON (28) and that effect's `SummonProperties` row
  has Slot 1-4 (a totem slot). Base arguments only.
- SPELL_MISSED `RESIST` from `SMSG_PROCRESIST` (`0x260`, handler `0x750d40` in table `0x7538a0`):
  caster, victim, spell; group `0x4004`, the `0x4000` integer 0.
- ENCHANT_APPLIED, or ENCHANT_REMOVED for an empty caster, from `SMSG_ENCHANTMENTLOG` (`0x1D7`,
  `0x751050`): no spell; group `0x1000` the `SpellItemEnchantment` name, then group `0x200` the
  item; an enchant with no row or with `Flags & 0x2` logs nothing.
- DAMAGE_SPLIT (`0x752de0`, `MOV EDI,0x2a` into the damage template `0x750660`): an
  `SMSG_SPELLNONMELEEDAMAGELOG` with hit flag `0x8` (AzerothCore `SPELL_HIT_TYPE_SPLIT`), ahead of
  the periodic test; the damage group as SPELL_DAMAGE's.
- DAMAGE_SHIELD_MISSED and SPELL_PERIODIC_MISSED from `SMSG_SPELLLOGMISS` and
  `SMSG_SPELLORDAMAGE_IMMUNE`: see "Damage-shield misses" below.

Labelled:
- the swing miss type by AC victim state, and the school as `1 << index`;
- no focus unit; names are nil while unknown;
- the cast unit check (vtable `+0x148`) is skipped, and the failure text's `%s` is not filled;
- the aura snapshot diff stands in for `0x72f5d0`: a unit's first snapshot logs nothing, a re-sent
  unchanged slot reads as a refresh, and the `AttributesEx6 & 0x400` gate isn't applied;
- the execute-log dispatcher isn't a defined function, so effects map by AzerothCore's effect ids;
  a durability or enchant entry's item name is nil;
- UNIT_DESTROYED and ENCHANT_* source and destination: `0x752ed0`'s spell-lookup test reads
  inverted in the decompile, taken as the found record; the enchant's caster is the source and
  the item's owner the destination, by AzerothCore's field names;
- a resisted proc raises no floating text (`0x750d40` calls `0x74d550(0x16)` for the local victim).

Not built: SPELL_AURA_BROKEN(_SPELL) (`0x752a40`, opcode `0x14F`, which AzerothCore never sends),
SPELL_BUILDING_DAMAGE/HEAL, SPELL_PERIODIC_DRAIN, SPELL_LEECH, SPELL_CREATE for items, and the
cast's own miss list (`0x751b80`'s other callers `0x700e20`/`0x702fc0`, not decompiled).

## Damage-shield misses (`0x751b80`)

`0x751b80(source, dest, spell, missType, shield, periodic, age)` gates on the spell (named,
`Attributes & 0x180` clear) and a non-zero miss type, then picks the event in `EDI` for the miss
template `0x750500`: `periodic` gives SPELL_PERIODIC_MISSED (`0x21`), else `shield` gives
DAMAGE_SHIELD_MISSED (`0x29`), else RANGE_MISSED or SPELL_MISSED by `AttributesEx3 & 0x8000`.

- `SMSG_SPELLLOGMISS` (`0x24B`, `0x6d3090`): each row passes `shield` as a constant 1, so every
  row is DAMAGE_SHIELD_MISSED with its miss type; a debug-format packet logs nothing. AzerothCore
  sends it for damage-shield misses (`Unit::DealDamageShieldDamage`) and for a spell's delayed hit
  result (`Spell.cpp:2752`); the client logs both as the shield miss, and so do we.
- `SMSG_SPELLORDAMAGE_IMMUNE` (`0x263`, handler `0x6ce3d0`, registered by `0x6e83b0`, not the
  combat-log table `0x7538a0`): miss type IMMUNE; `shield` is a zero log-format byte (always, from
  AzerothCore), `periodic` a non-zero one.

Labelled: the miss carries its type only; `0x750500`'s amount groups (`0x74d980`) are not traced.
