# Wrath party-assignment member resolution (build 12340)

The assignment bindings resolve unit tokens to GUIDs. The bounded exact-name path
also accepts `exactMatch=1` and a complete self/party/raid name (including any realm
suffix); a unique GUID is required. No prefix or roster-first fallback is used.
Unresolved queries return zero values; unresolved mutations enqueue no command.
This unit deliberately adds no all-held-player snapshot.

## Deferred shared non-exact name resolver

Native `0x520190` delegates held-object candidate ranking to `0x51ffe0`:

- Enumerate held player objects matching the case-insensitive name; exact matching
  uses the full name, while non-exact matching permits prefixes. Realm-qualified
  names are assembled by the native name path, not matched by stripping realms.
- Filter mode 5 accepts self/party (`0x52c680`); mode 6 accepts self/party/raid
  (`0x512a00`, which includes the party/self helper). Party getter uses mode 5;
  raid getter and assignment mutations use mode 6.
- Score is squared **3D** distance (`0x4f61d0`) from the player or controlled-unit
  anchor. Add **10000.0** (`float 0xAC8A74`) once if health is <= 0, dynamic flags
  contain `0x20`, **or** stand-state byte equals 7. The player health/stand-state
  virtual leaves are `0x716490` and `0x71a380` (vtable offsets `0x128`, `0x138`).
- Self uses the special score stored at `float 0xA02A3C` (bytes `f9 02 15 4f`),
  rather than ordinary distance/penalty scoring.

Before implementing the shared resolver, close object-enumeration tie order,
controlled-unit anchor selection, and realm-name construction details. Existing
app `target/by_name.rs::PlayerLookup` follows another resolver (`0x493aa0`), so its
ranking must not be reused unchanged. VM `UnitState` has no position; `UnitReach`
and chain snapshots do not enumerate all held players.

Audit evidence: `/tmp/azb-ui-party-assignment-{resolve,match,ranking,ranking2,vtable,leaves}.log`.
These are local read-only disassembly logs, not repository assets or runtime inputs.
