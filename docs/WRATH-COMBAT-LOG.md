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

## App producers (`crates/benilla-app/src/combat_log/native.rs`)

The combat-log handlers record each packet; `feed` flags (`native_flags.rs`, `0x74dcb0`) and names
the units and pushes into Native UI's store. Built: SWING_DAMAGE/MISSED, SPELL_DAMAGE,
SPELL_PERIODIC_DAMAGE/HEAL/ENERGIZE/LEECH, SPELL_HEAL, SPELL_ENERGIZE, DAMAGE_SHIELD, SPELL_MISSED,
SPELL_INSTAKILL, SPELL_DISPEL, SPELL_DISPEL_FAILED, PARTY_KILL, ENVIRONMENTAL_DAMAGE. Labelled:
the swing miss type by AC victim state; the school as `1 << index`; a dispel's aura type by the
victim's hostility; no focus unit; names nil while unknown. Not built yet: SPELL_CAST_*, the aura
events, UNIT_DIED, the remaining effect logs.
