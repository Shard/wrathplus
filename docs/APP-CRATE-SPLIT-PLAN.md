# Splitting benilla-app

Analysis and plan only; no code moves yet. The manager schedules execution after the Vanilla
removal. Measured on wrath-hud 3e913bc1 (2026-10-20).

## Why

`benilla-app` is one crate: 180 top-level modules, about 443k lines (with tests), about 4.8k lib
tests and 16 integration test files. Any edit recompiles all of it and relinks its test binary,
which is the slowest step in every worker's gate. Splitting it lets an edit in, say, a `ui_*`
feed rebuild only the UI layer and the thin root, and lets the lib tests of each layer build and
run in parallel.

## What the graph looks like today

Edges are `crate::<module>` references from one top-level module to another.

- 164 of the 180 modules (about 438k lines) form one strongly connected component: almost every
  module can reach almost every other. No crate boundary cuts it as it stands.
- The 16 modules outside it are small leaves (`debug_panel`, `showcase_capture`, `hover_log`,
  `smart_rect`, `dev`, `test_support`, `shaders`, …, about 5.7k lines).
- Hubs by in-degree (modules referencing them): `net` 127, `ui_script` 104, `ui_action` 66,
  `names` 62, `ui_unit` 58, `entities` 52, `target` 48, `player` 41, `items` 38,
  `creature_anim` 36, `char_select` 34.

The cycles come from three patterns, not from tangled features:

1. **Shared game state lives in a feature module.** The spell book (`ui_action::Spells`), UI
   error keys (`ui_action::UiErrorKeys`, `CastErrors`, `CastFail`), the client state
   (`char_select::ClientState`, `InWorldGated`, `Roster`), group state (`ui_party::GroupState`),
   loot state (`ui_loot::LootState`, `LootLatch`), the input map (`bindings::Input`,
   `BindingsState`), the NPC interaction (`ui_session::InteractNpc`), the pet bar
   (`ui_pet::PetBar`), the quest giver (`ui_quest::QuestGiver`) and the guild state
   (`ui_guild::GuildState`) are owned by UI modules but read by world and net code.
2. **`cvars` calls into the features it configures** (`player::camera_view::VIEW_CVARS`,
   `video::VideoConfig`, `realmlist::Realmlist`, `ui_gamma::DisplayGamma`, the `on_cvar` hooks of
   `ui_loot`, `ui_chat::combat`, `ui_guild`, `perf`, …): the registry knows every consumer.
3. **Low layers borrow constants and schedule sets from high ones.** `net` reads the player
   capsule (`player::CAPSULE_RADIUS`, `CAPSULE_HEIGHT`, `HOVER_HEIGHT`, `PlayerCapsule`,
   `Player`), movement helpers (`creature_anim::move_flags`, `wrap_pi`, `HardLanding`,
   `swim_body_rotation`) and `transport::TransportRider`; world systems order themselves on UI
   sets (`ui_script::UiFeed`, `UiInput`, `ui_unit::UnitFeed`); `net` re-exports the
   `ui_native::wrath_*` capture FIFOs it fills.

## Proposed crates

| Crate | Layer | Today's modules (examples) | Size | Lib tests |
|---|---|---|---|---|
| `benilla-app-core` | shared state, schedule sets, cvar registry | `run_mode`, `local_state`, `names`, `items`, `go_templates`, `query_cache`, `area`, `area_trigger`, `chr_classes`, `creature_type`, `text_filter`, `text_reshape`, `glue_strings`, `crash`, `preflight`, `probe_shield`, `shutdown`, `name_persist`, `pending_item_ops`, `quest_reputation`, `cvars` (registry only), plus the state extracted in step 0 | ~12k + extracted | ~150 |
| `benilla-app-net` | session and wire drain | `net`, `realmlist`, `swing_refusal`, the `ui_native::wrath_*` capture FIFOs | ~25k | ~280 |
| `benilla-app-world` | world, rendering, gameplay | `entities`, `creature_anim`, `player`, `target`, `spell`, `sound`, `portrait`, `minimap`, `transport`, `video`, `cursor`, `death`, `nameplates`, `vplates`, `quest_markers`, `world_state`, `loading_screen`, the effect modules (`aura_visual`, `weapon_trail`, `bowstring`, `fishing_line`, `footprints`, …) | ~136k | ~1.4k |
| `benilla-app-ui` | FrameXML host and feeds, glue screens | `ui_script`, every `ui_*`, `bindings`, `console`, `combat_log`, `char_select`, `char_create`, `realm_select`, `login`, `glue`, `textinput`, `tutorial`, `script_calls`, `experience_ui`, `world_state_ui` | ~234k | ~2.8k |
| `benilla-app-tools` | capture, harness, dev | `capture`, `addon_harness`, `perf`, `debug_panel`, `dev`, `hover_log`, `showcase_capture` | ~35k | ~100 |
| `benilla-app` (root) | assembly | `lib.rs` (`run`, `run_with`), `game_plugins`, the bins, `tests/` | ~1.4k | few |

Dependencies point down only: root → tools → ui → world → net → core. `benilla-ui` (the Lua VM)
becomes a dependency of `benilla-app-ui` only, so world and net builds stop pulling mlua.

Against this layering there are, today, about 1,100 references in about 250 module pairs that
point the wrong way (world → ui 613, net → world 153, core → world 92, core → ui 73, net → ui 72,
net → root 41, core → net 31, ui → root 26). Step 0 removes them.

## Step 0: break the cycles inside the one crate

Every sub-step is an ordinary refactor inside `benilla-app`, gated and merged like any unit, so
the later moves are pure file moves. Each item names the symbols that point the wrong way.

1. **Schedule sets to `core::schedule`.** `UiFeed`, `UiInput`, `UnitFeed`, `PointerOverUi`,
   `CursorPayloadHeld`, `ingame_ui_up`, the `ui_pass` overlay types world draws through
   (`UiQuad`, `UiQuadMaterial`, `UiQuadAppend`, `project_overlay`, `tile_quad_mesh`). Re-export
   from the old paths so callers keep compiling.
2. **Shared game state to `core::state`.** `Spells`, `LearnedAbilities`, `PlayerActions`,
   `UiErrorKeys`, `UiError`, `CastErrors`, `CastFail` (from `ui_action`); `ClientState`,
   `InWorldGated`, `Roster` (`char_select`); `GroupState`, `GROUPTYPE_RAID` (`ui_party`);
   `LootState`, `LootLatch`, `LootConfig`, `LootKneel` (`ui_loot`); `Input`, `BindingsState`
   (`bindings`); `InteractNpc` (`ui_session`); `PetBar` (`ui_pet`); `QuestGiver` (`ui_quest`);
   `GuildState`, `GuildMemberNotify` (`ui_guild`); `ChatLog` (`ui_chat`); `MeetingStone*`,
   `AreaSpiritHealer` (`ui_dialog_verbs`); `UnitTokens`, `CombatTextEvent` (`ui_unit`); `VmMemo`
   (`ui_script`). Types and their pure helpers move; systems stay with their feature.
3. **Movement model to `core::movement`.** `move_flags`, `wrap_pi`, `swim_body_rotation`,
   `HardLanding`, `KitPush`, the player capsule constants and `PlayerCapsule`, `Player`,
   `server_ride`, `may_swim`, `mover::{grounded_step, Support}`, `TransportRider`,
   `TransportAnchor`. `net` needs these to decode and apply movement; the systems that animate
   with them stay in `creature_anim` / `player` / `transport`.
4. **Invert the cvar registry.** `cvars` keeps the store, the parser and a registration API
   (`register_cvar(name, default, on_change)`); each feature registers its own cvars and hook in
   its plugin (`player::camera_view`, `video`, `realmlist`, `ui_gamma`, `ui_loot`,
   `ui_chat::combat`, `ui_guild`, `perf`, …). This removes 92 core → world/ui/tools references.
5. **Capture FIFOs into `net`.** The `ui_native::wrath_*` capture modules that `net` re-exports
   move under `net/` (the UI side keeps its drains).
6. **Tests that build the whole client.** `game_plugins::schedule_tests::headless_client` was
   named from 61 lower-module files (the first count, 44, missed the fixtures that call it, such
   as `ui_pet`'s `rig`). Done: every test that reaches it, directly or through a fixture, lives in
   the root layer's `#[cfg(test)] mod client_tests` under `game_plugins`, one file per source
   module; fixtures that do not build the client stay in their layer. Kept inside the crate
   rather than moved to `tests/`, so nothing became public: the moved code reaches its modules
   through `pub(crate)`, and the cross-crate widening happens once, at the split, from its
   resolved edge list. Test-only back references total 112; the rest are `Spells` (31, fixed by
   item 2) and a handful of `InteractNpc`/`HardLanding`/`QuestGiver`.

Done when the module graph, rerun with the script below, shows no back edge between layers.

### Item 3, part 2: `Player`, the mover and the rest of net → world (design note)

Measured on wrath-hud after part 1 (`e7a0fd7c`, movement model in `app_core::movement`), counting
`crate::<module>` paths with this plan's layer table (`/tmp/azb-split/layers.py`, Wire B):

| Direction | References | Distinct symbols | Largest |
|---|---|---|---|
| net → world | 57 | 33 | `player::Player` 10, `creature_anim::move_flags` 6 (in `net.rs`, kept for the clippy baseline), `go_anim` 7, the mover steps 6, `player::server_ride` 3 (doc links) |
| world → net | 622 (513 outside test files) | 75 | bare `crate::net::{…}` imports 109, `ObjectStore` 95, `GuidIndex` 38, `NetCommands` 29, `NetEntity` 28, `Reputations` 28, `SessionDialect` 31, `SelfPlayer` 29 |

**Inverting the order (core < world < net < ui) is not cheaper.** It would need the 75 world → net
symbols cut, which are net's object model (`ObjectStore`, `GuidIndex`, `NetEntity`, `Guid`,
`SelfPlayer`, `Embodied`), its command channel (`NetCommands`, `ClientCommand`) and its handler
table (`NetHandlerApp`, `handlers::dispatch`): moving those into core is a larger redesign than the
33 symbols the planned order leaves. The plan's order (core < net < world < ui) stays.

**What the 33 are, and the cut for each** (behaviour-neutral unless marked):

1. **`net::motion` is world simulation that lives under `net/`** (5.1k lines: remote
   dead-reckoning, spline sampling, ground clamping, display facing, swim marking). It is where
   every mover call comes from (`grounded_step`, `Support`, `airborne_step`, `fall_step`), and
   `player::mover`'s one net reference (`net::motion::remote`) points back into it. Cut: move its
   component and message types (`RemoteMotion`, `Spline`, `SplineStopped`, `UnitMoveModes`,
   `CreatureSwimming`, `FacingStep`, `DisplayFacing`, `GroundClamped`, `RelayMove`, the
   pending-move queue) to `app_core::movement`, since `net/objects.rs` writes them on create and
   update (47 uses); move the systems and helpers to a world-layer module (`crate::motion`),
   registered by a world plugin in the same sets with the same `.before/.after` edges. The mover
   stays in `player`: with its callers in world, `player::mover` needs no change.
2. **`Player` is a resource `net` resets once**: `activate_connected` clears it synchronously at a
   Wrath `Connected`, before publishing the new endpoint, so the session-end reader cannot stream
   the old pose to the new socket (`net.rs:589-623`). Cut: move the `activate_command_endpoint`
   handler (one `Connected` system) from `net` into `player`, which then calls
   `NetCommands::activate_connected(&endpoints, || *player = Player::default())`; world → net is
   the allowed direction. Kept order: the handler stays where it is in the `Connected` dispatch
   order, so it registers through a named hook that `net`'s plugin places (or `net` keeps the
   registration and takes the reset as a resource-held `fn` that `player` installs). `Player` and
   its impls do not move. The tests that seed `Player` in `net.rs` move with the handler.
3. **`transport::Transport` in the worldport sweep**: `session.rs` asks only `touches_map(map_id)`
   to spare a transport whose path crosses the new map (`session.rs:620-640`). Cut: a core
   component (`TransportMaps`, the maps the path touches) inserted next to `Transport` by
   `arm_transports`, queried by the sweep instead. Behaviour-neutral as long as it is inserted in
   the same frame as `Transport`; pinned by the existing worldport sparing test.
4. **State `net` writes**: `go_anim::{GoCustomAnim, DespawnAnimAnnounced, PendingDestroy}`,
   `world_state::WorldStates`, `spell::Cooldowns`, `target::ring::Factions`,
   `sound::EmoteSounds`, `entities::CollisionHeight` and the four animation components
   (`RangedHold`, `PlaySeq`, `NockLatch`, `BodyTwist`): types to `app_core::state` with item 2.
5. **Callbacks `net` calls**: `death::net::{note_corpse, recheck_corpse, forget_corpse}` and
   `death::DeathNet`, `go_anim::release_despawn_pin`, `cinematic`: turn into messages `net` writes
   and the feature reads, or move the function into the net layer when it only mutates core
   state. Each needs a one-frame-order check; done one at a time with the module's tests.

**Proposed unit (item 3, part 2a): relocate `net::motion`.** Cut 1 alone: types to
`app_core::movement`, systems to `crate::motion` (world), plugin registration moved with identical
sets and edges, `net` re-exports kept so callers compile. Moves only; gated by app `--lib --tests`,
the census and the clippy ratchet, plus a schedule check that the ambiguity counts
(`UPDATE_ACTIONABLE_CEILING`) and system order are unchanged. Cuts 2 and 3 follow as part 2b, one
small design change each, with their tests; 4 joins Assets' item 2; 5 is part 2c.
### Item 3, part A: the movement apply out of `net` (ordering argument, for review)

Status after parts 1, 2b and 2c: net → world is 40 references in 25 symbols. The rest of the
movement cluster is the apply: `net/objects.rs` writes `Transform`, `RemoteMotion`, `Spline`,
`UnitMoveModes` and `TransportRider` inside its handlers, through `net::motion`'s helpers, which
call `player::mover`. A must move that apply to the world layer **without changing what any reader
sees at any point of a frame**.

**The drain today.** `apply_net_updates` (`WorldStage::Net`) calls `handlers::dispatch`, which runs
each event's handlers in registration order. Every handler is `World::run_system_with`, which
applies the handler's deferred commands before it returns, so the next handler, of this event or
the next, sees them. The drain is the head of one `.chain()`: `apply_net_updates`,
`tag_self_player`, `sample_splines`, `mark_swimming_creatures`, `ground_clamp_creatures`,
`drain_pending_moves`, `extrapolate_remote_units`, `drive_display_facing`, with
`ui_session::feed_interact_npc` seated after `apply_net_updates` and before `drive_display_facing`.

**Who reads movement state during the drain** (every `net_handler`, its parameters expanded
through their `SystemParam` bundles; `/tmp/azb-split/handlers.py`): `net/objects.rs`'s object
handlers themselves; the spell handlers (`on_spell_start`, `on_spell_go`, `on_spell_chain_targets`,
`on_spell_failed_other`, `on_cancel_auto_repeat`, `on_item_cooldown`, `on_cast_result`), which read
caster and target `Transform`s; and the combat-log lines, which read `Transform`. So a move applied
earlier in a drain is visible to a `SpellGo` later in the same drain. **A post-drain apply system
would break that** (the `SpellGo` would read the pre-move pose), so A must not defer the apply.

**The design: split each movement handler at the event, not at the frame.** For each event kind
whose handler applies movement (`ObjectCreate`, `ObjectMove`, `MoveTimeSkipped`, `UnitMove`,
`MonsterMove`, `SplineMoveMode`, `ForceSpeedChange`, `SpeedChanged`), `net/objects.rs` keeps the
decode and the bookkeeping (index, store, field messages, self-move routing, despawn), and a world
module (`crate::motion`) registers its own `net_handler` on the same kind with the movement apply,
moved verbatim with `net::motion`'s helpers. Both run in the same event's slot, `net`'s first, its
commands applied before the world handler runs; the event value is the same (each handler gets a
clone). So **after every event the world is exactly what it is today**, and every later event's
handlers and every chain system see identical state. No queue, no deferral.

What has to hold, and how each is checked:

1. *No handler between the two halves reads movement.* A kind's handlers run in registration order,
   and `crate::motion` registers after `NetPlugin`. Handlers on those kinds other than
   `net/objects.rs`: `net/wrath_auras` (`ObjectCreate`, `ObjectsRemoved`, `ObjectDestroyed`),
   `combat_log::on_native_only` and `ui_native::wrath_threat` (`ObjectDestroyed`,
   `ObjectsRemoved`). None reads a movement component: they key aura, threat and log state by guid.
   The two destroy kinds keep their whole handler in `net` (despawning is bookkeeping), so only
   `ObjectCreate` has a third-party handler between the halves, and it does not read movement. A
   schedule test pins the list: a kind's handler table, by name, before and after.
2. *The world half needs nothing the net half kept local.* Each split is a cut at a point where the
   net half's decision (create or update, self or remote, entity found) is either recomputable from
   world state (`GuidIndex`, `SelfGuid`, the entity's components) or carried by the event. Where a
   handler branches on an intermediate it did not store, the split stores it on the entity first
   (one marker, removed by the world half), so both halves take the same branch.
3. *Two command batches instead of one are unobservable.* An `ObjectCreate` now inserts the movement
   components in a second batch. Nothing observes the difference: there is no `On<Add>`/`On<Insert>`
   observer, component hook, or `Added<>`/`Changed<>` filter on `RemoteMotion`, `Spline`,
   `UnitMoveModes` or `TransportRider` (checked by search), and no system runs between two handlers.
4. *The chain keeps its order.* The motion systems move to `crate::motion`'s plugin as the same
   chain, `.after` a `NetApply` set holding `apply_net_updates` and `tag_self_player`, still in
   `WorldStage::Net`; `feed_interact_npc` keeps its two edges against the moved functions. Pinned by
   the schedule tests: `UPDATE_ACTIONABLE_CEILING` and the ambiguity counts unchanged, plus a new
   assert of the chain's system order by name.

**Same-frame create, move and destroy.** In one drain these are three events in wire order, each
fully applied, both halves, before the next: the move finds the entity the create made, and the
destroy despawns it with everything inserted. That is today's behaviour exactly, since today the
same three handlers run in the same order. A new characterization test drives one drain of
create → move → destroy, and one of move → `SpellGo` that reads the caster's pose, through
`handlers::dispatch` with the real plugins. It is written and passing **before** the move, then
kept unchanged across it.

**Tests that pin the rest:** `net/motion/tests.rs` (1.3k lines: relays, extrapolation, splines,
facing), `net/apply/seam_tests.rs`, `net/objects.rs`'s tests (create, spline, stop, remote walk),
the transport tests, and the census and app gates. Every one runs unchanged on the moved code; a
test that needs editing beyond a path is a sign the move changed behaviour, and stops the unit.

**Unit A, as proposed:** (1) land the two characterization tests and the handler-table and chain
asserts on the current tree; (2) in one commit, move `net::motion` to `crate::motion` and split the
eight handlers as above, with the types it shares with `net` (`RemoteMotion`, `Spline`,
`SplineStopped`, `UnitMoveModes`, `RelayMove`, `PendingMove`, `DisplayFacing`, `GroundClamped`,
`CreatureSwimming`, `FacingStep`) in `app_core::movement`, since `net` still writes the bookkeeping
parts. Gates: app `--lib --tests`, the census, the clippy ratchet, and the asserts from (1)
unchanged.

### Progress and the unit 2c clusters

Units 1 (schedule sets), 2a (UI error queues; spell, client, roster, NPC, loot-gate and pointer
state) and 2b (quest giver, input map) are done. Five shared-state types stayed behind because
their fields hold types owned by the UI or world layer; moving one alone would leave `app_core`
pointing back up. Measured (read-only) by following the type names in each definition's code,
transitively, with name collisions checked by hand:

| Root | Cluster to move together | Size | Notes |
|---|---|---|---|
| `GroupState` (`ui_party`) | `GroupState`; `wrath::RaidState`; `saved_instances::NativeSavedInstances`; `pets::Roster` | 4 types, ~70 lines, all `ui_party` | `pets::Roster` clashes by name with `char_select::Roster` (already in `app_core::state`): give the cluster its own module, `app_core::group` |
| `LootState` (`ui_loot`) | `LootState`, `PendingReceive`; `wrath::WrathLootState`, `wrath::Phase` | 4 types, ~60 lines, all `ui_loot` | `app_core::loot`, beside the latch and config moved in 2a |
| `PetBar` (`ui_pet`) | `PetBar`; `spell::cooldowns::{Cooldowns, Record, Timer}` | 4 types, ~50 lines | the cooldown store is world-layer state every layer reads: `app_core::cooldowns` |
| `GuildState` (`ui_guild`) | `GuildState`, `Identity`, `RosterUpdate`; `sort::{SortStack, SortField}` | 5 types, ~70 lines | also reads `query_cache::QueryCache` (core, fine) and `net::SessionDialect`, which Vanilla-removal step 4 collapses; move after that field goes |
| net-inserted animation components (`creature_anim`, from Wire B's movement unit) | `RangedHold`, `NockLatch` (marker components), `PlaySeq` (the packet-order stamp resource); `twist::BodyTwist` with its private `Channel` | 5 types, ~30 lines, all `creature_anim` | `app_core::anim`; with the PetBar batch, after the whole-client test move |
| state `net` writes (Wire B's item 3 part 2, cut 4) | `go_anim::{GoCustomAnim, DespawnAnimAnnounced, PendingDestroy}` to `app_core::anim`; `world_state::WorldStates`, `target::ring::Factions`, `sound::emote::EmoteSounds`, `entities::collision_height::CollisionHeight` to `app_core::state` (`Cooldowns` is in the PetBar row) | 7 types, ~15 lines, each standalone (fields hold formats catalogs or std types) | with the PetBar batch, after the whole-client test move |
| `ChatLog` (`ui_chat`) | `ChatLog`, `Pending`; `event::{ChatEvent, ChatEventKind}`; `broadcast::Broadcast`; `combat::{PendingCombat, Family, Variant, Fills, Named}` and the slot/keying types they hold | ~14 types, ~300 lines, all `ui_chat` | this is the chat event model, not a flag; either move it whole as `app_core::chat`, or keep it in UI and have the five lower callers (death lines, combat log) write a narrower queue. Decide before moving |

Unit 2c waits for the whole-client test move (Experience) because `ui_pet` and `ui_chat/feed.rs`
overlap; Vanilla-removal step 5 takes priority over it.


## Moving a layer out (steps 1 to 4)

Order: `core`, then `net`, then `tools`, then `world` and `ui` (world first, so `ui` can depend
on it). Each step is one commit, done while the workers are paused (see "Coordination").

Mechanics for moving layer `X` with modules `m1 … mn`:

1. `cargo new --lib crates/benilla-app-X`; copy the dependency list of `benilla-app` and prune
   it later with `cargo machete`; add it to the workspace.
2. `git mv crates/benilla-app/src/m1.rs crates/benilla-app/src/m1/ … crates/benilla-app-X/src/`
   for each module (`git mv` keeps blame).
3. In `benilla-app-X/src/lib.rs`: `pub mod m1; … pub mod mn;`.
4. In `benilla-app/src/lib.rs`: replace `mod m1;` with `pub use benilla_app_X::m1;` for each.
   `crate::m1::Foo` paths in the modules still in `benilla-app` keep resolving through the
   re-export, so nothing above the moved layer is edited.
5. Inside the moved files, rewrite references to lower crates:
   `sed -i '' -E 's/crate::(core_mod_a|core_mod_b|…)::/benilla_app_core::\1::/g'` over the new
   crate's files, with the alternation listing only the lower layers' modules. References to
   modules of the same layer stay `crate::`.
6. Visibility: symbols the upper layers use become `pub` (about 115 for core, 114 for net, 277
   for world, 162 for ui, measured downward edges). Let the compiler list them: build, and for
   each `is private` error widen that one item from `pub(crate)` to `pub`. No blanket rewrite:
   the 5k `pub(crate)` items mostly stay crate-local.
7. Plugin registration does not move: `game_plugins` in the root still adds every plugin, so
   system order and run conditions are unchanged.
8. Gate: `cargo check --workspace --tests --examples --benches`, then each crate's `--lib` and
   `--tests`, and `scripts/quality/run.sh clippy -p` for the new crate.

Expected size of each move commit: mostly renames, plus one `lib.rs` per crate, the `sed`
rewrite and a few dozen visibility edits.

Things to update in the same commit:

- `tests/ui_script_wall.rs` and `tests/world_api_wall.rs` scan `crates/benilla-app/src` paths:
  point them at the new crate directories.
- `scripts/quality` and `gates.sh` crate lists, CI matrix, and `docs/WRATH-NATIVE-PLAN.md` gate
  commands (`-p benilla-app` becomes a list).
- Each worker's `CARGO_TARGET_DIR` is unaffected; crate names in their gate commands change.

## Coordination with in-flight work

Measured on each worker branch's own commits over the last three days (files touched in
`benilla-app`, `benilla-ui`, `benilla-protocol`):

| Worker | Areas in flight | Layer |
|---|---|---|
| Wire A | `net` (wire families), `ui_chat`, `spell` | net, world, ui |
| Wire B | `ui_script`, `ui_loot`, `ui_social`, `ui_calendar`, `ui_guild`, `ui_quest`, `ui_world_map`, `benilla-protocol` | ui |
| App Migration | `ui_script`, `ui_macro`, `ui_guild_bank`, `game_plugins`, `spell` | ui, root |
| Native UI | `ui_script`, `ui_quest`, `ui_items`, `benilla-ui` | ui |
| Experience | `spell`, `game_plugins`, formats/assets models | world, root |
| Assets | formats, assets, world crates | outside the app |

- Step 0 sub-steps touch shared types that every worker uses. Do them one at a time, each a
  normal unit with re-exports from the old paths, so in-flight branches merge cleanly; remove the
  re-exports in the matching move step.
- The move steps rename files under everyone's feet. Pause the workers, merge every in-flight
  branch, do the move as one commit on wrath-hud, then let them rebase or merge. A rename
  merges cleanly with git's rename detection when the moved file was not also heavily edited, so
  do each move right after a merge round.
- `ui` is the layer most workers are in; move it last and announce it a unit ahead.

## Re-running the analysis

The numbers above come from a short script over `crates/benilla-app/src`: list the `mod` lines
of `lib.rs`, walk each module's files, count `crate::<module>` references between top-level
modules, then Tarjan's SCC over that graph and a back-edge count against the layer table. It
takes seconds and needs no build. Keep it next to this plan when execution starts so each step's
"no back edges" check is reproducible.

## Risks

- `#[cfg(test)]` helpers shared across layers (fixtures, `test_support`) need a
  `test-support` feature or a small dev-dependency crate; plain `#[cfg(test)]` code is invisible
  across crates.
- Generic Bevy code that relies on `pub(crate)` system-set types across modules surfaces as
  visibility errors in the move steps, not as behaviour changes.
- Step 0 item 4 (cvar registration) changes when hooks are attached; keep the existing order of
  defaults and `on_cvar` calls, and pin it with the existing cvar tests.
- The root crate keeps `game_plugins`, so a whole-client test still rebuilds everything; the
  savings are for edits and tests inside one layer.
- `app_core::errors::UiErrorTexts` carries `benilla_ui::messages::MsgKind` (found in step 0
  unit 2a), so a `benilla-app-core` crate would depend on `benilla-ui` and its mlua build. Before
  the core move, put that plain enum in a small shared crate both can depend on, or mirror it in
  core; it must stay a plain enum.
