# App crate split after the wrathplus rename

Status: proposal, not an extraction authorization. This is a docs-only inventory of the current
`wrathplus-app` seams. Execute only after the native cutover is accepted, WS1–WS6 are merged, and
the single mechanical `benilla-*` → `wrathplus-*` rename is green. Keep `WOW_*` variables unchanged.
The parent requirements are in [WRATH-NATIVE-PLAN.md](WRATH-NATIVE-PLAN.md), “After the native
refactor”. Existing protocol, UI engine, assets, formats and world crates remain separate.

## Evidence and constraints

The current folders are **not** independent crates. A lexical scan of `crate::<module>` references
(including tests, comments and qualified expressions, not a resolved Rust dependency graph) found:

| Family | Rust files / lines | Important outward references |
| --- | ---: | --- |
| `net.rs`, `net/` | 26 / 17,149 | player, creature_anim, transport, items, ui_session |
| `ui_script/` | 115 / 76,380 | local_state, char_select, cvars, ui_text, portrait, ui_chat, ui_unit |
| `entities.rs`, `entities/` | 27 / 16,675 | portrait, net, creature_anim, aura_visual, player |
| `player.rs`, `player/` | 36 / 19,175 | net, creature_anim, bindings, char_select, ui_script |
| `capture/` | 54 / 22,533 | net, player, target, perf, entities, ui_script |
| `addon_harness/` | 8 / 5,273 | ui_script, cvars, ui_items, ui_unit |
| `sound/` | 37 / 15,121 | net, creature_anim, entities, char_select, ui_script |

These are local snapshot sizes, not promised destination sizes. `ui_*` is not a single layer:
`ui_session.rs` owns NPC range/session behavior; `ui_script/mod.rs` owns the VM and extraction;
`ui_unit.rs` mixes domain reads, UI feed and sound. Moving folders verbatim would create cycles.
Before each extraction, resolve imports, `super::` paths, system signatures, plugin build-time
resource reads, feature-gated edges and test-only edges. Record the resulting edge list in review.

Load-bearing examples:

- `game_plugins.rs` documents plugin registration order as behavior, including executor tie-breaks.
- `net/handlers.rs` dispatches one-shot systems in packet order; it is already a registration
  boundary. A subsystem registers its own handler without the dispatcher importing the subsystem.
- `net.rs` currently defines both transport channels and broadly shared ECS types (`ObjectStore`,
  `GuidIndex`, `FieldChanged`, `FieldEdges`, self/mover markers).
- `entities.rs` produces `portrait` parts/anchors; portrait contracts cannot remain owned by a
  downstream UI host if entities moves below it.
- `ui_script` owns pointer arbitration types read by gameplay. Those are input contracts, not VM
  implementation details. `run_mode.rs` also has a dev-gated call back into capture; lift that
  answer into configuration before enforcing a strictly downward tools dependency.

## Target: eight crates including the app shell

Names below are post-rename names. An arrow means “depends on”; all rows may additionally depend
on existing lower crates where needed. No existing lower crate may acquire an app-layer dependency.

| Crate | Allowed app-layer dependencies |
| --- | --- |
| `wrathplus-client-state` | none |
| `wrathplus-session` | client-state |
| `wrathplus-gameplay` | client-state, session |
| `wrathplus-scene` | client-state, session, gameplay |
| `wrathplus-ui-render` | client-state |
| `wrathplus-ui-host` | client-state, session, gameplay, scene, ui-render |
| `wrathplus-devtools` | all six libraries above; never app |
| `wrathplus-app` | all above; devtools optional |

The existing thin `wrathplus` launcher still depends on `wrathplus-app`, not the other way round.
No library imports the shell, even for tests. This is an intended DAG, not a claim that today's
imports already satisfy it. In particular session must lose its player/animation/UI imports,
scene must lose VM imports, and rendering must lose gameplay imports **before** moving them.

### 1. wrathplus-client-state: narrow shared vocabulary

Move the shared ECS identity/snapshot types from `net.rs`: GUID/index/store, self/active-mover
markers, semantic field edges, dialect and packet-aura snapshot contracts. Include only the
cross-layer parts of `run_mode`, pointer/capture input arbitration and lifecycle/system-set labels.
Move pure pose/animation/portrait data contracts here only when two lower layers truly need them.
Protocol still owns `Field`, `FieldLayout`, `ObjectFields` and wire messages; do not duplicate them.

No network IO, VM, asset loading, rendering systems or giant shared mutable client object. Prefer
small domain-owned resources over turning this into a dumping ground. Define each type once;
re-exports preserve paths temporarily but never wrap/clone a resource into a second identity.
The shell retains process configuration parsing and inserts the resulting run settings.

Compile benefit: common data changes are still broad invalidations; keep this crate small and
stable. It enables narrower later dependencies rather than supplying a large immediate win.

### 2. wrathplus-session: transport and ordered dispatch

Move `net/io`, channel/session generation management, login/realm/character request endpoints,
command routing and the `NetHandlerApp` dispatcher. Move generic object snapshot application and
lifecycle here once it only writes shared state. Keep receive-order application synchronous as
now; do not replace it with a deferred Bevy message hop. Protocol adapters remain in protocol.

Move player-facing reconciliation, animation updates, transport/world placement and NPC/UI
handlers **out** of this layer into their owning plugins first. Register those through the same
handler table; preserve current handler order and the ownership census (including documented
multi-handler exceptions). Shared command endpoint types live here; shared snapshots live in
client-state. The shell chooses `connect`, credentials policy and initial client state.

Compile benefit: codec/IO tests can run without rebuilding VM/feed/presentation implementation;
session edits still invalidate its consumers. Do not claim that every networking change gets a
small rebuild, especially when its exported event/command interface changes.

### 3. wrathplus-gameplay: state and decisions, not widgets

Move domain state and handlers from `items`, `names`, `query_cache`, `world_state`, `spell`,
`experience`, `pending_item_ops`, and the state/decision portions of player, target, combat, death,
NPC sessions and `ui_*` families (quests, inventory, parties, spells, merchants, guild, etc.).
Keep related behavior together initially rather than inventing a crate per window. DBC catalogs
belong with their domain or existing formats/assets, not with the VM that happens to display them.

Strip presentation from mixed modules in separate reviewed steps: UI model setters and Lua
notifications go to ui-host; skeleton/model/camera application goes to scene. Gameplay emits
existing intent/state edges, or exposes typed state for same-frame consumers, without importing
those upper layers. The shell retains full plugin composition, not gameplay's state machine.

Compile benefit: domain-only suites avoid the large host/render implementation. This will still
be a substantial crate; measure it before proposing any further split.

### 4. wrathplus-scene: entities, animation and world integration

Move `entities`, `creature_anim`, `go_anim`, `aura_visual`, visual combat effects, attachment/weapon
trails, transport placement, world-facing player/controller/camera systems, area/trigger/POI glue,
world backdrop/loading integration, and model/portrait scene production. Include sound and its
platform output modules initially: animation and positional audio already share event sources.
Keep terrain streaming, collision primitives and renderer internals in existing `wrathplus-world`.
This crate is the game-specific bridge, not a second terrain engine.

Move current portrait scene data to scene or client-state; leave UI portrait placement in ui-host.
Input arrives via typed gameplay/input state, not reads of `UiScript`. UI-driven dress-up/model
requests use scene-owned APIs consumed by the host. Session handler registration is allowed;
imports from session into scene are not. The shell still sets engine/window/device policy.

Compile benefit: animation/world glue/audio edits no longer recompile the VM implementation;
ui-host, which consumes scene APIs, may still rebuild. Platform dependencies become localized,
not eliminated from the final client link.

### 5. wrathplus-ui-render: presentation backend

Move the reusable portions of `ui_pass`, `ui_text`, `text_reshape`, `textinput`, `opaque2d`,
`smart_rect`, UI shaders and quad/font extraction output types. This is the Bevy presentation
backend for existing `wrathplus-ui`, not a new widget engine. It must not query unit/inventory/net
state. Split mixed cursor/model/portrait rendering first; scene model generation stays in scene,
and VM event/model interpretation stays in ui-host. Renderer-owned input/output contracts replace
any reverse reference to `ui_script`.

The shell retains window setup, backend selection and startup pipeline policy. Production
`pipe_warm` behavior must not become dev-only merely because probes also use it.

Compile benefit: font/quad/backend tests and implementation changes isolate from domain state;
shared Bevy/wgpu dependencies and final linking remain expensive.

### 6. wrathplus-ui-host: VM, feeds and game-to-UI adapters

Move `ui_script` (addon discovery, manifests, stock UI loading, VM lifecycle/memos, input and
extraction), bindings/script calls, model feeds, UI command adapters, chat formatting and UI
placement for minimap, portraits, tooltip, nameplates and other `ui_*` families. Keep existing
`wrathplus-ui` responsible for Lua API/widget semantics; this crate hosts it in Bevy and supplies
game state. Move login/character/glue UI controllers here after pure session state is below them.

Extract domain resources from these modules before moving feeds; do not make gameplay import the
host for `InteractNpc`, action state, party data or pointer-over-UI. Preserve VM memo invalidation,
feed gates, dialect behavior and same-frame event order. The shell retains startup policy,
saved-path/process selection and the ordered `GamePlugins` assembly.

Compile benefit: this is the largest removable family (~76k lines in ui_script alone). A host-only
edit stops recompiling scene/session/gameplay implementation, but still compiles the host itself.
This plan deliberately does not promise a cheap full FrameXML gate.

### 7. wrathplus-devtools: optional, downward-only instruments

Move `capture`, `addon_harness`, dev-only perf/debug panels, preflight/probes, hover logging,
asset churn and showcase instruments after removing dependencies on shell internals. It may
inspect all production libraries; production code may never import it, even under `cfg(dev)`.
Keep shipping screenshot, FPS journal/clocks, warmup and crash behavior outside it. Split mixed
`perf` and `showcase_capture` responsibilities by actual player use, not directory name.

Provide a tools plugin group and headless harness builder using the same production plugin recipe
passed in by the shell; do not import `GamePlugins` from app. The shell adds tools last and owns
list/report early exits. Pass configuration into production resources rather than calling probes
from `run_mode`. Devtools is an optional dependency with deliberate feature forwarding.

Compile benefit: probe iteration avoids recompiling the huge production library; player builds
omit tool-specific egui and capture code/dependencies. Headless tests still need their selected
production stack. Never start the real client as an extraction gate.

### 8. wrathplus-app: player application shell

Keep `run()`, ordered `GamePlugins`, engine/process plugin setup, window/device/start-screen and
run-mode policy, shutdown orchestration, global allocator ownership, and full-stack schedule,
handler-ownership and replay integration tests. The launcher keeps the build-id stamp as today,
so a new commit does not rebuild application libraries. Thin does not mean moving required
cross-layer integration tests into libraries that would need a reverse dependency.

The shell exposes composition, not a new service locator. It imports production plugins and adds
them in precisely the existing order; devtools consumes a supplied builder/recipe rather than
owning a second subtly different player stack. Compile benefit: composition-only changes compile
a small library, though executable relinking still traverses the full dependency closure.

## Boundary rules: resources, plugins and schedules

- Publish narrow plugin/config/resource APIs; do not make all former `pub(crate)` items public.
  Pure internal systems stay private. Consumers use domain accessors or typed commands.
- A resource has one owning crate and one initializer. Other plugins borrow it normally through
  Bevy `Res`, `ResMut`, queries and messages. Extraction must not duplicate `init_resource` with
  different defaults, reset caches, or move initialization past a build-time reader.
- Preserve `WorldStage` from the existing world schedule. Cross-crate ordering uses named public
  system sets or explicit exported system anchors, not incidental crate/plugin ordering. Introduce
  labels around existing systems without changing `.chain()`, `.before()` or `.after()` behavior.
- Keep `GamePlugins` registration order intact throughout the moves. Do not consolidate plugin
  groups or reorder them in the same commit. Check schedule tests plus headless initialization.
- Preserve message lifetime, packet-order dispatch, deferred-command flush points, disconnect
  cleanup, session generation and authoritative empty aura snapshots. A crate boundary is not a
  reason to add a queue, async task, cloned world snapshot or trait-object indirection.
- Unit fixtures stay beside owners. Full-handler replay and schedule/census tests remain shell
  integration coverage. Do not create test dependency cycles; accept production plugin builders
  or relocate integration tests upward instead. No new library depends on app in dev-dependencies.

## Mechanical rollout, after the rename

1. Record clean/full-build and warm one-file edit timings (domain, scene, VM, probe, shell), peak
   memory, crate sizes and dependency graph for dev and no-default-features player builds. Separate
   Rust compilation from link time. Choose consistent machine/cache/build settings for comparison.
2. In app, first isolate shared state and registration APIs into internal modules, one family per
   commit; keep temporary re-exports. Audit the real graph and eliminate reverse dependencies with
   typed state/config or moving the owning handler. No behavior redesign in these preparations.
3. Extract client-state, then session transport/dispatch in small units. Move session's remaining
   cross-layer handlers to their existing domain plugins before extracting object application.
   Preserve dispatch ordering and verify handler ownership after each registration move.
4. Separate gameplay state from VM feeds and scene side effects; extract gameplay family by family.
   Player/target and action/party/NPC-session cycles need explicit review, not blanket public APIs.
5. Extract ui-render's backend contracts and pure renderer, then scene family by family. Remove
   scene-to-host edges (pointer arbitration and portrait/input contracts) before the scene move.
6. Extract ui-host, starting with VM lifecycle/content, then feed/command families. Keep temporary
   app re-exports until all callers and source-inspection tests use destination paths.
7. Extract devtools, remove its shell imports through supplied composition, and forward features
   explicitly. Verify the no-default-features dependency graph excludes instruments/egui.
8. Delete compatibility re-exports, shrink app to composition, update test discovery/source-path
   checks, manifests, scripts and ownership docs. Re-measure the same edit matrix and link time.

Each unit is move-only plus necessary imports/visibility/manifests, with no opportunistic renames,
format-all churn, behavior fixes or dependency upgrades. Run the full app `--lib` suite and the
full `--lib` suite of every touched crate, plus targeted boundary tests. Gate the touched crates
with rustfmt/clippy, line-count and CRAP ratchets, unused-dependency and typo checks per the parent
plan. Split oversized source files mechanically before an extraction rather than growing the
baseline. Preserve installed-data census and both dialect fixtures; manager owns any live check.
Use the shared heavy-build lock, own target below 40 GB, disabled incremental, four jobs and low
priority until the manager changes that policy. Do not launch the game or push.

## Success criteria and limits

Acyclic Cargo graph in production **and tests**, player/dev feature parity, unchanged snapshots,
handler ownership and schedule behavior, and green per-step suites are prerequisites. Performance
success is a measured reduction in warm-edit compilation scope and focused gate time, not an
invented percentage: Cargo fingerprints, generic monomorphization, shared Bevy crates and final
linking can limit gains. A client-state API change still invalidates most crates. If the graph
cannot be made downward without redesign, stop at the last mechanical seam and review a separate
behavior/API proposal rather than hide a cycle behind callbacks or move the whole app into a
newly named crate.
