# Raw descriptor bitfield dialect audit

Status: tracked findings, 2026-10-15. App follow-up after NPC fix `98321cbf`
(integrated as `705ebdd0`). No behavioral changes in this document. No client launched.
Auction work remains paused. Protocol semantic predicates belong to Native Descriptor.

## Finding status

Statuses apply only to the bounded finding described, not complete subsystem parity.
`not-a-bug` means no demonstrated dialect collision; it does not certify native policy.

| Finding | Status | Resolution / next step |
| --- | --- | --- |
| Pet menu rename/abandon source | fixed | Descriptor `0f34b1c0`; App `ebdaaec6`. Ownership and drain unchanged. App 4,550 passed / 0 failed / 12 ignored. |
| Aura MOUNT/AURAS_VISIBLE collision and native 295 guard | fixed | App `5837336b`. Present effects, whole-list gate and catalog/context invalidation; missing spell row explicitly remains visible. App 4,553 / 0 / 12. |
| Unit snapshot PvP/FFA state sources | fixed | Descriptor `c89f8d25`; App `b3f2ce6a`. Raw state only. App 4,554 / 0 / 12. |
| Reaction/attack/assist PvP and FFA sources | pending | Audit `target/ring.rs` and `target/relations.rs`; preserve policy unless manager authorizes change. |
| Quest-marker invalidation after PvP/FFA source migration | pending | Audit `quest_markers/query.rs` generation and field-edge keys together with reaction migration. |
| Nameplate PvP source / controlled-unit ownership | pending | Separate direct state from inheritance policy in `nameplates.rs` and `vplates.rs`. |
| Vendor PvP/rank discount | pending | `target/price_discount.rs`; dialect source correction does not authorize Vanilla rank-discount policy in Wrath. |
| Shared unit-flag masks | not-a-bug | Compared masks agree; composed predicates still need separate review. |
| Swim and animation-suppression predicates | pending | Audit `player/swim.rs` and `creature_anim/driver.rs`; not a demonstrated mask collision. |
| Unit flags 2 coverage | pending | Missing native consumers are a coverage gap, not a proven collision. Keep SpellClick eligibility separate. |
| Dynamic-flag masks | not-a-bug | Compared consumed masks agree. |
| Player flags other than FFA | not-a-bug | Compared masks agree; PvP preference 0x200 is not replicated PvP state. |
| Already semantic packed bytes / PvP title guards | not-a-bug | Preserve existing layout handling listed below. |
| Wrath special-area PvP projection | pending | App `area.rs` still projects Vanilla territory semantics. Wrath `GetZonePVPInfo` `0x0051ba50` reads leaf AreaTable flags (falls back to zone flags when leaf flags are unavailable): `0x800` sanctuary, then `0x80` arena, then `0x1000000` combat. These branches return `(type, 1/nil isSubZone)` (two values); ordinary territory returns `(type, nil, factionName)` (three values). App needs explicit Wrath special-area/context projection; do not reinterpret Vanilla `is_arena` as `isSubZone`. Native evidence: `/tmp/azb-ui-live-errors-native.log` (pinned Wow.exe SHA256 `c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c`). |
| Drunk scaling | pending | Numeric-policy question; do not infer a field move. |
| Highest lifetime rank offset | not-a-bug | Offset retained; no demonstrated move. |
| Gameobject consumed flag masks | not-a-bug | Numeric agreement; NO_INTERACT/NOT_SELECTABLE wording alone proves no collision. |
| Gameobject interaction policy | pending | Native predicate audit needed before behavior changes. |
| Item soulbound/unlocked/wrapped masks | not-a-bug | Instance masks agree; do not substitute template flags. |
| Item force-red 0x10 and refundable/BOP-trade coverage | pending | Meaning/coverage unresolved; no confirmed repurposing. |

## Completed changes and evidence

The three fixed App units above are manager-confirmed cherry-picked. Test counts
are their locked full-App gates; no live-client validation is claimed. The sections
below retain the original hazard and native evidence so pending callers can be
reviewed without confusing direct-state accessors with composed policy.

## Method and evidence limits

Inventory covers App consumers of unit flags, secondary unit flags, dynamic flags,
player flags, packed unit/player bytes, gameobject flags and item instance flags.
Compare the local AzerothCore reference under
`azerothcore-playerbots/core/src/server/` with the repository's pinned vmangos
revision `e5f2bc2645bc971e3864cba5b601c1fc0eea8277`.
Paths below are relative to `crates/benilla-app/src` unless stated otherwise.
Enum agreement establishes a bit's meaning, not the complete native client branch
predicate. Missing native functionality is not automatically a mask collision.

## Confirmed dialect hazards (priority order)

### 1. Pet menu permissions: moved packed flags

`ui_pet/menu.rs` interprets `unit_flags()` mask `0x10` as rename and `0x20` as
abandon in menu feeding. Those are Vanilla masks. The current drain does not
re-check either permission; adding sender authorization is a separate behavior change.
Wrath stores rename/abandon in `UNIT_FIELD_BYTES_2` byte 2, masks `0x01`/`0x02`.
AzerothCore `game/Entities/Unit/UnitDefines.h` defines the packed pet flags;
`game/Entities/Pet/Pet.cpp` writes them. vmangos `UnitDefines.h` explicitly notes
the move. Wrath unit flag `0x20` instead denotes PREPARATION.

Implemented bounded fix (`ebdaaec6`): Descriptor-owned `ObjectFields` semantic rename/abandon
predicates, preserving Vanilla masks and choosing packed Wrath flags. App menu
feed consumes these predicates; keep the drain unchanged and preserve ownership,
pet-selection and command semantics unchanged. Regression cases must independently
set each real flag and each misleading old mask in both layouts.

### 2. Aura visibility: flag repurposed

`ui_aura.rs::buffs_visible_on` treats `UNIT_FLAGS & 0x08000000` as Detect Magic /
AURAS_VISIBLE. That is the vmangos meaning; AzerothCore `UnitDefines.h` gives the
same bit the Wrath meaning MOUNT. Mounted state cannot authorize revealing buffs.

The collision is confirmed; the replacement native UnitBuff visibility policy
is NOT established by enum comparison. Audit native getter/visibility behavior
before choosing whether to bypass, replace or dialect-gate this branch. Do not
merely rename the mask or assume the Vanilla can-assist fallback is native.

Current reachability: `ui_aura/units.rs::feed` calls `refresh_native` when a GUID
has a native packet-store entry (including an empty list), bypassing this gate.
An out-of-view group unit with native group aura state also uses `refresh_native`.
Without either native entry it falls back to `refresh`, which can invoke the
Vanilla visibility predicate. Thus this is not evidence that every native packet
aura is currently filtered by MOUNT; the fallback and its invalidation need review.

Wire's bounded native audit identifies UnitBuff `00614ca0`, shared core `006147c0`,
selector `0072c9b0`, and rebuild `0072c510`. None contains the Vanilla MOUNT-bit /
can-assist visibility ladder. A distinct conditional guard checks `unit+0xF44`
bit `0x80`. Follow-up resolves this as native aura-type bitset index 295, NOT a
unit descriptor flag: writer `00727e70` rebuilds the bitset from received aura
spell-effect types and present-effect flags. AC names type 295 only `SPELL_AURA_295`;
do not invent a semantic label.

The exact live getter guard is: if target has aura type 295, local-player GUID
is nonzero, and neither `reaction(target, localplayer) >= 4` nor target creature
metadata flag `0x04000000` (TREAT_AS_RAID_UNIT) holds, return no aura. This guard
is shared by UnitBuff, UnitDebuff, UnitAura and tooltip. `00514050` receives TARGET
as `this` and LOCALPLAYER as argument; it is not a local-player-only predicate.
Neutral reaction 4 passes, so this is not the old can-assist policy.

Manager decision: implement this native rule with the existing spell catalog's
`effect_apply_aura` metadata and received present-effect inputs. An unloaded spell
row is explicitly treated as no type 295 (visible), and visibility must be
re-evaluated when the catalog arrives. This is an authorized availability fallback,
not evidence that unknown spells cannot apply type 295. Keep the Vanilla gate only
on Vanilla; visibility/cache invalidation must cover native packet and roster
paths, target reaction and creature-template changes as well as aura changes.
Evidence: `/tmp/azb-wire-aura-{bindings,getters,core,core-asm,selection,rebuild}.log`
and follow-up `/tmp/azb-wire-aura-{fieldwriter,field,specialrefs}.log`.

### 3. PvP and FFA: state source changed

`ui_unit.rs` and `target/ring.rs::ffa_reaction` interpret PLAYER_FLAGS `0x80` as
FFA. AzerothCore `game/Entities/Player/Player.h` labels this pre-3.0.3 FFA;
Wrath FFA is `UNIT_FIELD_BYTES_2` byte 1 mask `0x04`.

`ui_unit.rs`, `nameplates.rs`, `target/{ring,relations,price_discount}.rs` read
UNIT_FLAGS `0x1000` as PvP. AzerothCore retains that name with a changed-in-3.0.3
annotation, but `Unit.h::IsPvP` reads BYTES_2 byte 1 mask `0x01`.

Implemented seam (`c89f8d25`): Descriptor-owned dialect-aware PvP/FFA state predicates;
App snapshot migrated in `b3f2ce6a`. Remaining callers are pending. Review
individual callers before migration: faction reaction, controlled-unit inheritance,
player preference, PvP title and vendor discount are distinct policies. In
particular, no evidence here authorizes transplanting Vanilla rank discounts.

## Existing semantic handling: do not regress

Protocol already handles the following packed-layout changes:

- Shapeshift: Vanilla UnitBytes1 byte 2, Wrath UnitBytes2 byte 3.
- Visual flags: Vanilla byte 3, Wrath byte 2; Vanilla ghost-visual bit is guarded.
- Pet loyalty: Wrath's pet-talent-point byte is not interpreted as loyalty.
- Combo points: native descriptor accessor returns None rather than interpreting
  Wrath RAF grantable levels as combo points. Callers defaulting to zero indicate
  incomplete functionality, not successful native combo-point integration.
- PvP title: byte shift is handled; city-protector medal and Vanilla honor rank
  progress accessors are guarded for Wrath.

## Other inventory and confidence

| Family | Status | Finding | Limits / next work |
| --- | --- | --- | --- |
| Unit flags | not-a-bug | Principal consumed stun, silence, pacify, fear/confuse, combat, taxi, disarm, possessed, selectable, skinnable, immunity and looting masks agree across references. | Agreement does not prove each composed client predicate. Review swim `player/swim.rs`, animation suppression `creature_anim/driver.rs` and nameplate owner gate `vplates.rs` separately. |
| Unit flags 2 | pending | No production App consumer found in reserved inventory; native layout has the field. AC includes feign death, hide body, offhand/ranged disarm and cannot-turn. | Coverage gap, not proof every current death/control path is wrong. SpellClick's additional FLAGS2/GUID eligibility is separately audited; raw flag presence alone must not send. |
| Dynamic flags | not-a-bug | Consumed lootable 1, tracked 2, tapped 4, tapped-by-player 8, special-info 0x10 and dead 0x20 agree. | Additional Wrath bits are not thereby implemented. |
| Player flags | not-a-bug | Leader, AFK/DND/GM, ghost, resting, helm/cloak and playtime masks agree. | Remaining FFA policy callers above are outstanding. PvP preference 0x200 is not a confirmed mismatch: AC still toggles it as preference despite renaming. |
| Other packed bytes | pending | Stand, race/class/power, sheath, drunk, rest, action bars, release timer, tracking and bank slots inventoried. | Drunk scaling remains a numeric-policy question. Highest lifetime rank retains its offset; do not report it as moved. |
| Gameobject flags | not-a-bug | Consumed in-use 1, locked 2, conditional-interaction 4, refusal 0x10 agree numerically. | NO_INTERACT versus NOT_SELECTABLE wording warrants policy review, not a claimed mask collision. |
| Item instance flags | not-a-bug | Soulbound 1, unlocked 4 and wrapped 8 agree. | Force-red 0x10 remains unknown in AC; no confirmed repurposing. BOP-tradeable/refundable additions lack App consumers in inventory. Do not confuse instance flags with template flags. |

## Remaining delivery plan

1. Audit PvP reaction and quest-marker invalidation together, then the remaining
   pending rows above. A pure dialect-reading fix preserves the composed policy;
   ask the manager before any other policy change.
2. Migrate one App module family at a time with independent-bit/collision tests;
   request missing semantic predicates from Native Descriptor, never edit protocol.
3. Run the locked full `cargo test -p benilla-app --lib` gate before committing
   each unit and paste counts in handoff. Do not launch the client or push.
