# AzerothBench client fork

## Scope

Port Benilla to AzerothCore WotLK 3.3.5a/build 12340 while retaining normal human play. Provide low-level client observation/control instrumentation for external evaluation harnesses, RL and synthetic trajectory generation. Mixed human/agent worlds are supported research scenarios, not an excuse to mix undeclared human assistance into autonomous scores.

The client is not a harness: do not implement an LLM loop, policy DSL, policy trainer, navigation planner or combat controller. External harnesses may implement/train fast policies, VLM/VLA controllers, state machines or hierarchical strategist/reflex systems. No `navigate_to` in the baseline.

## Control contract to implement

- Keyboard/mouse-like controls and ordinary player-accessible slash/console commands, subject to scenario permissions; no GM/debug authority or unrestricted runtime introspection.
- Synchronized screenshots and optional declared audio/state modalities. Additional structured information is a separately declared capability, not baseline omniscience.
- Actor/episode IDs, monotonic timestamps, observation/action sequences and observation references.
- Distinguish admission, execution and completion receipts; idempotency, expiration/staleness policies and bounded held-input durations.
- Cancellation and automatic input release on disconnect; explicit human/agent control ownership, handoff and intervention provenance.
- Versioned language-neutral schemas; thin MCP/CLI/SDK adapters over the same capabilities.
- Privileged evaluator/reset services remain outside the actor boundary. Tools are not isolation: shell-enabled harnesses must not reach server databases, admin endpoints, client process memory or evaluator fixtures.
- Asset files may contain more information than a player sees. Structured modes and audio labels must declare source, visibility/audibility, precision and freshness.

## Port gates

1. Pin upstream and establish unmodified offline test/build baseline. Missing-data skips are not gameplay validation.
2. Wrath authentication/session crypto, realm/character selection and object updates against an isolated test realm.
3. Starting-zone rendering from user-provided 3.3.5a data, movement and normal human input.
4. One quest: accept, combat, loot, turn in through normal gameplay.
5. Low-level observation/control contract and repeated recorded episodes.
6. Container/native packaging with read-only user-data mounts and isolated scoring.

Each feature needs relevant unit/fixture tests and live evidence before claims of compatibility. Do not merely change CLIENT_BUILD. Preserve upstream provenance/licence notices and keep differences reviewable. Main advances by reviewed PR; implementation stays on branches.

## Verified upstream baseline — 2026-10-02

Pinned upstream: `b396bbf69a0486b6d29829ec191145f63c363eea`.

- `cargo test --locked -p benilla-srp -p benilla-protocol -p benilla-bytes -p benilla-m2 -p benilla-dbc`: exit 0; 479 tests passed across 37 result groups.
- `cargo build --locked -p benilla --jobs 4`: exit 0; native dev build completed in 13m 16s.
- Non-blocking future-Rust compatibility warning: dependency `block v0.1.6`; inspect using `cargo report future-incompatibilities --id 1`.
- No assets supplied, client launched, realm connection made or services restarted. This is vanilla build/test evidence only, not Wrath compatibility or gameplay acceptance.

## Verified Wrath protocol slice — 2026-10-02

Additive APIs now implement build-12340 auth/SRP proof verification, RC4-drop1024 directional header crypto, bounded world framing, character enumeration, normal character creation and initial login packet observation. The default graphical client remains on the vanilla path.

Live dedicated-account gate on the authorized current realm passed: `AZBENCH` authenticated; `Benchone` was created normally as a level-1 human warrior; enumeration returned its roster; initial world login received 32 packets including one login verification, one account-data-times and one time-speed packet. No movement/combat or object-state decoding is established by this gate.

`Chain::open_wrath` resolves installed DBC/WDT/BLP/M2/SKIN/FrameXML sentinels read-only. External skin parsing succeeds; the first player-model parser gate currently stops explicitly at unsupported camera tracks. No graphical Wrath rendering or stock UI acceptance is claimed.

Next gates: model/render integration, Wrath DBC and terrain interpretations, Wrath gameplay state/packets, and the Lua/UI dialect. Human starting-zone gameplay must pass before agent/benchmark instrumentation work.

## Verified Wrath object and bounded movement slice — 2026-10-03

The dedicated `wrath_objects` probe passed on the authorized current realm using only `AZBENCH` / `Benchone`. Its default remains observation-only; movement requires `AZEROTHBENCH_OBJECT_MOVE=bounded`.

- Observation: 62 packets, 90 creates and 45 nearby units decoded; self position and health decoded; normal logout completed (exit 0).
- Separate movement gate: 62 initial packets, 92 creates and 46 nearby units decoded. Ordinary forward/backward start–heartbeat–STOP packets moved at most 0.75 yards from the origin, with a 1.25-yard path and a two-second movement deadline.
- After normal logout, normal authentication/character enumeration verified the server-persisted position changed from `(-8949.950, -132.493, 83.531)` to `(-8949.700, -132.493, 83.531)`, matching the final STOP within 0.10 yards on the same map (exit 0). Sends and self echoes are not the acceptance criterion.
- Final offline protocol gates: 289 library tests and nine probe tests passed; strict all-target protocol Clippy, formatting and diff checks passed. Framing tests cover continuously dribbled bytes, shared header/body deadlines and partial-frame poisoning. AC-derived fixtures cover combined movement-update flags and branch precedence.

Local evidence: `/tmp/azerothbench-live-objects.log`, `/tmp/azerothbench-live-movement.log`, `/tmp/azerothbench-movement-gates.log`. No realm restart, GM action, combat or direct database mutation was used. This qualifies the headless networking slice only: graphical starting-zone rendering, normal human input, UI, combat/loot/quests and liquid rendering remain unqualified.

## Graphical starting-zone gate — blocked, 2026-10-03

The current opt-in Wrath graphical path was built and launched on native macOS against the authorized realm using only `AZBENCH` / `Benchone`. This gate did **not** pass connected rendering, input or gameplay acceptance.

- First launch failed before world entry because the native UI route omitted `ReloadUiPending`, a mandatory resource of the shared logout system. The route now initializes that resource; a regression executes the real logout drain without a Lua VM and verifies no command is emitted.
- After that fix, the launcher authenticated and reached the level-1 character's Northshire position with 60/60 health. A stationary screenshot shows textured terrain, a path, buildings and props, but no player avatar; its overlay truthfully says `Session: disconnected`. The visual census found one pending player with zero meshes. A screenshot after disconnect is not connected-rendering acceptance.
- The session safety fence refused mandatory `SMSG_MOVE_UNSET_CAN_FLY` (`0x344`). Shared world-entry producers also queued unsupported `QueryNextMailTime` and `BattlefieldStatusRequest`; the writer refused Vanilla serialization. The reconnect loop was stopped by closing only the dedicated test-client window.
- No movement keys, combat, GM action, normal account, realm restart, direct database mutation or reset was used. Ground movement, normal logout/reconnect and quest acceptance remain unqualified. Read-only install path/size/mtime manifests matched before and after the run; no content-hash claim is made.
- Post-fix offline app library suite: 4,255 reported passes, zero failures, ten ignored, with `WOW_DATA=`. This is fixture/no-install evidence, not installed-data qualification. Launcher build, formatting, diff checks and strict app-library Clippy all exited zero on macOS. Full workspace/player-build/platform gates were not rerun in this slice.

Private local evidence: `/tmp/azbench-graphical-stationary.log`, `/tmp/azbench-graphical-stationary2.log`, `/tmp/azbench-graphical-stationary2.png`, `/tmp/azbench-graphical-stationary2.trace`, `/tmp/azbench-native-bootfix-tests.log` and `/tmp/azbench-native-bootfix-final.exit`. Asset screenshots are not committed.

Next compatibility gate: implement the reference-derived Wrath can-fly order/ack and authority semantics, suppress unsupported automatic query producers by session dialect, then repeat the same bounded graphical gate. Do not waive mandatory movement/control orders to keep the socket connected. M2 lights/particles/blend-map overrides and remaining Vanilla DBC schemas also produced visible load failures; stable avatar rendering still needs evidence.

## Bounded graphical ground/logout/reconnect gate — 2026-10-04

This supersedes the blocked graphical slice above only for bounded stationary/ground input and ordinary logout/reconnect, not broader gameplay or full rendering.

- The fork includes `mlua` and the original Vanilla FrameXML host. Its Lua-5.0 compatibility/API surface is for 1.12, not Wrath. Wrath explicitly bypasses that VM; the native six-key movement adapter reuses `BindingDispatch`, held/release state and the existing controller rather than privileged probe/controller shortcuts. Wrath stock FrameXML, addons and rebinding remain unsupported.
- Native root/unroot acknowledgements preserve exact pending-order FIFO, mover/counter checks, epoch fencing and independent control denial. Initial UNROOT is legal on reconnect (AzerothCore's logout-timer cleanup); it does not grant control and waits for a real self pose.
- Fresh no-install gates: 4,275 app and 314 protocol tests passed, ten ignored; formatting, diff checks and strict library Clippy passed. Launcher build passed. Full workspace/player-build/platform gates were not rerun. The dependency `block v0.1.6` still has a future-incompatibility warning.
- Dedicated Benchone graphical ground2: 90-second run exited zero. W/S emitted START/STOP and A/D emitted START TURN/STOP TURN. Last stationary reported pose was `(-8949.03, -132.49, 83.61)`, facing `6.2801`. Escape requested normal non-instant logout at 55 seconds; the server completed logout and the client returned to character select about 19 seconds later. No unsupported-order/session-loss event occurred.
- Separate 80-second no-input reconnect exited zero, with restored self position `(-8949.028, -132.493, 83.61113)`, facing approximately `6.2801`, matching the last stopped trace at its printed precision. No frame-poison/control-order disconnect occurred. Avatar census showed attached, settled player meshes; counts varied between two and three across census frames.
- Both runs preserved all 157 installed-data path/size/mtime records. This is not a content-hash guarantee. No realm changes, GM actions, combat, database mutations, normal accounts or commits were used.

Private evidence: `/tmp/azbench-groundfix-v3-{tests,clippy}.log`, `/tmp/azbench-groundfix-v3-launcher.{log,exit}`, `/tmp/azbench-ground2.{log,trace,png,exit}`, `/tmp/azbench-ground2-reconnect.{log,trace,png,exit}` and each run's data manifests. Screenshots remain outside git.

Remaining gaps: incomplete NPC/prop geometry; rejected M2 lights, particle emitters and blend-map overrides; remaining Vanilla DBC layouts (including loading-screen/taxi catalogs); missing Wrath stock UI/gameplay verbs. This gate does not establish combat, quests, loot, swimming/jumping/flight/transports, full UI or general human-play readiness.

## Bounded Wrath authored-addon evidence — 2026-10-04

The later profile-scoped addon host supersedes the earlier statement that Wrath bypasses every Lua VM. It still does not load stock FrameXML or grant gameplay/secure APIs. Vanilla remains the default; native Wrath ground input remains host-owned alongside the bounded authored-addon VM.

Final8 gates passed 6,301 UI/app/protocol library tests (11 ignored), strict all-target Clippy and launcher build. Three dedicated-account graphical runs exited zero. The actual AzerothBenchObserver TOC/Lua/XML observed Benchone's admitted player data, dispatched a pointer-originated sample, persisted SavedVariables through normal logout/reconnect and ordinary ReloadUI, and retained bounded W/S/A/D and server-confirmed logout. Reconnect restored the stopped pose at trace precision. Recorded install-data metadata remained unchanged.

See [WRATH-ADDONS.md](WRATH-ADDONS.md) for exact counts, private artifacts and limits. Pixel-text legibility of the status/button and connected overlay was subsequently measured with macOS Vision OCR on all three screenshots; human visual review and other resolutions remain unqualified. The historical optional hover self-check could not evaluate its unavailable API; a subsequent rebuilt pointer/DBC slice passed 6,775 library tests and three more bounded graphical runs, including actual `GetMouseFocus` assertion, cursor coordinates, reload persistence and logout/reconnect. The later identity-reader slice passed 6,778 library tests, strict Clippy and launcher gates; three further runs verified the observer's truthiness-normalized `UnitIsUnit` self observation, real pointer checks, reload persistence and native ground/logout/reconnect. Exact predicate return-type fidelity remains unqualified. Precise evidence and the remaining base-only loading-art limitation are recorded in WRATH-ADDONS.md. Known asset-loader errors remain. This is partial authored-addon integration evidence, not stock-addon compatibility, complete rendering, combat/quest acceptance, adversarial-addon isolation or general human-play readiness. No realm changes, GM actions, database mutations, install writes or commits were made.

## Data and safety

No proprietary assets, extracted client datasets, stock FrameXML or credentials in git/images. Read user installs without modifying them. Provide compatibility validation/import, not an unverified promise of a lawful historical-client download source. Current official Classic data is not presumed compatible.

The owner authorized current-realm development testing, shutdowns and restarts on 2026-10-02. Use a dedicated test account, never reuse player credentials, preserve existing player state and take backups before migrations. Destructive resets require specific approval. Upstream probes may invoke GM commands: those are development diagnostics only, never benchmark actor controls. Baseline tests run without WOW_DATA and without login credentials. World snapshots do not guarantee deterministic replay.

## Local development constraints

Canonical planning: Obsidian Projects/azerothcore-playerbots/notes/azerothbench-client-and-agent-interface.md. Logs/build outputs remain outside committed files. Check disk space before full Bevy builds; use bounded jobs and keep the live realm untouched. Do not delete other projects' caches/data to make space without approval.

## Read-only host CVar slice — 2026-10-04

The bounded authored Wrath addon surface now reads only `uiScale`, `gamma`, `mouseInvertPitch` and `mousespeed` through case-insensitive `GetCVar`. These are host registry strings, not effective clamped consumer measurements or stock Wrath defaults. Internal rows, `useUiScale`, defaults and mutation APIs are unavailable. Host seeding precedes actual addon execution on entry and ReloadUI; missing required rows fail the observer's file-scope assertions without fallback values. Vanilla behavior is preserved.

Final2 qualification passed **6,784 tests / zero failures / 11 ignored**, formatting/diff checks, strict all-target Clippy and launcher build. Three dedicated Benchone runs (`/tmp/azbench-cvar-{live,reconnect,postreload}.*`) exited zero, verified file-scope/current/sampled `uiScale="0.91"`, retained all four strings and one click through loads 1 → 2 → ReloadUI 3 → separate process 4, and preserved native ground/logout/reconnect behavior. Stopped/reconnected pose matched at trace precision: `(-8946.54,-132.36,83.70)`, facing `0.0598`. Installed-data metadata matched all 157 recorded files; OCR is only machine-readable text evidence. Detailed policy, tests, artifacts and runtime limits are in `WRATH-ADDONS.md`. This does not establish broad gameplay, stock FrameXML or human visual acceptance; existing rendering/backtrace/glue stalls and `block v0.1.6` warning remain. No commits or realm/database/install changes occurred.

## Macro-catalog lifecycle stall — 2026-10-04

Main-thread samples traced the earlier 30,833 ms / 36,066 ms lifecycle hitches through `ui_macro::load_icon_catalog` → `load_macro_icons` → full `Chain::list` archive resolution. Wrath does not expose the macro chooser APIs, so it now skips this catalog entirely. Vanilla retains the chooser, with successful and failed immutable catalog results cached by weak archive-source identity across VMs; each replacement VM is still seeded, and replacement archive sources invalidate the cache without retaining old archives. Candidate filtering now precedes archive resolution while preserving priority, tombstones, normalized deduplication and ordering.

Fresh Final4 gates passed **6,790 library tests / zero failures / 11 ignored**, formatting/diff checks, strict four-package all-target Clippy and launcher build. An explicit Wrath installed-data probe verified exact full/filtered icon-catalog equivalence: 208,055 resolution calls became 1,103 (**188.6× fewer**). Full enumeration took 982.94 ms and filtered enumeration 178.78 ms in this probe; it ran full-first, so timing is warm-cache-biased, not a cold-start benchmark (`/tmp/azbench-stall-timing.log`).

Matched dedicated-account runs (`/tmp/azbench-stall-{live,reconnect}.*`) exited zero, retained observer data/clicks and CVar assertions, performed native ground movement and normal logout, and restored the stopped pose `(-8945.94,-132.33,83.70)`, facing `0.0709`, at trace precision on reconnect. The reconnect run also invoked ordinary ReloadUI. Largest logged post-logout frame hitches were 4,611 ms and 4,664 ms respectively, versus the earlier 30,833 ms and 36,066 ms. These are observed frame-hitch maxima, not logout wall-clock latency or controlled statistical benchmarks. No macro-catalog stack remained in these samples; multi-second streaming/rendering/backtrace stalls remain, so smooth teardown and general gameplay readiness are not established.

The remaining sample `stall-77914-82881.txt` identifies wgpu-core debug `snatch::trace::LockTrace::enter` → `Backtrace::capture` on the render path, not an Avian relationship failure. Earlier teardown-order speculation is not established: ordinary streamed static colliders co-locate `Collider` and `RigidBody`, and pinned Avian 0.6.1 explicitly tolerates missing relationship targets. No speculative despawn reorder or error suppression was applied. No realm/database/install changes or commits were made.

A third run (`/tmp/azbench-stall-postreload.*`, exit zero) verified loads=4, clicks=1 and persisted identity/CVars after the previous ReloadUI; its largest post-logout hitch was 5,890 ms and the bounded harness required its existing hard exit five seconds after AppExit. A separate diagnostic control (`/tmp/azbench-stall-backtrace-control.*`, exit zero) used the identical binary with `RUST_LIB_BACKTRACE=0`, retaining `RUST_BACKTRACE=1` for panic diagnostics. It verified loads=5 and ordinary logout; largest logged post-logout hitch was 448 ms. This disables non-panic `Backtrace::capture`, including debug lock history; it is a single warm-process-order comparison, not proof that all residual stalls are fixed. No runtime default or production source was changed for this control. Real-physics collider lifecycle regressions were subsequently added in `terrain_stream/collider.rs`; their separate focused world-package run passed three tests (attachment plus same-entity and parent/child teardown), zero failures, exit zero (`/tmp/azbench-teardown-tests.log`). No errors reached the configured application error handler; Avian's internally handled command races are not counted by that handler, so this is not proof of zero internal error construction or backtrace captures. These tests are not included in the Final4 counts above; full world-package and fresh strict Clippy coverage of the added tests were not run.

## Wrath atlas, external animation and NPC spline gate — 2026-10-04

Wrath clothing composition scales authored 256-space regions onto the installed 512×512 skin atlas; Vanilla placement stays unchanged. Coverage-premultiplied bilinear resampling and density-matched mip selection are implementation choices, not verified stock-client kernels. The read-only appearance probe reported torso 25,713/32,768 changed pixels and pants 32,768/32,768, with nine composite mips. Strict external animation parsing follows canonical sequence aliases and sibling `.anim` offsets rather than interpreting external offsets in M2 bytes. Installed HumanMale used 47 dependencies / 156 sequences / 130,414 quaternion keys, with zero invalid norms. The current renderer eagerly admits all clips; stock-style lazy dependency loading remains deferred and missing dependencies still fail closed.

Wrath NPC paths preserve explicit interpolation separately from Z authority and optional run-mode metadata. Compressed linear points use the AzerothCore midpoint(start, endpoint) minus packed offset. Non-finite/oversized paths are rejected; unsupported cyclic/effect paths are fenced without fabricating a stop acknowledgement. A raw packet→adapter→actual object handler→spline sampler ECS fixture verifies midpoint/facing and a real subsequent stop. Fresh visual-final2 gates passed **7,521 library tests / zero failures / 17 ignored**, formatting/diff checks, strict seven-package all-target Clippy, and launcher build. Dependency `block v0.1.6` future-incompatibility warning remains. Logs: `/tmp/azbench-visual-final2-{fmt,tests,clippy,launcher}.{log,exit}`.

The bounded dedicated Benchone run `/tmp/azbench-visual-main1.*` exited zero and returned normally to character select. Observer preflight verified GUID 0x421, HP 60/60, level 1 and build 12340; actual report pointer loads, refreshes and ReloadUI replacement-VM counters passed. Short W/S/A/D inputs produced ground movement/turn start and stop packets. Unit census recorded the self drawable (two meshes) and multiple drawable NPCs changing world positions across samples: for example GUID 0xf13000066a0015ef changed from (-8927.65,-118.06,82.19) to (-8998.91,-85.84,85.88). This establishes numeric live motion/render attachment, not human visual approval, measured gait quality or broad gameplay acceptance. Screenshots 000–014 were saved outside the install, including post-logout captures; they have not been visually judged here. Installed-data size/mtime manifests matched (not content hashes). No realm/database/install changes or commits occurred.

Character-selection environment M2s still reject unsupported lights; other assets reject particles/blend-map overrides, and several ancillary DBC schemas remain incompatible. Character-screen appearance, all NPC models, stock Wrath FrameXML, combat/quest play, animation fidelity and reconnect persistence after this specific run remain unqualified. Earlier offline gates or successful movement traces do not waive these limits.

## Human inspection — 2026-10-04

Shannon inspected the interactive, non-probe Benchone client (`/tmp/azbench-human-inspection1.log`, ordinary 1280×800 window). Reported: no further texture issues observed in the inspected scene; startup/server-selection connection worked; NPCs visibly moved; mob selection worked; mouse-plus-keyboard movement felt correct except jumping. This is human-observed scene-specific acceptance, not an exhaustive texture/model or motion-fidelity claim. Selection is currently local presentation; it does not qualify server-side selection or combat.

Reported missing: ordinary action bars, Escape menu, character/profile UI, jumping and right-click attack. Source confirms intentional boundaries: `net::NetCommands::supports` and `net::io::wrath::route` do not admit SetSelection/AttackSwing/AttackStop; controller jump admission remains Vanilla-only and Wrath movement refuses airborne tails. The native Wrath overlay currently maps Escape to safe logout, not a game menu; stock Wrath FrameXML/gameplay APIs remain unported. These are next implementation slices, not an invitation to remove fences or borrow Vanilla wire assumptions. No runtime intervention or realm changes were performed during this inspection.

Next recommended vertical slice: real server target/auto-attack and server-confirmed combat feedback with visible player/target state, followed by loot; independently extend jump/fall/land movement with tested Wrath serialization and authoritative echo/reconnect behavior. Incrementally replace the safety overlay with supported gameplay UI, beginning with Escape menu, player/target frames and action controls, then inventory/character/spellbook and quest interaction. Completion milestone remains one ordinary starting-zone quest: accept, fight, loot and turn in without GM/debug assistance. Full stock secure/taint UI compatibility remains separate work; do not stub unsupported calls to make stock modules appear loaded.

## Attended jump and solo-loot disconnect — 2026-10-05

Shannon's ordinary 1280×800 AZBENCH/Benchone run `/tmp/azbench-human-jump-loot2.*` confirmed **jumping works**. No agent inputs, timed combat, focus takeover, privileged actions or gameplay rescue were used. Server-confirmed melee packets included a lethal swing; at t26.909 the clicked mob was dead and lootable. The reader rejected `SMSG_LOOT_RESPONSE` (`0x160`) slot type4 at01:18:26UTC and deliberately disconnected/returned to login. This was not a process panic; later window closure exited0. The reported weird corpse animation remains undiagnosed: detailed movement-trace animation summaries track the self/mount, not that mob, and cannot establish its final pose.

AzerothCore `LootMgr.h` defines slot type4 as OWNER, and `LootMgr.cpp::operator<<(LootView)` emits it for OWNER_PERMISSION. The native codec and manual-take producer now accept ALLOW0/OWNER4 while preserving the raw type. Roll1/master2/locked3/unknown rows remain rejected. New regressions cover raw OWNER packet decoding and both adapter surfaces, truncation/trailing-byte rejection, duplicate-safe manual takes, unsupported-row refusal, and raw packet → actual response/take/removal handlers. Rows are not removed until a server acknowledgement.

Final owner-fix2 gates: **6,687 passed / zero failed / 11 ignored across47 result groups** for scoped app/protocol/UI library and integration tests with `WOW_DATA=`; a separate installed-data gate ran the two actual stock LootFrame tests without skips. Formatting, diff checks, strict three-package all-target Clippy and native launcher build all exited0. Dependency `block v0.1.6` future-incompatibility warning remains. Logs `/tmp/azbench-loot-owner-fix2-{fmt,diff,tests,installed,clippy,build,chain}.{log,exit}`. Full workspace/player-build/Linux gates were not rerun.

At this checkpoint the corrected client had not been relaunched; the attended retest below supersedes that status. All157 installed-data path/size/mtime records remained unchanged, not a content-hash claim. The realm and stock WoWSilicon client were left untouched; no database/install changes or commits were made.

## Attended item-loot/menu acceptance and corpse diagnosis — 2026-10-05

Shannon tested the corrected ordinary AZBENCH/Benchone client `/tmp/azbench-human-jump-loot3.*` and reported that item loot worked without disconnect, logout/login worked, and menu Exit worked. Logs confirm normal logout completion twice and final exit0. This is scene-specific human item-loot/lifecycle acceptance, not money-loot, group-loot, inventory, quest or full stock-UI qualification. The agent sent no input, focus takeover or automated combat. The user then requested corpse investigation with the client closed, without another launch.

The actual killed wolf entity2933v0 armed Death once at02:38:35UTC. There was no demonstrated Death→Stand re-arm; detailed movement animation summaries remain self/mount-only. Read-only installed `Wolf.m2` v264 metadata identifies Death1, duration1833ms, flags0x23. The Wrath adapter incorrectly treats flag0x20 as looping, which makes the death driver call `active.repeat()` and wrap the death motion forever. Flag0x20 describes resident/in-model animation data, not repetition: [wowdev M2 flags](https://wowdev.wiki/M2#Flags), [WebWowViewerCpp loading](https://github.com/Deamon87/WebWowViewerCpp/blob/1a8cccbeffc46231c6497e6b3f5bfbf3507d8071/wowViewerLib/src/engine/managers/animationManager.cpp#L924-L935), and [separate replay budget](https://github.com/Deamon87/WebWowViewerCpp/blob/1a8cccbeffc46231c6497e6b3f5bfbf3507d8071/wowViewerLib/src/engine/managers/animationManager.cpp#L1570-L1580).

The scoped Wrath death arm now explicitly requests `RepeatAnimation::Never`: witnessed deaths play once and hold their terminal sample; corpses first streamed in already dead seek directly to the terminal pose. Feign death uses the same hold and releases when the server flag clears. Vanilla playback is unchanged. No speculative completion latch or unverified bit0 inversion was introduced.

Five new regressions run real Bevy animation clocks plus the production rig-pose evaluator: witnessed death over many clip periods, streamed corpse with omitted zero health, feign/release, Vanilla isolation, and actual installed wolf metadata passed through the render adapter into a synthetic pose fixture. Before the fix all four Wrath cases failed on Forever; Vanilla passed. The terminal test pose is distinct from both bind and Stand, preventing a bind-pose false positive. Installed assets remain private/read-only and are not fixtures in git.

Known follow-up: both Wrath `ModelAnimation.looping` and `SeqSlot.looping` still carry the legacy residency-derived hint for other consumers. These sites now explicitly document the debt. A general correction requires separating playback operations and authored replay budgets from sequence storage and material clocks; neither bit0 inversion nor globally clearing the hint is supported by this slice. Attacks/gaits already choose their own playback policies in the driver. Corpse human visual acceptance remains pending a later requested retest; this diagnostic run does not reopen the client.

Final corpse-final4 gates passed: **6,692 scoped app/protocol/UI library and integration tests, zero failures, 11 ignored across47 result groups** with `WOW_DATA=`. Separate required installed-data gates ran all five death regressions (including actual Wolf metadata, no skips) and both actual stock LootFrame tests. Strict three-package all-target Clippy, launcher build, formatting and diff checks exited0. All157 installed path/size/mtime records matched; no content-hash claim. Logs `/tmp/azbench-corpse-final4-*`. No client relaunch, realm/database/install change or commit occurred. Full workspace/player-build/Linux gates were not rerun. Earlier full test gate caught a nested-test module being counted as production API; explicit cfg(test) declaration corrected that without raising the API ceiling. Strict Clippy subsequently requested as_chunks for the new metadata fixture; corrected and reran all gates. Dependency block0.1.6 future-incompatibility warning remains.

## Attended corpse acceptance and player sword-overlay release — 2026-10-05

Shannon requested another ordinary AZBENCH/Benchone handoff and reported **mobs correctly die** in `/tmp/azbench-human-corpse1.*`. This closes the scene-specific corpse visual retest, not general animation fidelity. The same run exposed a separate player symptom: the torso remains stuck mid-sword swing. The user subsequently closed the client; exit0 and all157 installed path/size/mtime records matched. No agent input, focus takeover, automated combat, realm/database/install change or normal-client intervention occurred.

The self1405v2 trace identifies Attack2H18 playing full-body at194.857, then transplanted to the key-bone twin at195.282 with0.425s elapsed on a zero-speed turn flag0x20. `upper=Some(18)` persists through Ready27, Stand0 and later logout Pose96/97. Unlike the corpse arm, transplantation already sets `RepeatAnimation::Never`; the error is the completion predicate. The residency-derived Wrath `AnimClip.looping` hint wrongly selects movement-only release, while turning is not horizontal movement. The finished, weight8 upper node consequently keeps contributing its terminal pose over a changing base.

The narrow driver change retains the legacy hint-based movement release only for Vanilla. Wrath one-shot overlays now retire when their armed `Never`/`Count` clock finishes, holding the terminal sample through the existing150ms fade and then dropping the node. Explicit driver-owned cast holds (`Overlay.looping`) remain hold-controlled. No global M2 flag reinterpretation, material-clock change, finished-pose exclusion or speculative latch was added.

Seven regressions use the real Bevy animation clock and production rig evaluator: trace-shaped turn transplantation and Ready/Stand restoration, moving swing without early release, counted replay budget, explicit cast hold, Vanilla isolation, re-arm during the release fade, and actual installed HumanMale Sword18 raw metadata through the strict render adapter into a controlled pose fixture. Terminal, base and bind poses differ; masked legs also differ from full-body swing legs. Three pre-fix Wrath tests failed for the reported persistence or premature movement release; controls passed. Required installed-data green2 ran all seven with no skips. The counted fixture uses frame-sized steps because Bevy counts at most one completion per update, even when one large delta crosses several spans.

Independent read-only review found no production or test-design blocker. Final3 gates passed: **6,699 scoped app/protocol/UI library and integration tests, zero failures, 11 ignored across 47 result groups**. Required installed-data sword7 (green2), death5 and stock LootFrame2 tests passed without skips. Strict three-package all-target Clippy, native launcher build, formatting, diff checks and all157 installed path/size/mtime comparisons exited0. Logs `/tmp/azbench-sword-final3-*`; dependency `block v0.1.6` future-incompatibility warning remains. Full workspace/player-build/Linux gates were not rerun.

Shannon requested the corrected client again in `/tmp/azbench-human-sword1.*` and reported **“much better”**, accepting the sword correction for this attended scene and requesting a source checkpoint before the next UI round. No automated inputs or timed exit were used; the client remains under human control. This accepts the reported animation symptom, not all animations or broader gameplay/UI parity. The dialect guard contains our residency-to-looping metadata error; it is not evidence of fundamentally different Wrath sword behavior. Separate storage/residency, sampling and requested playback policies before removing the shared metadata debt. The preserved Vanilla movement-release branch remains untraced.
