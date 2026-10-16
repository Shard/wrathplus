# Wrath L0 preference persistence contract

Scope: `GetLFDChoiceCollapseState`, `GetLFDChoiceEnabledState`,
`SetLFGHeaderCollapsed`, `SetLFGDungeonEnabled`. No queue membership, lock
eligibility, role mutation, command or network side effect is implied.

## Ownership and registration

The existing host CVar registry owns persistence; UI reads its applied mirror and
uses the existing engine-write/change queue. There is no parallel LFG preference
store or new App reducer. Native registration `0051d9b0 -> 00767fc0` gives:

| Name | Default | Native flags |
| --- | --- | --- |
| `lfgSelectedRoles` | `"0"` | `0x120` |
| `lfdSelectedDungeons` | empty | `0x120` |
| `lfdCollapsedHeaders` | empty | `0x120` |

The current registry models the latch bit, not all raw flags: these rows are not
latched. Other native flag policies are not claimed implemented. Shared registry
exposure of these three names in Vanilla is intentional and manager-approved,
not evidence of native Vanilla registration. The four bindings are Wrath-only.
Roles registration does not implement role selection or replace the role shim.

## Getter and setter shapes

Both getters accept nil/no argument or a table; other types raise. A supplied
table is cleared and reused. Results cover the union of native LFD/LFR catalog
choice lists (the same filters as choice order/info), not guessed eligibility.

- Collapse returns explicit booleans for every choice. Header keys are negative;
  their bit index is the positive header ID. Dungeon keys read the dungeon's
  group ID, not the dungeon ID.
- Enabled returns only positive dungeon keys whose stored bits are set, with
  boolean true. Unset keys and header keys are absent, not false.
- Header setter accepts a numeric/coercible negative header ID resolving in group
  metadata. Other numeric IDs do nothing. Its omitted flag defaults false.
- Dungeon setter accepts any positive numeric/coercible dungeon ID, subject to
  the native bitset buffer bound; it does not require a catalog match. Its omitted
  flag defaults true. Explicit nil is false, distinct from omission.
- Both setters return zero values. Flags use the native boolean/default helper,
  not Lua truthiness. Invalid numeric arguments raise.

## Bytes and VM replacement

Version 2 is byte `02` followed by six-bit chunks with bit `40` set. Bit `i`
uses payload byte `i / 6` and mask `1 << (i % 6)`. Empty is the registered default.
Legacy version 1 repacks seven-bit payload chunks into version 2. A getter may
perform that migration; migration dirties persistence only if the applied string
changes. Reading current values, unchanged writes, host reseeding and registration
must not dirty persistence. Native strings stop at their first NUL.

The setter uses the native 256-byte scratch-buffer bounds, refuses original
strings longer than 254 bytes, and trims trailing `40` chunks when clearing.
Only version 1 is migrated; unknown markers are not silently reset. Invalid
metadata must not cause the native null-dereference behavior: existing catalog
choice builders retain their safe deterministic policy.

At VM replacement the host retains its authoritative registry, seeds saved values
before registration and/or installs its applied mirror. Registration preserves
existing values. No replayed setter, queue join, request or role decision occurs.
The host saves changed strings through its existing config path; it must neither
expand the sparse enabled table into eligibility nor reconstruct saved strings
from visible choices (which could discard choices absent from the current catalog).

## Evidence

Installed Wow.exe SHA256
`c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c`.
Getters `005564d0`, `005567e0`; setters `00556ad0`, `00556b60`;
helpers `00555f30`, `00555f80`, `00766860`, `00766b20`, `00766ca0`.
Local read-only logs: `/tmp/azb-ui-lfg-preference-{closure,helpers}.log`,
`/tmp/azb-ui-lfg-l0-{selection,state-arguments-asm,boolean-bitset,preference-state}.log`.
No executable bytes, game sources or extracted data are committed.
