# AzerothBench: native WotLK 3.3.5a client plan

Status: agreed with Shannon 2026-10-14. Supersedes the "adapt 3.3.5 onto 1.12" approach for state and
UI. Companion to `docs/WRATH-PLAN.md` (history, server issues, done list).

## Why

The client so far translates build 12340 into benilla's 1.12-shaped internals: 3.3.5 descriptor fields
are copied into 1.12 indices (`wrath_events::projected_pairs`), NPC flags and dialog statuses are
renumbered, the quest log is repacked into 1.12's 20 slots, and the Lua API is 1.12's with shims
(`script/wrath_compat.lua`). That got a playable HUD quickly, but it:

- **loses data**: 25→20 quest slots, 128→64 explored-zone words, 6-bit quest counters, and nothing
  for dailies, auto-accept, honor/talent/arena rewards, extended costs, threat, vehicles, glyphs,
  achievements, runes, multiple texture stages;
- **keeps producing one class of bug**: wherever 3.3.5 behaves unlike 1.12 (re-parent visibility,
  `RegisterForClicks("AnyUp")`, modified attribute lookup, `<NormalFont style=…>`, `questnpc`), the
  adaptor patches one instance and the next waits to be found by playing;
- **makes the eval measure 1.12**: agents would be graded against a 3.3.5 client whose state and
  API are 1.12 underneath.

## The decision: hybrid

- **Keep translating at the wire boundary** where a 3.3.5 message means the same thing as a 1.12
  one: decode into the shared `SessionEvent`s (names, spell go, loot, vendor list, gossip). That is
  a codec concern and the consumers are correct.
- **Go native for state and API**:
  1. **Descriptor**: objects keep their 3.3.5 field indices; code reads fields through a semantic
     layer (`Field::UnitHealth`, …) resolved by a per-dialect layout. No projection.
  2. **Lua API**: implement the 3.3.5 signatures directly in Rust for the Wrath dialect (the
     installed FrameXML is the spec), retiring the `wrath_compat.lua` shims.
  3. **Engine/widget semantics**: the Wrath dialect follows 3.3.5 widget rules wherever they differ
     from 1.12, found systematically rather than by play.
- Vanilla (`ScriptDialect::Vanilla`, `SessionDialect::Vanilla5875`) keeps working and its tests stay
  green throughout. Every dialect fork is explicit.

## Workstreams

Each workstream is owned by one worker bot, on its own branch and worktree off
`azerothbench/wrath-hud`. The manager (session "Revise Azerothbench") reviews and merges.

### WS1: native descriptor, protocol layer (blocks WS2)
Crate `benilla-protocol` (`messages/update_object/fields/*`, `wrath_events.rs`, `wrath_auras.rs`).
- A `Field` enum naming every descriptor field the client reads (object, item, container, unit,
  player, gameobject, dynamicobject, corpse), and a `FieldLayout` per dialect mapping
  `Field` → index (+ width). Sources: vmangos `UpdateFields.h` (1.12) and AzerothCore
  `UpdateFields.h` (3.3.5).
- `ObjectFields` carries its dialect's layout; every accessor reads through it. Packed values whose
  shape changed (quest log slots, aura data, power types, bytes fields) get dialect-aware accessors.
- `WrathEventAdapter` stops projecting: objects carry raw 3.3.5 fields plus the Wrath layout.
  `npc_flags_1_12`, `projected_pairs` and the quest-log repack go away; accessors return 3.3.5
  semantics (e.g. NPC flags as typed bitflags with 3.3.5 meanings, mapped to whatever the app needs).
- Auras: 3.3.5 auras are packet state, not fields. Replace the `wrath_auras` field synthesis with an
  aura store event (`SessionEvent::UnitAuras`) that the app's aura feed reads for Wrath.
- Done when: vanilla and Wrath fixtures read the same semantic values through one API; no 1.12
  index appears in Wrath code paths; protocol tests green.

### WS2: native descriptor, app migration (after WS1's API lands)
Crates `benilla-app`, `benilla-world`.
- Replace every raw field index (`const F_*: u16 = 147;`, `FIELD_*` imports, `get_u32(N)`; about 256
  constants) with `Field` reads or the WS1 accessors. Remove duplicated per-module constants.
- Field-change watchers (`FieldChanged`, `FieldEdges`) key on `Field`, not raw index.
- Done when: `rg 'const [A-Z_]*: u16 = [0-9]+' crates/benilla-app crates/benilla-world` finds no
  descriptor indices; both dialects' tests green; a live Wrath session behaves as before.

### WS3: native 3.3.5 Lua API (parallel with WS1/WS2)
Crate `benilla-ui` (`script/*`), reading data the app pushes into the `Model`.
- Spec: the installed 3.3.5 FrameXML and Blizzard addons (dump with `AZB_FRAMEXML_DUMP`), plus the
  census `wrath_framexml_tests` "missing surface" list (546 names today).
- Implement Wrath-dialect bindings with 3.3.5 signatures and return shapes, natively. Retire the
  corresponding `wrath_compat.lua` shims. Priority: gossip/quest/quest log/watch frame, spell and
  action (`GetSpellInfo`, `GetActionInfo`, `IsPassiveSpell`), unit (`UnitAura` extras, threat,
  vehicles), chat windows, then the long tail (arena, LFG, achievements, calendar, glyphs, talents,
  equipment manager, currency) as "answers like a character with nothing" where no system exists.
- New data the API needs (quest auto-accept, rewards XP/honor/talents, dailies, quest IDs) is added
  to the `Model` setters; WS4 or the manager wires the app side.
- Done when: the census lists no FrameXML-called global or method; stock frames open without Lua
  errors in a headless census with fixture data; vanilla UI tests green.

### WS4: wire boundary, server messages and client commands (parallel)
Crates `benilla-protocol` (`wrath_*.rs` codecs), `benilla-app/src/net/io/wrath.rs`, `net.rs`.
- Decode the 3.3.5 messages still dropped (see `WOW_CAST_TRACE=1` "unhandled opcode" census):
  power update, threat (update/highest/clear), world states, proficiency, talents info, GO query
  response, loot list, heal/energize/periodic/dispel/interrupt logs, chat, group, trainer, bank,
  taxi, mail, auction, guild, who, friends, duel, trade, LFG status as empty.
- Encode the commands still routed `DisabledGameAction`.
- Layouts from AzerothCore source (protocol reference only; never require server changes).
- Done when: a full Northshire session (questing, vendor, trainer, chat, group invite, death and
  resurrection, hearth) logs no unhandled opcode and no dropped command.
- Coordination: WS1 edits `wrath_events.rs` (projection removal). WS4 adds decoders in their own
  modules (`wrath_npc.rs`, `wrath_vendor.rs` style) and only touches the dispatch arms.

### WS5: engine and widget semantics (parallel)
Crate `benilla-ui` (`loader/`, `widget/`, `script/object*`, `script/event.rs`).
- Systematic audit of where the Wrath dialect must follow 3.3.5 widget rules: script kinds
  (`OnEnable`, `OnDisable`, `PreClick`, `PostClick`, `OnTooltipSet*`, `OnMinMaxChanged`,
  `OnMovie*`), `AnimationGroup`/`Animation`, frame methods missing per the census, XML elements
  and attributes 1.12 lacks (`parentKey` everywhere, `style=`, `<Attributes>`, `<KeyValues>` if
  any, `inherits` lists), re-parent and visibility rules, `HybridScrollFrame` needs.
- Done when: the census lists no missing method or script kind; loader reports no unknown element
  or attribute for the installed UI.

### WS6: assets and data (parallel)
Crates `benilla-formats`, `benilla-m2`, `benilla-assets`, `benilla-world`.
- Remaining DBCs (AreaPOI, Cfg_Categories, ChatProfanity, CreatureSoundData, PetPersonality,
  SpellChainEffects) and any 3.3.5-only tables the API needs (Achievement*, CurrencyTypes,
  GlyphProperties, Vehicle*, LFGDungeons, QuestFactionReward, QuestXP …).
- M2 v264: lights, ribbon emitters, blend-map overrides, batches with two texture stages
  (`generalweaponrack01`, zeppelin), sequences in `.anim` for particles.
- Done when: no asset load failure in a Northshire→Goldshire→Stormwind run; installed-data tests
  cover each mapped table.

### WS7: experience profile and asset overrides (parallel; client-side only)
Shannon (2026-10-14): the client should switch cleanly between the **classic** experience and a
**plus** experience with optional extras (modern graphics, UI and QoL improvements, party play
with agents). Client-side only for now; new lands, races and mechanics (server-side) come later.
Build the seams now; features land on them later.
- **Experience profile**, orthogonal to the data/protocol dialect: `Classic` (faithful stock
  3.3.5, the eval default, pinned for reproducibility) or `Plus`, with named feature toggles in one
  typed registry (config `[experience]`, `WOW_EXPERIENCE=classic|plus`, per-feature overrides),
  queried through one resource, never scattered string checks. Classic must render and behave
  exactly as today; tests assert every Plus feature is off under Classic.
- **Asset override layer**: loose files and mod packs (`<home>/mods/<pack>/` mirroring archive
  paths) take priority over the MPQ chain, per-pack enable list and order, clean fallback, logged
  provenance per overridden file; Classic ignores overrides unless explicitly allowed. Works for
  textures, models, DBCs, sounds and UI files alike (one choke point where the chain resolves a
  path).
- **First Plus feature (the visible payoff)**: a "modern" lighting preset, no new art: real-time
  shadows, height/distance fog, bloom, filmic tone mapping and colour grading toward a darker
  fantasy look, all behind Plus toggles. Must not change Classic's frames.
- Not in WS7: new art (high-detail models, cloth-simulated cloaks), movement/combat mechanics
  (server-side), party-command UI (builds on the agent bench contract).
- Done when: profile switch works end to end; an override pack replaces a texture and a model in a
  live session; Classic screenshots are pixel-identical to before; the Plus lighting preset can be
  toggled at runtime for a side-by-side capture.

## Rules for every worker

- **Repo**: `~/Workspace/artivus/azerothbench-client`. Create your worktree:
  `git worktree add ../azb-<ws> -b native/<ws> azerothbench/wrath-hud`. Never edit the main
  checkout; never push; never commit to another branch.
- **Build/test**: own target dir `CARGO_TARGET_DIR=/tmp/azb-target-<ws>`, `CARGO_BUILD_JOBS=4`,
  `nice -n 15`, tests with `--test-threads 4`. Only one heavy build at a time across workers when
  possible: if another worker's cargo is running (`pgrep -fl cargo`), wait for it.
- **Unit tests** must run with `env -u WOW_CLIENT_PROFILE -u WOW_LOCALE WOW_DATA=` (otherwise
  vanilla tests read Wrath data). Installed-data tests: `WOW_CLIENT_PROFILE=wrath WOW_LOCALE=enUS
  WOW_DATA=/Users/shannon/Games/Client/Data`.
- **Census** (installed UI, headless): `cargo test -p benilla-app --lib wrath_framexml_tests --
  --ignored --nocapture --test-threads 1` with the Wrath env above; `AZB_FRAMEXML_DUMP=<dir>` writes
  the stock sources for reading (never into the repo).
- **Never launch the game client** (`target/azb-live/*.sh`): Shannon plays on the same account. The
  manager runs live checks.
- **Trusted UI**: no taint, no protected frames, no secure-execution emulation.
- **No server changes**: AzerothCore source is a protocol reference only.
- **Never commit** Blizzard assets, dumps, or `.azerothbench-test.env`.
- **Every unit passes `cargo test -p benilla-app --lib` before it is reported** (heavy; take your
  turn), with the counts pasted. Library-only runs miss what the app builds and asserts: struct
  literals, event ownership (`net::handlers`: every `SessionEvent` kind needs exactly one owner),
  and behaviour tests on top of descriptor accessors.
- Commit in small, reviewable steps on your branch, with tests. Report to the manager when a
  reviewable unit is ready: the branch, commits, what changed, test results (pasted counts).
- Follow existing code style: comment density, naming, references to sources (`AzerothCore
  UpdateFields.h`, FrameXML file:line).

## Integration

The manager merges reviewed branches into `azerothbench/wrath-hud` in dependency order (WS1 → WS2;
others as they land), resolves conflicts, runs the full suites and a live session, and records
results in `docs/WRATH-PLAN.md`.

## After the native refactor (requested by Shannon, 2026-10-14)

### Rename: `wrathplus`
The codebase has drifted far from benilla. Once WS1-WS6 are merged (a rename mid-refactor would
conflict every open branch), rename in one mechanical pass: crates `benilla-*` → `wrathplus-*`,
the `benilla` binary, module paths, docs, logs/targets that embed the name. `WOW_*` environment
variables stay (they name the game, not the project). One commit, no behaviour change, full suites
green before and after.

### Tidy-up and quality gates
Shannon's usual stack (as in snapdragon: `.githooks/pre-commit`, `scripts/loc-gate.sh`,
`scripts/crap-gate.sh`, `.cargo-crap.toml`), minus Verus:
- **rustfmt** `--check` and **clippy** `-D warnings` on the touched crates.
- **Line-count gate**: no Rust file over 500 lines.
- **CRAP gate**: complexity² × (1 − coverage)³ + complexity per function, threshold 10, coverage
  from `cargo llvm-cov`, scored by `cargo crap`.
- **Unused dependencies** (`cargo machete`) and a **typos** pass.
- Pre-commit hook checks the staged snapshot, like snapdragon's.

Starting point (2026-10-14): 1,585 Rust files, 740k lines; 516 files over 500 lines (183 over
1,000, 28 over 2,000; largest 4,847). Hard gates would block every commit, so they start as a
**ratchet**: a checked-in baseline lists today's violations (file → line count, function → CRAP),
new or changed code meets the bar in full, the baseline may only shrink, and the tidy-up work
burns it down crate by crate (split oversized files by responsibility, cover or simplify
high-CRAP functions). CRAP on the whole Bevy app is too slow per commit, so the hook scores only
the crates a commit touches; a full run is a manual gate before merges.

### Split the app crate
`benilla-app` is ~393k lines (over half the codebase): game systems, net handling, UI glue and
capture/probe tooling in one crate. As part of the tidy-up (after the rename, so the new crates are
born `wrathplus-*`), split it along its existing seams into a few crates with one-way dependencies,
e.g. net/session handling, gameplay systems (spells, combat, units, items, quests), UI script
glue (`ui_*`), and dev/capture tooling (`capture/`, `addon_harness/`, probes), keeping the binary
crate thin. Goals: faster incremental builds and per-crate gates, clearer ownership. Plan the
seams from the actual module dependency graph first; no behaviour change, suites green per step.
