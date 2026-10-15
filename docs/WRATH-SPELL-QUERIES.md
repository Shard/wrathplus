# Wrath learned-spell predicate

`IsSpellKnown` is Wrath-only. Its model snapshot is deliberately independent of the visible
spellbook: hidden learned abilities must not disappear from this predicate.

Installed build-12340 Wow.exe SHA256:
`c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c`.
Read-only native audit:

- Binding `0x53c3a0`: numeric spell ID (numeric strings accepted), truncate to signed low i32,
  require positive; otherwise raise `Usage: IsSpellKnown(spellID[, isPet])`. Returns exactly one
  Lua boolean, not 1/nil.
- Optional argument helper `0x815500`: booleans, numeric values and numeric strings; omitted,
  nil and unsupported types default to zero. Numeric values truncate before testing nonzero.
- `0x53b4e0` scans separate player (`0xbe6d88`, count `0xbe8d98`) and pet (`0xbe7d98`, count
  `0xbe8da4`) arrays. No rank substitution.
- Pet-only fallback `0x5d3560`: scans ten pet action words at `0xc23508`, matching the low
  24-bit ID where `(word >> 24) & 0x3f` equals 1 or is in 8..17. Commands/reactions do not count.

UI seam: `UiScript::set_wrath_known_spells(player, pet, pet_actions)` replaces all three inputs
atomically, retaining at most ten action words. App owns feeding the full learned sets on
initial/learn/unlearn and pet changes, resetting on session removal, and preserving this snapshot
on UI reconstruction. Empty initial state is unavailable feed data, not proof of an empty server
spellbook. UI tests cover native ABI, hidden learned IDs, separate books, pet action types,
reset/replacement and dialect isolation. No app integration is claimed by the UI seam alone.

Stock callers: `MultiCastActionBarFrame.lua:950,1027` (`IsSpellKnown`).
Separate predicates `IsSelectedSpell` (`0x5412c0`, helper `0x53d350`) and `IsCurrentSpell`
(`0x541500`, helper `0x806030`) are not implemented by this unit. Current includes attack,
auto-repeat, trade-skill and form checks; it must not be approximated by ordinary cast state.

Local audit artifacts (not repository assets): `/tmp/azb-ui-spellquery-{names,bindings,known,
known-asm,resolvers}.log`.

Transport audit: the current Wrath adapter does not register `SMSG_PET_SPELLS` (`0x179`);
the packet is dropped. The shared Vanilla parser is not a substitute (it omits Wrath's u16
family). Native pet feed requires a separate codec/route before integration can claim complete
pet knowledge. Retained native words must decode spell IDs with `& 0x00ffffff`, not the shared
Vanilla `PetActionEntry::action()` low-16-bit helper. This UI unit adds no protocol changes.
