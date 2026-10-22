# Splitting wrathplus-app

Analysis and plan only; no code moves yet. The manager schedules execution after the Vanilla
removal. Measured on wrath-hud 3e913bc1 (2026-10-20).

## Why

`wrathplus-app` is one crate: 180 top-level modules, about 443k lines (with tests), about 4.8k lib
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
| `benilla-app-core` | shared state, schedule sets, cvar registry | `run_mode`, `local_state`, `query_cache`, `area`, `chr_classes`, `text_filter`, `text_reshape`, `glue_strings`, `crash`, `shutdown`, `pending_item_ops`, `quest_reputation`, `cvars` (registry only), `launch_plan` (the run's command line and environment), `app_core` (the state extracted in step 0) | ~12k + extracted | ~150 |
| `benilla-app-net` | session and wire drain | `net` (with the capture FIFOs), `realmlist`, and the wire-query caches `names`, `items`, `go_templates`, `area_trigger`, `name_persist`, `creature_type` (reassigned after the move dry run) | ~25k | ~280 |
| `benilla-app-world` | world, rendering, gameplay | `entities`, `creature_anim`, `player`, `target`, `spell`, `motion`, `swing_refusal`, `sound`, `portrait`, `minimap`, `transport`, `video`, `cursor`, `death`, `nameplates`, `vplates`, `quest_markers`, `world_state`, `loading_screen`, the effect modules (`aura_visual`, `weapon_trail`, `bowstring`, `fishing_line`, `footprints`, …), the 2-D primitives (`overlay`: the quad types and model tiles; `ui_text`, the text engine), the guild session `guild`, and `game_tip` (the loading screen's tip) | ~136k | ~1.4k |
| `benilla-app-ui` | FrameXML host and feeds, glue screens | `ui_script`, every `ui_*` but `ui_text`, `bindings`, `console`, `combat_log`, `char_select`, `char_create`, `realm_select`, `login`, `glue`, `textinput`, `tutorial`, `script_calls`, `experience_ui`, `world_state_ui`, `shipped_cvars` (every row source sits at or below ui) | ~234k | ~2.8k |
| `benilla-app-tools` | capture, harness, dev | `capture`, `addon_harness`, `perf`, `debug_panel`, `dev`, `hover_log`, `showcase_capture`, `preflight`, `probe_shield` | ~35k | ~100 |
| `wrathplus-app` (root) | assembly | `lib.rs` (`run`, `run_with`), `game_plugins`, the bins, `tests/` | ~1.4k | few |

Dependencies point down only: root → tools → ui → world → net → core. `wrathplus-ui` (the Lua VM)
is a separate crate below all of them: core depends on it (decision of 2026-10-21, below), so
every layer builds mlua once and a world feed taking `UiScript` is not a layer break.

**Decision (2026-10-21): core depends on `wrathplus-ui`.** `app_core::vm_memo::VmMemo` holds
`UiScript`, `app_core::errors` carries `MsgKind` and `app_core::interaction` a `WhoSortChain`, and
the world feeds (area, cinematic, cursor, minimap, sound::ui, spell::unit_cast,
spell::targeting::item, video, vplates) take `UiScript`. `wrathplus-ui` is already its own crate
below `wrathplus-app`, so these are downward edges; nothing moves up to avoid them.

Against this layering there are, today, about 1,100 references in about 250 module pairs that
point the wrong way (world → ui 613, net → world 153, core → world 92, core → ui 73, net → ui 72,
net → root 41, core → net 31, ui → root 26). Step 0 removes them.

## Step 0: break the cycles inside the one crate

Every sub-step is an ordinary refactor inside `wrathplus-app`, gated and merged like any unit, so
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
   Done: the hooks already were each feature's `on_cvar` observer, so only the rows moved. 47 of
   the 138 rows live in their 15 features as `CVARS` and register through
   `cvars::RegisterCvars` in the feature's plugin; the core table keeps 91 that name no feature
   (`fpsJournal` among them, since `perf` registers only in `dev`). Static users (the VM seeds,
   the addon harness, the whole-table tests) read `shipped_cvars`, the aggregate the split puts
   with the UI host; `client_tests::shipped_cvars` pins it to a built client's rows, the latched
   rows' commit order and the 22 hooks' registration order.
5. **Capture FIFOs into `net`.** The `ui_native::wrath_*` capture modules that `net` re-exports
   move under `net/` (the UI side keeps its drains). Done: `net` names no `ui_native` module.
   Seven families split into a `net/` capture half (FIFO, state, handlers, session clear) and a
   `ui_native` feed of the same name: quests completed, UI timer, appearance, quest POIs, death,
   movie, threat. The reducer feeds' state moved into the capture module that clears it:
   `Achievements` (achievement capture), `ItemRefunds`, `EquipmentSets`, `SocketTemplates` and
   `SocketSession` (inventory capture), `RaidBrowser` (social lists). The feeds register from
   `ui_native::NativeFeedsPlugin`, after `UiNetPlugin`, and re-export the moved types under their
   old paths. Left: `QuestPois::view` and `queries` stay an inherent impl in the feed (an
   extension trait at the split); `ThreatStore` wraps `wrathplus_ui::script::ThreatTable`.
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
   `combat_log::on_native_only` and `net::wrath_threat` (`ObjectDestroyed`,
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
state), 2b (quest giver, input map) and 2c (below) are done. Five shared-state types stayed behind because
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

Status: every row above has moved (2c part 1 group and loot, part 2a cooldowns, anim and the
net-written state, part 2b `app_core::chat`, moved whole as decided) except `GuildState`, which
waits for App's last step-4 batch. What is left of step 0 is in "Step 0 residue" below.

## Step 0 residue

Re-measured on wrath-hud after units 1, 2a, 2b and 2c (head afa6f7f0, 2026-10-20), with
`app_core` in the core layer and `shipped_cvars` (Experience's cvar unit; it lists every
feature's rows) in the root layer. A reference that resolves through `pub(crate) use` chains to a
type now homed in a lower layer is not a back edge: the move step only rewrites its path.

| Layer pair (from → to) | Baseline | Now |
|---|---|---|
| world → ui | 613 | 196 |
| net → world | 153 | 10 |
| core → world | 92 | 48 |
| core → ui | 73 | 45 |
| net → ui | 72 | 35 |
| net → root | 41 | 0 |
| core → net | 31 | 23 |
| ui → root | 26 | 16 |
| core → root | – | 19 |
| core → tools | 11 | 10 |
| tools → root | – | 7 |
| others | 7 | 2 |
| **total** | **~1,119** | **411** (20 in tests) |

Another 423 references (world → ui 361, net → ui 27, net → world 17, core → ui 13, core → world 5)
already resolve to `app_core` through the old paths' re-exports: the move steps rewrite them to
`benilla_app_core::…` mechanically.

### Checklist by owner

**In flight**

- Wire B, movement (item 3): `player::{server_ride, mover::{grounded_step, airborne_step,
  fall_step, Support}}`, `transport::{Transport, tick_transports}`, and the `app_core` references
  to `net::motion::spline::mark_swimming_creatures` and `creature_anim::EmoteAnim` (13).
- Assets, `GuildState` (after App's last step-4 batch): `ui_guild::GuildState` from `entities`
  and `nameplates` (3); `GuildMemberNotify` goes with it.

**Done (Wire A): producer-less native writers.** Of the seven Wrath writers Wire B's dead-field
audit (d20aeba3) left under `#[allow(dead_code)]`, five now have their 12340 producers
(`DeclineChannelInvite`, `ClearChannelWatch`, `NativeParty`, `NativeMirrorImageQuery`,
`MoveTimeSkipped`; see the coverage rows) and two are deleted: `NativeChatCommand` duplicated the
shared `CMSG_MESSAGECHAT` writer, and Wow.exe never sends `GetChannelMemberCount`.

**The cvar registry (item 4 follow-up, 121): needs a short design note before moving.** After
Experience's unit the rows register from their features, but `cvars` still names each feature's
config resource and `on_cvar` hook (`video::VideoConfig`, `realmlist::Realmlist`,
`ui_gamma::DisplayGamma`, `perf::FpsJournalSetting`, `player::camera*`, the 26 `on_cvar` calls,
…), and `shipped_cvars` is read from core (`cvars`), UI (`ui_script`) and tools (`capture`,
`addon_harness`). Finishing item 4 means each feature registering its hook as well as its rows,
and the static table being passed in by the root instead of named.

**Design note (item 4 follow-up).** The registry ends up owning only the store, the parser, the
file and the VM mirror. A feature contributes three things through the same registration
trait (`cvars::RegisterCvars`), all from its own plugin:

1. *Rows*: done, `register_cvars(app, CVARS)`.
2. *Hooks*: production code already registers these, since each feature's plugin adds its own
   `on_cvar` observer. The 26 names left are the whole-table tests in `cvars::tests`: their
   `ALL_OBSERVERS` list, the feature resources the rig inserts by hand, and the
   `FEATURE_ROWS` loop. Those tests need every feature, so they move to
   `game_plugins::client_tests::cvars` (the root layer), unchanged, with the 138-row, latched-order
   and hook-order pins. `cvars` keeps only the tests that use its own rows (`Cvars::with_rows`).
3. *Session levers*: new. `session_values` reads `$WOW_*` levers off feature resources
   (`video::VideoConfig`, `realmlist::Realmlist`, `world_backdrop::RenderScale`,
   `ui_script::UiScaleCvar`, …). Each becomes a `SessionLever { env, rows, read: fn(&World) ->
   Vec<(&str, Option<String>)> }` that the feature registers with `register_session_lever`, and
   `cvars` iterates the pending list in registration order. Benilla-world and wrathplus-assets
   levers (farclip, clutter, MSAA, texture filter) stay in `cvars`, since those crates sit below
   it.

The static table is passed in, not named. `shipped_cvars` stays in the root. A VM's rows come
from the world's `Cvars` registry, which holds all 138 rows once the plugins have built. That is
already the live path in `ui_script::lifecycle`. Its "no registry" fallback drops to core rows,
which only a stripped test world reaches. The tools and rigs that build a VM without an
app (`load_default_ui`, `addon_harness`, `capture::fixtures`, the census) take
`&[(&str, &str)]` from their caller. Root-layer callers pass `shipped_cvars::pairs()`. A UI-layer
test that needs the feature rows takes them from its root-layer caller, or moves up into
`client_tests` when the crates split. Remaining `cvars` → feature references:
the `ui_script` VM-mirror seam (`VmMemo`, `UiInput`, `end_ui_session`), which goes with
`app_core::state` as a pure move. Pins: `client_tests::shipped_cvars` (138 rows, `LATCHED_ORDER`,
`HOOK_ORDER`) plus a new one: the lever list equals today's `session_values` rows in
order.

Done (Experience):
- `cvars` names no feature now. Its production references are `app_core` (`vm_memo::VmMemo`, moved
  from `ui_script/session.rs`, and `schedule::UiInput`), `shutdown` and `local_state`.
- The whole-table tests moved to `client_tests::cvars` (21 of the 23); `cvars` keeps 2.
- Levers: `CORE_LEVERS` holds farclip, clutter, trilinear, aniso and MSAA. `video`, `realmlist`,
  `world_backdrop` and `ui_script` each register theirs.
- The `client_tests::shipped_cvars` lever pin fixes the order to the real one: core first, then
  plugin order. Each row is owned on its own, so the order fixes only the log's. The rows are the
  same 13.
- The capture fixtures read the live registry (`Cvars::defaults`), and the lifecycle's no-registry
  fallback registers only the core rows.
- Two production readers of `shipped_cvars` are left: `load_default_ui` and `addon_harness`. Their
  table parameter comes after App's step 4b. The UI-layer tests' reads stay until the split.

**Pure moves (~150 references, definitions only):**

| Kind | Symbols (from) | Home |
|---|---|---|
| open-window and interaction state | `TrainerOpen` (ui_trainer), `MailOpen`, `Stationery`, `Packages` (ui_mail), `GuildBankOpen`, `ItemTextOpen`, `LetterTexts`, `PageMaterials` (ui_item_text), `GossipState`, `BinderState`, `MeetingStone`, `MeetingStoneUse`, `AreaSpiritHealer`, `AREA_SPIRIT_HEALER_AURA` (ui_dialog_verbs), `FlightMasterStatus` (ui_taxi), `TradeSkillOpens`, `MerchantOpen`, `SocialState`, `BindGate`, `TabardDesign`, `GoOpener`, `GoOpenerCasts`, `PlayerActions`, `SPELL_ATTACK` (ui_action), `ItemUse`, `ItemSearch`, `InventoryScope`, `ItemSubClasses` (ui_items), `UnitTokens`, `CombatTextEvent` (ui_unit), `LootKneel`, `LootMoveStart` (ui_loot), `AuraDurations`, `SkillLines`, `LearnedInTab`, `GROUPTYPE_RAID`, `PARTY_PET_TOKENS`, `AfkMirror`, `DURABILITYDAMAGE_DEATH`, `TutorialEvent`, `tutorial::id::LOOTING`, `InputHooks`, `InspectMode`, `UiScaleCvar`, `PendingEntryUiLoad`, `UiKeyboardCapture`, `PlayerUiClickConsumed`, `PlayerUiHover`, `UiClock`, `GlueCanvas`, `Realms`, `LoginIntent` | `app_core::state` |
| catalogs the entity spawn reads | `PetFamilyTables`, `PetStatTables` (ui_pet_stats), `RepairTables` (ui_merchant), `BankPrices` (ui_bank), `StableSlotPrices` (ui_stable) | `app_core::catalogs` |
| native packet queues `net` fills | `NativePetNamePackets`, `NativeBattlegroundRest`, `ArenaRosterRefresh`, `NativeWhoPackets`, `NativeTotems`, `combat_log::native::NativeCombatLog` | the `net` layer, beside Wire B's FIFOs. Done (Wire B): the pet-name and WHO FIFOs moved whole; App's 4a took the arena lines out of `net`, so it names neither BG queue; the spell-log and spell-control captures keep only their FIFOs, and `combat_log` and `ui_totems` record steals and totems from their own handlers on the same kinds. |
| UI overlay drawing world code uses | `ui_pass::{UiQuad, UiQuadMaterial, UiQuads, UvRect, PlayerUiCamera, PlayerUiPlugin}`, `ui_models::UiModelTiles`, `ui_text::TextSeat`, `area_poi::AreaPoiRes` | a world-layer overlay module (the quads are drawn by world systems) |
| net and player items lower layers name | `net::{SelfPlayer, ObjectStore, NetOffline, Embodied, DisconnectedMessage, EnteredWorldMessage}`, `player::{Player, UNIT_FLAG_IN_COMBAT}`, `creature_anim::UNIT_FLAG_DISARMED`, `target::Selection`, `console::{ConsoleEcho, ConsoleCommandApp}`, `capture::ProbeAuctionPlugin` | `app_core::state` (resources, markers, constants); the plugins stay and their users move up. Done (Wire B): the `net::*` half is in `app_core::session`, re-exported from `net`. |
| test fixtures | `ui_items::TestObjects` and the other test-only references (20) | a `test-support` module or feature |

**Needs design (~93 references): calls from a lower layer into a higher one.** Each wants an
event, a message the higher layer drains, or the caller moving up:

- `ui_script::VmMemo` (17): it holds the Lua VM; it is `app_core::vm_memo` now, core depending on
  `wrathplus-ui` (the 2026-10-21 decision under Proposed crates), so its world users (area,
  cinematic, cursor, death, minimap, sound, spell, video, vplates) are downward edges.
- UI helpers world code calls: `ui_items::{find_item, carried_counts, slot_guid,
  collect_inventory, send_item_use}`, `ui_session::feed_interact_npc`,
  `ui_merchant::end_repair_mode_on_right_press`, `ui_loot::{close_interaction,
  resolve_loot_kneel, begin_wrath_corpse_loot, drain_loot}`, `ui_action::{show_messages,
  attack_actor_blocked, attack_nearest_probe, cast_fail::from_wrath,
  drop_item::drop_item_on_unit}`, `ui_unit::{snapshot, race_names, class_names,
  player_token_guid}`, `ui_aura::buffs_visible_on`, `ui_cast::feed_cast_bar`,
  `ui_pet::pet_stop_on_old_target_clear`, `script_calls::{apply_script_calls, in_call_order}`,
  `ui_guild::{unit_guild_name, unit_guild_emblem, corpse_guild_emblem}`,
  `ui_world_map::world_enter_selection`, `ui_macro::identity`.
- Run conditions and drawing helpers: `ui_script::{ingame_ui_up, seam_scale,
  load_stock_bindings}`, `ui_pass::{project_overlay, tile_quad_mesh, overlay_z,
  ui_render_layers}`, `ui_models::spawn_warm_tile_cam` (with the overlay module above).
- `net` calling UI drains: `ui_party::net::{member_deactivated, roster_deactivated}`,
  `ui_trade::answer_trade_request`, `ui_battlefield::drain_native`,
  `char_select::char_login_refusal_text`, `combat_log::chat::faction_standing`.
- Core calling up: `net::apply_net_updates` (query_cache), `spell::mods` (chr_classes),
  `perf::stall` (local_state), `capture::{ui_opted_in, start_state, probe_rig,
  probe_uncap_mode}` (run_mode, probe_shield, video), `game_tip::{drive_game_tip, tip_bundle}`
  (loading_screen), `entities::item_glow` and `creature_anim::disarmed_hand` (items),
  `combat_log::text::{melee_center_text, classify_source}`.

**Proposed mechanism per group (draft, for batching).** Four kinds: *move down* (a pure function
or catalog lookup that only looks like UI; a pure move), *drained message* (the lower layer writes
a Bevy `Message` and the UI system drains it, as `UiError` already works), *event* (a core
resource or run condition the higher layer sets and the lower one reads), and *move the caller up*
(the system is a UI feed that lives in a world module). App Migration owns the world calls into
`ui_items`, `ui_loot` and `ui_action` and the `net` calls into UI (`member_deactivated`,
`roster_deactivated`, `answer_trade_request`, `ui_battlefield::drain_native`,
`char_login_refusal_text`, `faction_standing`) and `end_repair_mode_on_right_press`, and writes those
mechanisms itself ("App Migration's groups").

| Group | Mechanism | One line |
|---|---|---|
| `VmMemo` (17) | move the caller up | Every world user is a Lua feed that already takes `UiScript`; move those systems into a UI-side feed module beside their world state, and `VmMemo` stays in `ui`. |
| lookups: `race_names`, `class_names`, `player_token_guid`, `unit_guild_name`, `unit_guild_emblem`, `corpse_guild_emblem`, `buffs_visible_on`, `ui_macro::identity` | move down | Pure tables or reads over core state; move beside their data in `app_core`. |
| other action requests: `feed_interact_npc`, `pet_stop_on_old_target_clear`, `world_enter_selection`, `apply_script_calls` / `in_call_order` | drained message | World writes a request message and the UI system that owns the session drains it in its existing set; check the frame order per row. |
| UI feedback from world: `feed_cast_bar`, `ui_unit::snapshot` | drained message | A cast-bar message the UI drains; `snapshot` reads move down with the lookups. |
| run conditions: `ingame_ui_up`, `seam_scale`, `load_stock_bindings` | event | `ui_script` sets a core `InGameUi` resource (up flag) that world reads; `seam_scale` is a pure fn and moves down; `load_stock_bindings` moves its caller (vplates) up. |
| drawing helpers: `project_overlay`, `tile_quad_mesh`, `overlay_z`, `ui_render_layers`, `spawn_warm_tile_cam` | move down | Go with the world-layer overlay module (the pure-moves row). |
| core up, pure: `creature_anim::disarmed_hand`, `entities::item_glow`, `combat_log::text::{melee_center_text, classify_source}`, `spell::mods` (chr_classes) | move down | Pure functions or catalogs; move into `app_core` with their tables. |
| core up, wiring: `net::apply_net_updates` (query_cache), `perf::stall` (local_state), `capture::{ui_opted_in, start_state, probe_rig, probe_uncap_mode}`, `game_tip::{drive_game_tip, tip_bundle}` | event / move the caller up | `query_cache` and `local_state` read a core resource the owner fills; the capture and game-tip calls are tool or world wiring and move up into the plugin that registers them. Done (Wire B) except `game_tip`: `query_cache` orders after `app_core::schedule::NetDrain` (the drain alone); `run_mode` asks a `DevSeam` the root installs at launch (`dev::install_run_mode_seam`); the probe and stall references were doc links, now prose. `net::session`'s pet-name and WHO queues moved with their whole FIFO modules to `net::{wrath_pet_name_capture, wrath_who_capture}`. |

#### App Migration's groups: proposed mechanisms (2026-10-20)

Taken by App Migration: the `ui_items`, `ui_loot` and `ui_action` helpers world code calls, and
`net` calling UI drains. Each lands as a behaviour-neutral unit with the usual gates.

| Group | References | Mechanism |
|---|---|---|
| Inventory walkers | `ui_items::{find_item, carried_counts, slot_guid, collect_inventory}` (+ `ItemSearch`, `InventoryScope`) from `death`, `spell::usable`, `spell::targeting::item`, `target::{lock, click}` | **Move down.** They are pure over the descriptor and `net::Objects`, so they can't go to core: a world-layer `inventory` module holds them and their two types; `ui_items` calls down to it. |
| Item-use record | `ui_items::ItemUse` from `app_core::state::GoOpener::Key` (core) and `target::click` | **Move down.** A plain record, so it joins `GoOpener` in `app_core::state`, as the step-0 table lists; `ui_items` re-exports it. Done. |
| Item use from the death drain | `ui_items::send_item_use` from `death::drain_death` | **Move the caller up.** `drain_death` takes the `UiScript` and drains Lua requests (the soulstone use), so it is UI-layer code: it moves to a `ui_*` module, which also takes `death`'s `UiScript` edges. Done: `ui_death` holds the feed, the drain and their tests; `death` keeps `DeathNet`, the net handlers and the ghost look. |
| Loot state machine | `ui_loot::{close_interaction, begin_wrath_corpse_loot, resolve_loot_kneel}` from `creature_anim` and `target::click` | **Move down.** They touch only `LootState`, `LootLatch` and `NetCommands`: the loot state machine and its transitions move to a world-layer `loot` module; `ui_loot` keeps the Lua feed and drain. The `.after(resolve_loot_kneel)` ordering then names a world system. Done: `loot` holds the three and `LootKneel` (`ui_loot` re-exports them for its own tests and drain); `creature_anim`, `target::click` and `go_anim` name `crate::loot`. |
| Attack predicate, cast-fail map | `ui_action::attack_actor_blocked` from `target::click`; `ui_action::cast_fail::from_wrath` (+ its key tables) from `spell::net` | **Move down.** A predicate over the player's store, to `target`; a pure wire map, to `spell`. `from_wrath` maps 12340 reasons onto 1.12 codes and goes away when the cast-fail codes are native. |
| UI systems in the target chain | `script_calls::apply_script_calls`, `ui_action::drop_item::drop_item_on_unit`, `ui_action::attack_nearest_probe`, scheduled inside `target`'s plugin chain | **Move the caller up.** `target` exposes a `SystemSet` at each slot; the UI modules register their own systems into it, with the same chain order. |
| Arena lines | `net::wrath_arena_lines` runs in `ui_script::UiFeed` `.after(ui_battlefield::drain_native)` | **Move the caller up.** It is a UI feed: its registration (or the file) moves to the UI layer beside `ui_battlefield`. Lands with the step-4 batch, which also edits `net.rs` (its clippy baseline entry is keyed on the file hash). |
| Group presence | `ui_party::net::{member_deactivated, roster_deactivated}` from `net::{objects, session}` | **A drained message.** `net` writes the deactivated guid or roster; `ui_party` drains it the same frame, before anything reads `GroupState`, in `net`'s order. Done: `net::MemberDeactivated` carries the member's and the held pet's cloned stores (the despawn would drop them), written only for a roster member; `ui_party::net::on_member_deactivated` drains it as a handler on the same three kinds, registered after `net`'s, so the snapshot and stats request keep their place in the dispatch. A whole-client test pins the order. |
| Reputation lines | `combat_log::chat::faction_standing` from `net::session` | **A drained message carrying the old and new standing.** Today it runs before the store is overwritten; the message keeps that, and `combat_log` composes the lines. Done: `net::StandingChanged { list_id, old, new }` is written before the overwrite; `combat_log::FactionStandingPlugin`, added right after `NetPlugin` (the `CombatLogPlugin` precedes it), drains it as a handler on the same kind, so the line keeps its place among the dispatch's chat lines rather than trailing them after `NetDrain`. A whole-client test pins the handler order, the pre-overwrite `old` and the overwrite. |
| Doc links | `ui_trade::answer_trade_request`, `char_select::char_login_refusal_text`, `ui_action::show_messages`, `ui_loot::drain_loot` named from `net`, `sound` | **Plain text.** Intra-doc links count as references; they become prose. The `sound` and `net.rs` ones are done, and the new modules link no UI module. |

#### Native UI's groups (claimed 2026-10-21)

Taken by Native UI: the UI-side design groups of the table above that App Migration did not claim.
Each lands as a behaviour-neutral unit with the usual gates, in this order.

| Group | References | Mechanism | Status |
|---|---|---|---|
| Run conditions | `ui_script::{ingame_ui_up, seam_scale, load_stock_bindings}` | **Move down.** `ingame_ui_up` reads only `PendingEntryUiLoad` and `ClientState`, both already core, so the run condition itself moves to `app_core::ui_state` (no new resource, no frame of lag); `seam_scale` is pure and goes with it; world callers name the core path, `ui_script` re-exports both. `load_stock_bindings`' one world caller was a `vplates` test; the 12340 nameplate port (Assets, 71fed466) replaced it with a client test, closing the edge. | done |
| Drawing helpers | `ui_pass::{project_overlay, tile_quad_mesh, overlay_z, ui_render_layers}`, `ui_models::spawn_warm_tile_cam` | **Move down.** The four `ui_pass` helpers are a new world-layer `overlay` module (`ui_pass` re-exports `ui_render_layers`, which it still uses); `spawn_warm_tile_cam` goes to `portrait` beside its twin `spawn_warm_booth`, with the tile atlas's first edge and camera order. chat_bubble, combat_text, minimap::composite, pipe_warm and vplates name the new paths. | in review |
| Cast-bar feedback | `ui_cast::feed_cast_bar` from world | **Drained message.** The queue world already writes, `CastBarEdge`/`CastBarFeed`, moves to `spell::cast_bar`; `ui_cast` keeps the drain in its slot (`UnitFeed`, after `spell::LocalCancel`), the same frame, pinned by `a_queued_edge_reaches_the_vm_the_same_frame`. The four Esc tests that ran `feed_cast_bar` from `spell::inflight` move to `ui_cast_esc_tests.rs` over a shared `spell::cast_test_support` harness. | in review |
| Other action requests | `ui_session::feed_interact_npc`, `ui_pet::pet_stop_on_old_target_clear`, `ui_world_map::world_enter_selection`, `script_calls::{apply_script_calls, in_call_order}` | Narrower than the dry run by now. `InteractNpc` is already `app_core::state`: world code (target::click, creature_anim, motion::facing, portrait, capture) names that path, and `target::click`'s re-click test, which runs `feed_interact_npc`, moves to `ui_session`'s tests. `pet_stop_on_old_target_clear` is ordered against by `spell::targeting::feed_targeting_to_vm`, itself a `VmMemo` user: it moves up with the `VmMemo` row. `world_enter_selection` and the `script_calls` links in target and spell docs are prose. `apply_script_calls` in the target chain was App Migration's split unit 1. | done (`pet_stop_on_old_target_clear` with `VmMemo`: its one world reader moved up) |
| `VmMemo` (17) | `ui_script::VmMemo` users in area, cinematic, cursor, death, minimap, sound, spell, video, vplates | `VmMemo` is `app_core::vm_memo` now, so a `ui_script::VmMemo` path is a re-export path that `scripts/split/canonical-paths.py` rewrites at move time, and `ui_party::GroupState` likewise. Done: `spell::targeting::feed_targeting_to_vm` moved up to `ui_cast::spell_targeting` (its plugin keeps the `UnitFeed`, old-target-clear and aura-feed order), and its four VM tests went with it onto `spell::targeting::test_support`. `UnitTokens` moved down to `target::unit_tokens` (world state only), and `raid_row_guid(s)` to `app_core::group`. Not inversions: `bindings` and `tutorial` are UI-layer, so the bindings loaders and `TaxiState` (read by `tutorial`, `ui_session`, `capture`) stay. The other world feeds that take `UiScript` (area, cinematic, cursor, minimap, sound::ui, spell::unit_cast, spell::targeting::item, video, vplates) hold `wrathplus-ui` in world; they stay: core depends on `wrathplus-ui` (the 2026-10-21 decision under Proposed crates). The death row is App's. | done |

When the pure moves and the in-flight items land, the move phase can start with `core` and `net`;
`world` and `ui` wait for the design items above.

### Move dry run: core and net (Wire B, 2026-10-21)

On a throwaway detached worktree of wrath-hud f051c2e5 (which predates 563f0119), the moves were done
as "Moving a layer out" describes. The plan's 22 core modules were `git mv`'d into
`crates/benilla-app-core`, `#[path]` test files included, with `pub use benilla_app_core::m;` in the
root and the root depending on the crate. Then `cargo check -p benilla-app-core --lib --tests` was
run. A second pass moved core and net together (25 modules: the 22 plus `net`, `realmlist` and
`swing_refusal`) into one crate, so net's own edges show without core compiling first. rustc stops
after name resolution, so the compiler counts below are resolution errors only. The later-phase
causes (inherent impls, visibility, test helpers) were measured statically on the same tree.

**Compiler, core alone: 45 distinct error sites** (82 with the lib and lib-test duplicates).

| Cause | Sites | What | Owner |
|---|---|---|---|
| Path through an upper module's re-export of an `app_core` item | 17 | `char_select::ClientState` (6), `ui_script::{VmMemo, UiFeed, UiInput}`, `char_select::{InWorldGated, Roster}`, `target::Factions`, `ui_action::Spells`, `player::{Player, UNIT_FLAG_IN_COMBAT}`, `creature_anim::UNIT_FLAG_DISARMED`, `entities::CollisionHeight` | The move step, mechanically. The plan's step-5 `sed` rewrites module prefixes only; it needs a symbol map instead (108 `pub(crate) use crate::app_core::…` re-exports today) that turns `crate::<upper>::X` into `crate::app_core::<m>::X`. |
| Fixed since the dry-run base | 6 (and part of 2 more) | `net::{ObjectStore, SelfPlayer, NetOffline, EnteredWorldMessage, DisconnectedMessage}`, now in `app_core::session` | Done, 563f0119 |
| core → net command API | 13 (4 in `names`' tests, 1 follow-on) | `names`, `items`, `go_templates` and `area_trigger` send through `ClientCommand`/`NetCommands`, key on `Guid`/`GuidIndex`/`Objects`, and register with `NetHandlerApp`; `names` reads `net::wrath_pet_name_capture`; `preflight` and `probe_shield` read `SelfGuid` and `ServerSaidMessage` | Proposed below |
| core → world/ui lookups | 6 | `ui_unit::{snapshot, race_names, class_names}` (`creature_type`, `preflight`), `ui_macro::identity` (`name_persist`), `creature_anim::disarmed_hand` (`items`, 2) | The "lookups" and "core up, pure" rows above; unowned |
| core → root | 1 | `crate::BuildId` (`crash`) | Done (Assets, pure moves 2): `crash` names `wrathplus_world::build_id::BuildId`, where it is defined; the root keeps its re-export for the launcher API |
| Pure-move rows not done yet | 1 | `login::LoginIntent` (`probe_shield`) | Done (Assets, pure moves 2): `app_core::ui_flags` |
| `cfg(test)` helper across layers | 1 | `app_core::vm_memo` tests use `crate::test_support` | See the test-helper item below |

**Compiler, net (core+net crate, net files only): 43 distinct sites.**

| Cause | Sites | What | Owner |
|---|---|---|---|
| Re-export path to an `app_core` item | 27 | `world_state::WorldStates`, `go_anim::{GoCustomAnim, DespawnAnimAnnounced, PendingDestroy}`, `ui_party::GroupState`, `ui_quest::QuestGiver`, `target::ring::Factions`, `ui_chat::ChatLog`, `spell::Cooldowns`, `ui_action::{UiError, UiErrorKeys, Spells, FillArg}`, `creature_anim::PlaySeq`, `ui_script::UiInput`, `ui_unit::UnitFeed`, `char_select::ClientState` | The move step's symbol map |
| Fixed since the base | 2 | `ui_totems::NativeTotems`, `combat_log::native::NativeCombatLog` | Done, 563f0119 |
| `net` drains UI state | 4 | `ui_party::net::{member_deactivated, roster_deactivated}`, `combat_log::chat::faction_standing` | App Migration's groups |
| `net` installs and re-exports `crate::motion` | 5 | `MotionPlugin` (installed by `NetPlugin`), the `drive_display_facing`/`Spline`/… re-exports | Wire B: `game_plugins` installs `MotionPlugin` and the callers repoint (the ui_session ordering test then adds it) |
| Pure move not done | 3 | `ui_item_text::LetterTexts` (`wrath_inventory_capture` and its test) | Done (Assets, pure moves 2): `app_core::interaction` |
| `swing_refusal` → world | 2 | `creature_anim::{AttackSeam, Engaged}` (plus one `chain` follow-on error) | Proposed: `swing_refusal` is the attack seam's feedback, a world module, not net |

**Static, both layers** (what rustc reports only after resolution succeeds):

- *Inherent impls on a moved type*: 38 `impl T {}` blocks in upper layers on 34 types defined in core or
  net, which become E0116 after the split. Most are step 0's state types, whose helpers stayed with the
  feature: `Spells` (2), `GroupState` (4), `ChatLog` (2), `BindingsState` (2), `Player` (2), `LootState`,
  `LootLatch`, `WrathLootState`, `QuestGiver`, `PetBar`, `Roster`, `UiError`, `CastErrors`,
  `CastFail`, `UiErrorTexts`, `Cooldowns`, `Timer`, `Factions`, `WorldStates`, `EmoteSounds`,
  `ConsoleEcho`, `PlaySeq`, `BodyTwist`, `Channel`, `MoveModes`, `ChatEvent`, `ChatEventKind`,
  `Family`, `Variant`, `NativeSavedInstances` and `QuestPois` (item 5's feed). These were matched by
  name and need a check per row. Fix per row: the impl moves with the type when it is pure (the
  plan's rule), or becomes an extension trait in the feature. Done (Wire B, taken over from Assets):
  21 impls moved whole with their types, with the pure items they needed (`UnitClass`, `ListOutcome`,
  `CooldownInfo`, `GROUPTYPE_RAID`, `GROUP_MEMBER_SUBGROUP`, the loot-candidate and fall constants).
  The other 17 moved too except 13 methods that need their feature (the Lua VM, `SpellModifiers`,
  `BindingInput`, `ArcEdges`, `move_trace`, the chat broadcast and combat queues, `LootAction`, the
  push-container map). Those are eight extension traits beside the feature: `BindingsScriptInput`,
  `ChatEventScript`, `ChatLogFeed`, `CooldownsSpell`, `LootStateRows`, `PlayerArc`, `PlayerSettle` and
  `QuestPoisView`. Assets' pure moves 2 added 26 more (the `interaction` and `ui_flags` types,
  `AuraDurations`, `AfkMirror`, `TutorialEvent`, and the chat types after Experience trimmed them):
  all moved, `SetHealerOutcome` with them, except `GossipGreeting`, `AuraDurationsTrace`,
  `RealmsCategory` and `RealmsSelect` (realm_select's helpers and `net::{RealmChoice, RealmRequest}`).
  The matcher now finds no inherent impl on a moved type.
- *Visibility*: 428 distinct `crate::<core|net module>::…` symbols named from upper layers (254 core,
  174 net; an upper bound, since it counts re-export paths and module paths). The plan's estimate was
  about 115 and 114. Each still widens to `pub` only when the compiler names it.
- *`cfg(test)` helpers across the boundary*: upper-layer tests use 7 core or net `tests::` helpers from
  9 files (`net::…::tests::dispatch` and similar, from `game_plugins::client_tests`), and one core test
  uses `test_support`. `#[cfg(test)]` items are invisible to another crate, so these need a
  `test-support` cargo feature on the lower crate, or the helper moving to the root's `client_tests`. Done (Wire B): `wrathplus-app` has a `test-support` feature; `net::test_support` (the per-family
  `dispatch`/`captured` helpers and the vehicle fixture guid) and the root `test_support` compile under
  `cfg(any(test, feature = "test-support"))`, and every cross-layer user imports from there.
- `env!("CARGO_MANIFEST_DIR")`, `include_str!` and `include_dir!` in moved files resolve against the new
  crate's directory. None failed in this pass; the move commit still needs to check them.

**Proposed:**

1. Reassign layers rather than move net's API into core. `names`, `items`, `go_templates` and
   `area_trigger` are wire-query caches: they send `ClientCommand`s, key on `GuidIndex` and register
   handlers. Nothing in core needs them except `name_persist` and `creature_type` (`NameCache`,
   `CreatureRecord`), which go with them, and `preflight` and `probe_shield`, which are dev
   instruments and belong in tools. With `SelfGuid` and `ServerSaidMessage` moved to `app_core::session`,
   core has no net edge left. Done (Wire B): the layer table above carries the reassignment,
   `SelfGuid` and `ServerSaidMessage` are in `app_core::session`, and `game_plugins` installs
   `MotionPlugin` right after `NetPlugin`, with `net`'s `motion` re-exports dropped and their callers
   repointed by the script below.
2. The symbol-map rewrite for the move step: `uv run scripts/split/canonical-paths.py`, a dry run by
   default. It rewrites `crate::<old>::X` through every re-export to the defining path, splits
   `use` lists, and with `--via <module> --drop-reexports` deletes that module's re-exports.
   `--home` limits it to one destination. Done (Wire B); its first run moved net's 11 `motion`
   re-exports (65 paths in 24 files).
3. `swing_refusal` moves to the world layer, which is a table edit.
4. Unclaimed, for the manager: `BuildId` to core; the lookup rows (`ui_unit` names and snapshot,
   `ui_macro::identity`, `creature_anim::disarmed_hand`); `LetterTexts`, `LoginIntent`; the 38
   inherent impls; the test-support feature.

**Re-run (Wire B, 2026-10-21, wrath-hud 333bcecb, with Native UI's lookup rows 5bd416f8).** Same
moves, with the layer reassignment (core: the 14 modules of the table above; net: `net`,
`realmlist` and the six wire-query caches), counted as distinct `cargo check --lib --tests` error
sites. Raw: core 27, core+net 65. After `canonical-paths.py --apply` (2610 paths in 443 files; the
rewritten tree builds, warnings being the re-exports it leaves unused): **core 13, core+net 20.**
The script now also follows a relative re-export (`pub(crate) use cooldowns::Cooldowns`,
`use ring::{…, Factions}`) into a chain; before that, 12 more core+net sites (3 of them core) (`spell::Cooldowns`,
`ui_action::{Spells, UiError, UiErrorKeys, FillArg}`, `target::Factions`, `player::Player`,
`creature_anim::move_flags`, `entities::CollisionHeight`) failed only because a chain stopped at a
relative hop.

| Cause | Sites | What | Owner |
|---|---|---|---|
| Core calls an extension-trait method | 4 | `GossipState::resolve_greeting` → `GossipGreeting::draw_greeting`, `Realms::highlight_current` → `RealmsSelect::rows` (and their `use` lines) | Done (Wire B): `open_menu`, `text_arrived` and `resolve_greeting` are `GossipGreeting`'s, `highlight_current` is `RealmsSelect`'s |
| `Default` impl left in an upper layer | 2 | `CollisionHeight` (`entities::collision_height`), `ui_flags::Sort` (`realm_select`) | Done (Wire B): both impls moved with their types (the inherent-impl matcher did not look at trait impls) |
| Core → spell | 3 | `Spells`' cast-time helpers take `spell::SpellModifiers` and `OP_CAST_TIME` (`app_core::state`) | Done (Wire B): `spell::SpellsCastTime` |
| Core → ui_items | 1 | `app_core::state`'s `Key(ui_items::ItemUse)` | App Migration (`ui_items`): `ItemUse` moves down if pure |
| Core → net | 1 (core alone only) | `app_core::interaction` reads `names::NameCache` | The method leaves core, or takes the name as an argument |
| `cfg(test)` helper | 1 | `app_core::vm_memo` tests use the root's `test_support::{fn_parameter_lists, rust_files}` | The test moves to the root, or the helpers to a core `test_support` |
| `launch_plan` unassigned | 1 + 2 | `run_mode` (core) and `realmlist` (net) read `launch_plan::plan()` (bad7aa42) | Done (Wire B): `launch_plan` is in the core row |
| `net` drains UI state | 4 | `ui_party::net::{member_deactivated, roster_deactivated}` (3), `combat_log::chat::faction_standing` | App Migration's groups (unchanged) |
| Root module alias | 2 | `names` reaches `NativePetNamePackets` through the root's `use net::{wrath_pet_name_capture as ui_native_pet_name}` | Done (Wire B): `names` names `net::wrath_pet_name_capture` |

After those (860a358f, with `launch_plan` moved as core): **core 3, core+net 6**. Left: `ItemUse` and the four
`net` drains (App Migration, in flight), the `vm_memo` test helper, and in core alone
`app_core::interaction`'s read of `names::NameCache`.
After fa933740 (both moved, the `vm_memo` scan to the root): **core 0, core+net 0**: both crates
build, lib and tests (`cargo check -p benilla-app-core --lib --tests`).

`ConsoleCommandApp` and `capture::ProbeAuctionPlugin` are not net-side. The console trait is used by
`experience`, `video` (world) and `ui_script`. The probe plugin is named only by `dev` (tools) and in a
plain-text `ui_auction` comment. They stay with the world → ui rows.

### Move dry run: world and ui (Wire B, 2026-10-21)

On 939f2801 (wrath-hud 2a5560d2 and the core rows), after `canonical-paths.py --apply` (2588
paths). The layers below each pass go into one crate with it, so a pass shows only the edges out
of its top layer and below, as resolution errors (distinct `cargo check --lib --tests` sites):
**world pass** (core + net + world, 71 modules): **62** (49 in the lib); **ui pass** (core through
ui, 175 modules): **16** (3 in the lib). Each pass's one core → root site, `vm_memo`'s test, is
gone in fa933740. Not measured: the visibility widening and the cross-crate inherent impls, which
only separate crates report.

Modules the table above does not list, as the pass placed them. World: `asset_churn`,
`blob_shadow`, `camera_shake`, `chat_bubble`, `cinematic`, `combat_text`, `doodad_events`,
`experience`, `experience_ao`, `go_anim`, `inventory`, `loot`, `overlay`, `pipe_warm`,
`poi_marker`, `area_poi`, `raid_marks`, `ranged_flex`, `screen_fade`, `screenshot`, `shaders`,
`opaque2d`, `smart_rect`, `world_backdrop`. Ui: `npc_text`, `quest_rewards`, `game_tip`,
`wrath_vehicle_seats`. Root: `shipped_cvars`, `test_support`.

**World → ui: 61 sites.**

| Cause | Sites | What | Owner |
|---|---|---|---|
| 2-D drawing primitives in ui | 20 | `ui_pass::{UiQuad, UiQuads, UiQuadMask, UiQuadMaterial, UiQuadAppend, UvRect, PlayerUiCamera}`, `ui_text::{layout_text_quads, measure_text, FontSpec, Justify, TextSeat, UiFontAtlas}` (3 are `Justify` follow-ons), `ui_models::UiModelTiles`, from `minimap` (6), `chat_bubble`, `nameplates`, `combat_text`, `pipe_warm` (4), `portrait`, `world_backdrop` | **Design:** the quad and text layout primitives move into the world `overlay` module (Native UI's), or the 2-D overlays (`minimap`, `chat_bubble`, `nameplates`, `combat_text`) are ui modules |
| A ui test fixture | 7 | `ui_items::TestObjects::new` in world tests | Mechanical: the fixture moves to `net::test_support` |
| Ui-owned state and data | 15 | `ui_guild::{GuildState, unit_guild_name, unit_guild_emblem, corpse_guild_emblem}` (6), `ui_unit::{UnitTokens, player_token_guid}` (4), `ui_loot::LootMoveStart`, `ui_items::ItemSubClasses`, `ui_chat::{default_color, combat::DURABILITYDAMAGE_DEATH}`, `tutorial::id::LOOTING` | Pure moves: `GuildState` (Assets) and `UnitTokens` (Native UI) are in flight |
| World calls a ui system or helper | 10 | `combat_log::{native::NativeCombatLog, text::{melee_center_text, classify_source}}`, `ui_quest::net::register`, `ui_pet::pet_stop_on_old_target_clear`, `ui_merchant::end_repair_mode_on_right_press`, `ui_party::raid_row_guids`, `ui_action::reagent_totem_refusal`, `tutorial::InputHooks`, `game_tip::tip_bundle` | **Design per row:** a message the ui drains, the caller moving up, or the helper moving down |
| A ui extension trait called from world | 8 | `ConsoleCommandApp` (`experience`, `video`: 4), `ChatLogFeed::push_combat` (`death`: 2), `AuraDurationsTrace::set` (`spell::net`: 2), each counting its `use` and its call | **Design:** the console trait is the registry question above; the other two methods need their ui feature, so world sends a message or the trait's lower half moves down |
| `Default` impl in ui | 1 | `app_core::state::LootConfig` | Mechanical: the impl moves to its type |

**Ui → root: 15 sites** (the ui pass's 16, less `vm_memo`).

| Cause | Sites | What | Owner |
|---|---|---|---|
| `shipped_cvars` | 11 | `ui_script` reads the root's aggregate (`addons`, `manifest`, `wrath_addons`, five test files) | **Design:** the cvar-registry note above: the root passes the static table in |
| Root module alias | 2 | `ui_social` reaches `wrath_who_capture` through the root's `use net::{wrath_who_capture as ui_native_who}` | Mechanical |
| `include_dir!` path | 1 | `ui_script::content` embeds `$CARGO_MANIFEST_DIR/assets/ui` | Mechanical: the assets move with `ui_script`, or the path names `wrathplus-app` |
| `cfg(test)` helper | 1 | `ui_session` tests use the root's `test_support` | Mechanical: the test moves to the root, or the helper down |

So of the 76 sites, 27 are mechanical (the fixture 7, the pure moves 15 of which 10 are in flight,
two aliases, the asset path, the test helper, the `Default` impl), and 49 are four design
questions: where the 2-D primitives live (20), how world reaches ui systems (10) and ui-feature
traits (8), and how the root hands `ui_script` the cvar table (11).

**Decisions (manager, 2026-10-22), applied by Wire B:**

1. The 2-D drawing primitives are rendering, not UI logic: they move down into the world
   `overlay` module (`UiQuad`, `UiQuads`, `UiQuadMask`, `UvRect`, `UiQuadMaterial`,
   `PlayerUiCamera`, and the model tiles `UiModelTiles`, `TileRequest`, `Cell`), and `ui_text`,
   the text engine, is a world module whole (its one ui reach, the addon root, is
   `local_state::addons_root`). `minimap`, `nameplates`, `chat_bubble`, `combat_text` and
   `portrait` stay world.
2. World calling ui: the caller moves up, or a pure helper moves down. The spell-failure combat
   log entry and the death durability line register in `combat_log` (two owners pinned on
   `WrathSpellFailure`); `melee_center_text` and `classify_source` are `combat_text::center`;
   the totem and reagent check is `spell::reagents`; `InputHooks` and the tutorial ids are
   `app_core::tutorial`; `game_tip` is a world module (it serves the loading screen); the three
   world tests that assembled ui plugins moved to the root or stopped needing them.
3. A ui-feature trait called from world moves to its type's layer when its body is pure: the
   console registry and `ConsoleCommandApp` are `app_core::console`; `AuraDurations::set` is
   inherent again (its trace switch is `app_core::state::aura_trace_period`); `ChatLogFeed`'s
   world caller moved up (above).
4. `shipped_cvars` is a ui module (final decision): every row source sits at or below ui.

The guild session is a world module, `guild` (state, identity cache, sort, chat lines and packet
handlers); `ui_guild` keeps the feed and the plugin. It cannot be core: its identity asks send
through `NetCommands`.

**Re-measured** on ed8df006 (wrath-hud 83cff406 with de01d1ce and dcff03f2, which also moved
`player_token_guid` to `target::unit_tokens` and `wow_font` to `ui_text`), canonical-paths
applied (2740 paths): **world pass 0** (core + net + world build, lib and tests); **ui pass 1**,
`ui_script::content`'s `include_dir!("$CARGO_MANIFEST_DIR/assets/ui")`. That one is the move
step's: the asset tree moves with the ui crate, or the path names `wrathplus-app`'s, as does
`overlay`'s `embedded://wrathplus_app/shaders/ui_quad.wgsl`. Every layer below tools now resolves
downward. Not yet measured: the tools pass, and what only separate crates report (the
visibility widening and inherent impls on a type from another crate).

## Moving a layer out (steps 1 to 4)

Order: `core`, then `net`, then `tools`, then `world` and `ui` (world first, so `ui` can depend
on it). Each step is one commit, done while the workers are paused (see "Coordination").

Mechanics for moving layer `X` with modules `m1 … mn`:

1. `cargo new --lib crates/benilla-app-X`; copy the dependency list of `wrathplus-app` and prune
   it later with `cargo machete`; add it to the workspace.
2. `git mv crates/wrathplus-app/src/m1.rs crates/wrathplus-app/src/m1/ … crates/benilla-app-X/src/`
   for each module (`git mv` keeps blame).
3. In `benilla-app-X/src/lib.rs`: `pub mod m1; … pub mod mn;`.
4. In `wrathplus-app/src/lib.rs`: replace `mod m1;` with `pub use benilla_app_X::m1;` for each.
   `crate::m1::Foo` paths in the modules still in `wrathplus-app` keep resolving through the
   re-export, so nothing above the moved layer is edited.
5. First run `uv run scripts/split/canonical-paths.py --via <each upper module> --apply`, so no moved file
   names an item through an upper layer's re-export. Then, inside the moved files, rewrite
   references to lower crates:
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

- `tests/ui_script_wall.rs` and `tests/world_api_wall.rs` scan `crates/wrathplus-app/src` paths:
  point them at the new crate directories.
- `scripts/quality` and `gates.sh` crate lists, CI matrix, and `docs/WRATH-NATIVE-PLAN.md` gate
  commands (`-p wrathplus-app` becomes a list).
- Each worker's `CARGO_TARGET_DIR` is unaffected; crate names in their gate commands change.

### Separate-crate dry run and the move script (Wire B, 2026-10-22)

The real move, every layer a crate, run end to end on a throwaway worktree of ace77059 with the
scripts below. Result: `cargo check --workspace --tests` is clean, and the six app crates' tests
run to 4,621 passed and 7 failed (core 62, net 196, world 1,633, ui 2,446/1, tools 114, root
174/2, plus the root's integration tests with 4 failures), all seven a test that reads where a
file lives or what its crate is called (below).

What only separate crates report, as the dry run counted it:

| Kind | Count | How the move handles it |
|---|---|---|
| Visibility widenings | about 2,530 declarations (core 6, net 101, world 434, ui 1,268, tools 336, root 385) | `widen-all.sh`: only what the compiler names; 3 passes to a fixpoint |
| Glob imports from another crate | 38 module files | `widen-all.sh` widens each module another crate glob-imports (no error names those) |
| `#[cfg(test)]` helpers other crates' tests use | 210 items, 8 test modules, 27 `test_support` items (blanket count) | `widen.py` opens only the gates another crate reaches; `--test-support` makes `test_support` items `pub` |
| Warnings the move leaves (lib, test, `test-support` builds) | 290 with the blanket opening; 0 now | `quiet.py` (below) |
| Inherent impls on another crate's type (E0116) | 0 | (step 0 moved them all) |
| Orphan impls (E0117) | 3 | fixed ahead (ace77059: `Default` for `UiClock`/`UiScaleCvar`, `AskOnce` for `MailOpen`) |
| `include_dir!`/`CARGO_MANIFEST_DIR` assets | `assets/ui` | `move-layers.sh` moves it with `ui_script` and points `run_mode::dev_source_dir` there |
| `embedded://` shader paths | 5 | fixed ahead (71212f59: served from `CARGO_CRATE_NAME`) |
| Whole-tree source scans | 7 unit tests | fixed ahead (71212f59: `app_core::source_tree`) |
| Tests that name a crate or its folder | the dry run's 7 failures | fixed ahead (Wire B's last pre-move unit, below) |

**Fixed ahead, the seven failures**: `the_movement_kinds_other_handlers_are_pinned` and
`the_cvar_hooks_register_in_plugin_order` compare `type_name` strings by module path, with the
crate prefix dropped; `ui_session`'s portrait test anchors on `fn feed_interact_npc(`, which the
widening to `pub` keeps; `tests/cover_input_wall.rs`, `message_keys.rs`, `ui_script_wall.rs` and
`world_api_wall.rs` read every app crate's `src/` through `tests/app_sources`, which finds the
root and its `<root>-<layer>` siblings under any prefix.

**The rename goes first.** `scripts/rename-wrathplus.sh` renames a fixed crate list, guarded with
`(?![\w-])` and `(?!\w)`, so after the move it would miss `benilla-app-<layer>` and every
`benilla_app_<layer>` token. The split scripts take the prefix from the root that exists
(`crates/wrathplus-app`, else `crates/wrathplus-app`), so the moved crates are born
`wrathplus-app-<layer>`. The tests above name no prefix either.

**The move: one run of every layer, one commit, during a worker pause** (`scripts/split/`):

1. Merge wrath-hud; gates green; `git status` clean. `move-layers.sh` refuses to start with
   uncommitted or untracked changes or a `MERGE_HEAD`: a half-moved crate directory would count as
   already moved on a re-run. After a failed run, back to the pre-move head and run again:

   ```sh
   git reset --hard <pre-move head>
   git clean -fd crates/
   scripts/split/move-layers.sh --layers "core net world ui tools" --test-support
   ```
2. `scripts/split/move-layers.sh --layers "core net world ui tools" --test-support`. It runs
   `canonical-paths.py --apply`, then `layer-edges.py`, which stops the move on any import that
   crosses `layers.txt` upward (merges bring new ones: 795d62c3's `wrath_vehicle_seats` was the
   last, placed in core). It `git mv`s the modules with their `#[path]` siblings, writes each
   crate's `Cargo.toml` (the root's dependencies, the lower crates, `dev` and `test-support`
   forwarded, a `test-support` dev-dependency on each lower crate) and `lib.rs` (`pub mod` per
   module, every lower module re-exported at its root, so `crate::m::…` keeps resolving), moves
   `assets/ui` with the ui layer, and makes each `test_support` module's items `pub`.
3. `scripts/split/widen-all.sh 6`: compiler-named widenings to a fixpoint. Each `#[cfg(test)]`
   item another crate reaches is opened to `#[cfg(any(test, feature = "test-support"))]` (rustc's
   "configured out … gated here" note, or for a method, a same-scope name or a test-only
   re-export, the gated declaration of that name); the rest stay `cfg(test)`. It fails, listing the
   errors, if a pass widens nothing while a crate still has errors.
4. `uv run scripts/split/quiet.py --apply`: per crate, the lib, test and `test-support` lib
   builds. It removes imports the test build also calls unused, moves imports only tests use to a
   `#[cfg(test)]` import (the "Test-only" re-export pattern), gives `#[cfg(test)]` to helpers in an
   opened block that only their own crate's tests use (with a struct's `impl`s), and widens a
   `private_interfaces` type. It deletes nothing; anything else it lists.
5. Gates for every crate (`--lib --tests --no-fail-fast`), clippy per crate, the census, the
   replay (sha256 `16bc666b…`; a move changes no state).
6. Commit: renames, one `Cargo.toml` and `lib.rs` per crate, the widenings and gates.
7. Then regenerate `scripts/quality/loc-baseline.json` once against the new paths (strict again
   from then on), and update the gate crate lists (`scripts/gates.sh`, `check.sh`, CI, worker
   gate commands: `-p wrathplus-app` becomes the six crates).

**Final dry run** (0b43ae3d, wrath-hud 4f89242b plus the pre-move lint fixes; throwaway worktree,
warm target, heavy slot; 2026-10-23):

| Step | Time | Result |
|---|---|---|
| `move-layers.sh`, every layer | 33 s | core 16, net 8, world 51, ui 102, tools 9 modules; 0 upward edges |
| `widen-all.sh 6` | 55 min | 38 glob modules; 2,659 widenings and opened test gates, 89 of them gates (core 6, net 105, world 453, ui 1,320, tools 339, root 436); fixpoint in 3 passes |
| `quiet.py --apply` | 11.5 min | 0 warnings left in any crate's lib, test or `test-support` build; nothing deleted; 67 `#[cfg(test)]` lines added (test-only imports and own-crate helpers) |
| clippy per crate (`run.sh clippy -p`) | 22 min | all six `clippy-report passed` |
| six crates' tests (`--lib --tests --no-fail-fast`) | 14.5 min | lib 4,631 passed, 19 ignored (root 177, core 62, net 197, world 1,642, ui 2,439, tools 114), as on the unmoved tree; all 15 integration binaries pass |

One layer at a time costs more, because every layer widens and checks the root again: core alone
took 9 s to move, 59 min to widen and 12.5 min to check (505548ff). So the move is a single run.
Budget about 1 h 10 min for steps 2 to 4, then the gates.

**Fixed ahead of the move** (0b43ae3d): the five clippy API lints the widening raises, which
`quiet.py` does not touch: `is_empty` beside `len` on `QueryCache` and `NameCache`,
`PlaySeq::next` renamed `advance` (`should_implement_trait`), and `Default` for the `TestDeps` and
`TestObjects` fixtures.

**The play binary and the player build on the moved tree** (same dry run):

- `cargo build --profile play -p wrathplus` (what `cargo play` builds; the installed binary carries
  the `dev` instruments, so default features): 22 min, no rustc warning; `ld` repeats its
  `__eh_frame section too large` note, as the unmoved build does. Headless smoke, no window:
  `benilla --help` prints the usage and exits 1, `WOW_CAPTURE=list benilla` lists the four
  captures (tools crate) and exits 0, both before any setup.
- Player tests (`--no-default-features --lib --no-fail-fast`, the gate's `player-tests` rung on
  the six crates and `wrathplus-formats`): core 62, net 197, world 1,642/7 ignored, ui 2,439/9,
  tools 47, formats 441 pass; the root 175 pass and 2 fail,
  `the_movement_chain_keeps_its_order` and `the_cvar_hooks_register_in_plugin_order`, which fail
  the same way on the unmoved tree (24ec10d8): without `dev` they read no system or observer names.

**Unused dependencies** (`cargo machete` on the moved tree; each crate starts with the root's
list, the unmoved root has none; prune after the move, not in it; `bevy_egui` is optional and named
in each crate's `dev` feature, so removing it edits that feature too):

| Crate | Unused |
|---|---|
| core (19) | anyhow, arboard, assert_no_alloc, bevy_egui, bytemuck, cosmic-text, crossbeam-channel, fontdb, half, image, include_dir, kira, libc, mint, rand, rtrb, symphonia, wgpu, winit |
| net (21) | arboard, assert_no_alloc, avian3d, bevy_egui, bytemuck, cosmic-text, fontdb, half, image, include_dir, kira, libc, mint, rand, regex, rtrb, serde, symphonia, toml, wgpu, winit |
| world (8) | arboard, bevy_egui, bytemuck, include_dir, rand, regex, wgpu, winit |
| ui (14) | assert_no_alloc, avian3d, bevy_egui, cosmic-text, fontdb, half, kira, mint, regex, rtrb, serde, symphonia, toml, wgpu |
| tools (14) | anyhow, arboard, assert_no_alloc, cosmic-text, fontdb, half, include_dir, kira, mint, rand, regex, rtrb, symphonia, winit |
| root (20) | anyhow, arboard, avian3d, bevy_egui, bytemuck, cosmic-text, fontdb, half, image, include_dir, kira, libc, mint, rand, regex, rtrb, serde, symphonia, wgpu, winit |

## Coordination with in-flight work

Measured on each worker branch's own commits over the last three days (files touched in
`wrathplus-app`, `wrathplus-ui`, `wrathplus-protocol`):

| Worker | Areas in flight | Layer |
|---|---|---|
| Wire A | `net` (wire families), `ui_chat`, `spell` | net, world, ui |
| Wire B | `ui_script`, `ui_loot`, `ui_social`, `ui_calendar`, `ui_guild`, `ui_quest`, `ui_world_map`, `wrathplus-protocol` | ui |
| App Migration | `ui_script`, `ui_macro`, `ui_guild_bank`, `game_plugins`, `spell` | ui, root |
| Native UI | `ui_script`, `ui_quest`, `ui_items`, `wrathplus-ui` | ui |
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

The numbers above come from a short script over `crates/wrathplus-app/src`: list the `mod` lines
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
- `app_core::errors::UiErrorTexts` carries `wrathplus_ui::messages::MsgKind` (found in step 0
  unit 2a). Settled by the 2026-10-21 decision: `benilla-app-core` depends on `wrathplus-ui`.
