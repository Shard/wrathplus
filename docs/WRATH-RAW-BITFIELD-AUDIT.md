# Raw descriptor bitfield dialect audit

Status: closed, 2026-10-22: no pending rows. The last one, the Wrath special-area projection
(`0x800` sanctuary, `0x80` arena, `0x1000000` combat, per `0x51ba50`), is in `area.rs` with tests;
its ordinary-territory gate stays labelled. Reconciled 2026-10-16 (App Migration); tracked
findings from 2026-10-15. App follow-up after NPC fix `98321cbf`
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
| Reaction/attack/assist PvP and FFA sources | fixed (bounded) | App `native/app-migration`: Wrath `CanAttack`/`CanAssist` run 12340's `00729740`/`007293d0` branch topology (`target/wrath_relations.rs`) on dialect-aware PvP/FFA/sanctuary state, the two-hop CHARMEDBY/CREATEDBY controller resolver `00718b70` (a fresh guid lookup per hop, never a cached owner) and creature-query `type_flags` (Unit objects only). Every production caller passes live context (`RelationContext`). The selection ring's PvP read is dialect-aware. **Unresolved, labelled, not guessed:** reaction `007251c0`'s group/global helpers (`004cee50`, `005129f0`/`00512a00`), the full forced-reputation comparator, and `CanAttack`'s FLAGS2 bit16/vehicle guard (`0074c650`, metadata `0x20000000`). Spell-group predicates (`spell/group_relation.rs`) keep their one-hop owner. Evidence `/tmp/azb-wire-reaction-*.log`. |
| Quest-marker invalidation after PvP/FFA source migration | fixed (already merged) | `quest_markers/query.rs` polls the dialect-aware `unit_is_pvp`/`unit_is_ffa_pvp` pair as an authorised conservative superset, coalesced once per second (`pvp_refresh`); the native BYTES_2 watch registration remains a labelled TODO there. Reconciled 2026-10-16. |
| Nameplate PvP source / controlled-unit ownership | fixed (source) / labelled (inheritance) | `nameplates.rs` and the selection ring (`target/ring.rs`) read the unit's own dialect-aware `unit_is_pvp`. Controlled-unit inheritance (the reference reads the charmer's or summoner's) is not built and labelled in code. `vplates.rs` reads no PvP bit; its category is `CanAttack` (native on Wrath). |
| Vendor PvP/rank discount | fixed (bounded) | Wrath no longer applies the 1.12 honor-rank steps (Wrath `PLAYER_BYTES_3` byte 2 is the PvP title, which the old path read as a rank). The Wrath discount is AzerothCore `Player::GetReputationPriceDiscount`, 5% per rank above Neutral, what the server charges for repairs and taxi fares. **Labelled:** the 12340 client routine is untraced; the vendor reaction stands in for the rank, so Exalted depends on the reaction reaching 7. |
| Shared unit-flag masks | not-a-bug | Compared masks agree; composed predicates still need separate review. |
| Swim and animation-suppression predicates | not-a-bug | Swim `0x8 \| 0x800 \| 0x8000` = AC `UNIT_FLAG_PLAYER_CONTROLLED`, `PET_IN_COMBAT`, `SWIMMING`; loot kneel `0x400` = AC `UNIT_FLAG_LOOTING`, suppressor `0x10000000` unchanged (AC `UNK_28`). Masks agree; the 12340 composed predicates are untraced (labelled). |
| Unit flags 2 coverage | labelled (coverage gap) | No collision: no App code reads FLAGS2 as anything else. Feign death still reaches the dead look through `UNIT_DYNFLAG_DEAD`, which AC sets alongside `UNIT_FLAG2_FEIGN_DEATH`. Offhand/ranged disarm, cannot-turn and the CanAttack FLAGS2 bit16 vehicle guard remain unbuilt features, not migration rows. |
| Dynamic-flag masks | not-a-bug | Compared consumed masks agree. |
| Player flags other than FFA | not-a-bug | Compared masks agree; PvP preference 0x200 is not replicated PvP state. |
| Already semantic packed bytes / PvP title guards | not-a-bug | Preserve existing layout handling listed below. |
| Wrath special-area PvP projection | fixed | `area.rs::wrath_zone_pvp` runs `0x51ba50`: leaf flags (else zone), `0x800` sanctuary, `0x80` arena, `0x1000000` combat, with isSubZone from the leaf (`ZoneTexts::pvp_is_subzone`, wrathplus-ui, agreed with Native UI); then AC `CONTESTED_AREA` `0x40000`, then the player template masks (no template reads friendly). Vanilla `is_arena` untouched. **Labelled:** the ordinary-territory gate `[0xb2f998] == 0 && !(zone flags & 0x10)` (no values) is untraced and not applied. |
| Drunk scaling | not-a-bug | AC `Player::SetDrunkValue` writes 0..100 to `PLAYER_BYTES_3` byte 1; `player/drunk.rs::fraction` is `min(byte,100) × 0.01`, so the Wrath byte is consumed directly. The 12340 constant is untraced (labelled). |
| Highest lifetime rank offset | not-a-bug | Offset retained; no demonstrated move. |
| Gameobject consumed flag masks | not-a-bug | Numeric agreement; NO_INTERACT/NOT_SELECTABLE wording alone proves no collision. |
| Gameobject interaction policy | labelled (unresolved) | No mask collision (see inventory). The 12340 interaction predicate is untraced; the current policy stands and is not claimed native. |
| Item soulbound/unlocked/wrapped masks | not-a-bug | Instance masks agree; do not substitute template flags. |
| Item force-red 0x10 and refundable/BOP-trade coverage | labelled (unresolved) | No repurposing confirmed. Refundable/BOP-tradeable state belongs to Native UI's item-services seam (`set_item_refunds`, capture-only); no App consumer yet. |

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
Paths below are relative to `crates/wrathplus-app/src` unless stated otherwise.
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
App snapshot migrated in `b3f2ce6a`; the remaining callers are resolved in the status table (2026-10-16). Review
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
| Unit flags 2 | labelled (coverage gap) | No production App consumer found in reserved inventory; native layout has the field. AC includes feign death, hide body, offhand/ranged disarm and cannot-turn. | Coverage gap, not proof every current death/control path is wrong. SpellClick's additional FLAGS2/GUID eligibility is separately audited; raw flag presence alone must not send. |
| Dynamic flags | not-a-bug | Consumed lootable 1, tracked 2, tapped 4, tapped-by-player 8, special-info 0x10 and dead 0x20 agree. | Additional Wrath bits are not thereby implemented. |
| Player flags | not-a-bug | Leader, AFK/DND/GM, ghost, resting, helm/cloak and playtime masks agree. | Remaining FFA policy callers above are outstanding. PvP preference 0x200 is not a confirmed mismatch: AC still toggles it as preference despite renaming. |
| Other packed bytes | not-a-bug | Stand, race/class/power, sheath, drunk, rest, action bars, release timer, tracking and bank slots inventoried. | Drunk scaling resolved (byte 1 is 0..100 on AC; see the status table). Highest lifetime rank retains its offset; do not report it as moved. |
| Gameobject flags | not-a-bug | Consumed in-use 1, locked 2, conditional-interaction 4, refusal 0x10 agree numerically. | NO_INTERACT versus NOT_SELECTABLE wording warrants policy review, not a claimed mask collision. |
| Item instance flags | not-a-bug | Soulbound 1, unlocked 4 and wrapped 8 agree. | Force-red 0x10 remains unknown in AC; no confirmed repurposing. BOP-tradeable/refundable additions lack App consumers in inventory. Do not confuse instance flags with template flags. |

## Remaining delivery plan

1. Audit PvP reaction and quest-marker invalidation together, then the remaining
   pending rows above. A pure dialect-reading fix preserves the composed policy;
   ask the manager before any other policy change.
2. Migrate one App module family at a time with independent-bit/collision tests;
   request missing semantic predicates from Native Descriptor, never edit protocol.
3. Run the locked full `cargo test -p wrathplus-app --lib` gate before committing
   each unit and paste counts in handoff. Do not launch the client or push.
