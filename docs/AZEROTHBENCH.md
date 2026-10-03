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

## Data and safety

No proprietary assets, extracted client datasets, stock FrameXML or credentials in git/images. Read user installs without modifying them. Provide compatibility validation/import, not an unverified promise of a lawful historical-client download source. Current official Classic data is not presumed compatible.

The owner authorized current-realm development testing, shutdowns and restarts on 2026-10-02. Use a dedicated test account, never reuse player credentials, preserve existing player state and take backups before migrations. Destructive resets require specific approval. Upstream probes may invoke GM commands: those are development diagnostics only, never benchmark actor controls. Baseline tests run without WOW_DATA and without login credentials. World snapshots do not guarantee deterministic replay.

## Local development constraints

Canonical planning: Obsidian Projects/azerothcore-playerbots/notes/azerothbench-client-and-agent-interface.md. Logs/build outputs remain outside committed files. Check disk space before full Bevy builds; use bounded jobs and keep the live realm untouched. Do not delete other projects' caches/data to make space without approval.
