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
6. **Tests that build the whole client.** 44 references to
   `game_plugins::schedule_tests::headless_client` sit in lower modules' unit tests. Move those
   tests to `crates/benilla-app/tests/` (root integration tests) or give each layer a narrower
   harness that adds only its own plugins. Test-only back references total 112; the rest are
   `Spells` (31, fixed by item 2) and a handful of `InteractNpc`/`HardLanding`/`QuestGiver`.

Done when the module graph, rerun with the script below, shows no back edge between layers.

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
