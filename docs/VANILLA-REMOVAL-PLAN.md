# Vanilla (1.12) removal plan

Prepared 2026-10-19 at `9af6f2db` (wrath-hud), read-only. No code changes. The client targets 3.3.5a
only (`WRATH-NATIVE-PLAN.md`, "3.3.5 only"); this plan removes every 1.12 path in mergeable,
green steps, after the WS3 census settles. Counts are `rg` over `crates/` at that commit and move
as work lands; re-measure before each step.

## 1. Inventory

### Dialect forks

| Fork | Total | Where | Production vs test |
|---|---:|---|---|
| `SessionDialect::Vanilla5875` | 84 | all `benilla-app` | 78 production, 6 test. By module: `player` 40, `ui_loot` 22, `target` 19, `creature_anim` 24, `spell` 16, `ui_party` 13, `ui_social` 7, `net` 10, `ui_mail` 5, `ui_chat` 4, `ui_guild` 3, `names`/`items` 3 each, `go_templates` 2, others 1 each (176 dialect-check sites in total, both arms) |
| `FieldLayout::Vanilla5875` | 637 | app 521, protocol 116 | 445 in `src/` files, most of them in in-file test modules that iterate both layouts; 192 in test files |
| `ScriptDialect::Vanilla` | 17 | ui 16, app 1 | 14 production: `loader/scripts.rs`, `script/event.rs`, `script/stdlib.rs`, `script/addon.rs` (×3), `script/storage_slots.rs`, `script/mod.rs` (×3), `app ui_macro` |
| `UiScript::new()` (Vanilla by default) | ~1,466 | ui and app | almost all test VMs; one production-looking call (`player/camera.rs:2031`) is inside its test module |
| `DataProfile::Vanilla` | 67 | formats 62, assets 3, app 2 | formats: `characters/customization` 13, `lib` 8, `maps` 5, `characters/sections` 5, `items`/`creatures`/`area_table` 4 each, `game_tips`/`characters/geosets` 3 each, `overrides`/`models/m2_batches` 2 each |
| any `Vanilla` | 1,055 | app 714, protocol 194, formats 99, ui 41, assets 5, adt 1, m2 1 | 838 in `src/`, 217 in test files |

### Vanilla-only modules and code

| Area | What | Size |
|---|---|---:|
| Protocol | `messages/` (the 1.12 codec: `parse.rs`, `packet.rs`, `opcode.rs`, `opcode_names.rs`, `client.rs`, per-family 1.12 parsers) | 30,871 lines; `update_object/` is 11,959 of that and is shared (see risks) |
| Protocol | `world/` (the 1.12 world session) | 3,703 |
| Protocol | `auth.rs` (1.12 SRP and realm list; Wrath has `wrath_auth`) | 428 |
| Protocol | 1.12 integration tests (`tests/*.rs` except `wrath_*`) | 26 files, ~9,900 lines |
| App | `net/io.rs` Vanilla reader and writer (`writer_loop`'s 1.12 arms; the Wrath path is `net/io/wrath.rs`) | 1,654 file, ~1,300 Vanilla |
| App | `capture/fixtures.rs` and the 1.12-era probes (mail, auction, bank, vendor swap, BG; see `WRATH-SESSION-CHECKLIST.md`) | 1,685 plus probes |
| Formats/M2 | `benilla-m2` 1.12 parser (`lib.rs` `parse_m2`, `tests.rs`, `model.rs`/`track.rs` 1.12 halves) | ~1,100 |
| Formats | 1.12 particle and ribbon lanes (`particles.rs` non-264 branch, `ribbons.rs` `parse_m2_ribbon_emitters`), `models/records.rs` 1.12 light parse | ~900 |
| Assets | `m2.rs` 1.12 `M2ModelLoader` and `AdtLoader` (selected by `DataProfile::Vanilla` in `benilla-assets/src/lib.rs`) | 683 plus ADT |
| Data loaders | 29 DBC loaders declare 1.12 schemas; `wrath_layouts.rs` maps them onto build 12340's columns (installed-tested by `installed_every_mapped_table_parses_through_its_map`) | maps ~400; loaders shared |
| Data tests | `wow_data_or_skip!` tests against 1.12 data (skip without a 1.12 install) | 1,640: app 1,263, formats 353, assets 12, world 12 |
| Dev tools | `benilla-extract` scanners call the 1.12 `benilla_m2::parse_m2` (`m2dump`, `scan/geometry`, `scan/material`, `scan/skeleton`) | ~3,000 |
| UI shims | `script/wrath_compat.lua`: build-12340 globals over the 1.12 state, installed by `wrath_surface.rs` before the native modules | 597 lines, 94 globals: 15 also bound natively, 79 shim-only |
| UI census surface | the 1.12 reference-surface gates: `script/tests/widget_surface.rs` (with `BEYOND_1_12`), app `ui_script/surface_gate.rs`, `reference/1.12-globals.tsv`; `ui_script/reference_ui.rs` (1.12 reference UI) | 353 + 143 + 2,223 + data |

The 79 shim-only globals include real game state that is now available natively and should become
bindings rather than stubs, for example `GetQuestResetTime` (01CF now decodes the daily reset),
`GetMirrorTimerInfo`/`GetMirrorTimerProgress` (01D9–01DB are shared events), `IsPVPTimerRunning`,
`GetInstanceInfo`, `IsModifiedClick` and `SecureCmdOptionParse`; the rest (Battle.net, voice, map
debug) are legitimately "nothing here" and can move to a named native-absent table.

## 2. Risks: Wrath still running through a 1.12 path

1. **About 30 Wrath opcodes decode through the 1.12 parser.** `wrath_npc` (default arm "Unchanged
   since 1.12": gossip complete, NPC text, quest list/invalid/failed/log-full/update-failed/
   failed-timer/complete/add-kill/add-item, quest confirm-accept, gossip POI, quest push result,
   status multiple), `wrath_session` (XP gain, party kill log, exploration XP), `wrath_vendor`
   (sell, buy, buy failed) and `wrath_spline` (every arm but the flag toggles) call
   `messages::parse_server_with_tail` and `events::decode`. Deleting `messages/parse.rs` or the
   1.12 `ServerPacket` breaks them. They must be ported first (step 1).
2. **Shared types live in `messages`.** `SessionEvent` payloads (`GmTicket`, `MirrorTimerStart`,
   `LevelUpInfo`, `AttackerState`, `TrainerSpell`, `MonsterMoveFacing`, `TransportPose`,
   `dialog_status`, `ChannelNoticeTail`…) and the descriptor model (`update_object/`, which holds
   both layouts) are Wrath's too. `messages` cannot be deleted wholesale; it splits into types
   (kept) and 1.12 codec (deleted).
3. **The defaults are Vanilla.** `DataProfile::default()` is `Vanilla`, and the asset plugin
   registers the 1.12 M2 and ADT loaders when no `ClientDataProfile` resource is set;
   `UiScript::new()` builds a Vanilla VM. Anything constructed without the env (tests, tools, a
   future entry point) silently runs 1.12 semantics. Flip the defaults before deleting variants.
4. **The Wrath UI is the 1.12 surface plus layers.** `wrath_surface::install` runs "the mature
   implementation above", the 1.12 bindings, then the Wrath ABI. Every 1.12 binding a Wrath frame
   still calls is part of the Wrath client; only bindings a Wrath VM never reaches can go. The
   census decides which; this is why UI removal comes last.
5. **Data loaders read through 1.12 column numbers.** The `wrath_layouts` maps are correct and
   installed-tested, so removing the 1.12 file path is safe, but rewriting loaders to native column
   numbers is a separate, larger change. Keep the maps as the column source.
6. **Dual-layout tests.** Most `FieldLayout::Vanilla5875` uses are tests that assert both layouts
   agree semantically; dropping the variant halves them rather than deleting them.
7. **Dev tools.** `benilla-extract` scanners decode only 1.12 M2s; deleting `parse_m2` removes them
   unless ported to `parse_wrath_m2`.

## 3. Removal sequence

Each step is one branch, green on `--lib` and `--tests --no-fail-fast` for every touched crate plus
the app, and merged before the next starts. Owners are suggested by the code each step touches.
Estimates are net lines removed (a minus is an addition).

| # | Step | Touches | Depends on | Est. removed | Conflict note |
|---|---|---|---|---:|---|
| 1 | Port the ~30 "unchanged since 1.12" opcodes into their Wrath modules (own decoders, strict framing, fixtures), and add a test that no `wrath_*` file names `parse_server*` or `events::decode` | protocol `wrath_npc`, `wrath_session`, `wrath_vendor`, `wrath_spline` | — | −700 (added) | Wire A/B own these modules; do it between their units |
| 2 | Flip the defaults: `DataProfile` default Wrath (enUS unless set), asset plugin defaults to the Wrath loaders, `UiScript::new()` builds a Wrath VM (`new_vanilla()` for the few tests that still need 1.12) | formats `profile`, assets `lib`, ui `script/mod.rs` | — | ~0 | Test churn: run the full ui/app suites and fix only what breaks; do it in a quiet window |
| 3 | Delete the Vanilla session: protocol `world/`, `auth.rs`, 1.12 `parse`/`packet`/`opcode*`/`client` and per-family 1.12 parsers; app `net/io.rs` Vanilla reader/writer; keep `messages` types (rename to `types` in a separate mechanical commit); delete the 26 1.12 protocol integration tests | protocol, app `net` | 1 | 35,000–40,000 | One owner, short window; touches `net.rs`/`io.rs` hot files |
| 4 | Collapse `SessionDialect`: delete the enum's Vanilla arm and every `SessionDialect::Vanilla5875` branch (78 sites), module batches by owner (`player`+`target`+`creature_anim`; `ui_loot`+`ui_party`+`ui_social`+`ui_mail`+`ui_chat`+`ui_guild`; `spell`+`net`+rest) | app | 3 | 2,500–3,500 | Done 2026-10-20 (App Migration, 4a): the type is gone, with `supports`' dialect param, every threaded param and stored dialect, and the login request's; `ClientCommand` keeps a labelled `allow(dead_code)` for the 1.12-only variants and fields its producers still build (prune follow-up). 4b done 2026-10-21: the 1.12 UI loader, layer load, single-file saved-variables writer and shutdown folds are gone; harnesses without a data profile load Wrath (`ui_script::load_default_ui`, the live session's seeding); the 1.12 roll model, macro-icon path, `layer_tests`/`surface_gate`/`shape_gate`/`game_menu_addon_tests` and App's `new_vanilla` callers are gone. Left: `reference/1.12-*.tsv` (still read by the event/verb gates, `reference_ui`, `message_keys`, the addon harness and `benilla-ui`), the unloaded layer files (`Addon::layer`, the content hash), the two UI routes (`wrath_native` vs the harness lifecycle) |
| 5 | Drop `FieldLayout::Vanilla5875` from the descriptor model (generated 1.12 tables, field maps) and halve the dual-layout tests | protocol `update_object/`, app tests | 3 | 4,000–6,000 | Done 2026-10-21 (Assets): `FieldLayout` has the one variant; `gen-fields.rs` reads only AzerothCore's header (the Wrath table is byte-identical) and drops the 51 1.12-only `Field`s with their accessors (descriptor auras, honor week/session, combo, loyalty, training points, virtual items, GO/dynobj position); the `FIELD_*` anchors resolve through the Wrath layout; dual-layout tests run Wrath only, 1.12 descriptor-aura fixtures moved to packet snapshots. Left: `layout == Wrath12340` branches whose 1.12 else-arms are dead (`ring::can_attack`, `relations::can_assist`, `ui_aura::buffs_visible_on`, `inventory::StorageSlots`, `quest_markers` refresh, `wrath_notices`), `cursor_mode::npc_flags`'s 1.12 bit encoding (internal), 1.12 doc references in `fields/mod.rs` |
| 6 | Data: remove `DataProfile::Vanilla` (62 formats sites), the 1.12 M2/ADT loaders, `benilla-m2`'s 1.12 parser, the 1.12 particle/ribbon/light lanes; keep `wrath_layouts` as the column maps; delete or convert the 1,640 1.12-data tests (most are deleted; a few become Wrath installed-data tests) | formats, m2, assets, world tests | 2 | 25,000–35,000 (mostly tests) | Assets owns; Experience touches formats occasionally |
| 7 | Dev tools: port the `benilla-extract` scanners that still matter to `parse_wrath_m2`, delete the rest | formats `bin/` | 6 | 1,000–2,500 | None in flight |
| 8 | UI, after the census: replace the 79 shim-only `wrath_compat.lua` globals with native bindings or a named native-absent table, delete `wrath_compat.lua` | ui | census done | ~400 | Done 2026-10-20 (App Migration, moved from Native UI): every global is native or in the reviewed offline-services set (`WRATH-COMPAT-SHIM-AUDIT.md`, test-enforced); the file keeps that set, the stock CVar defaults and the combat-log constants |
| 9 | UI, after the census: remove `ScriptDialect::Vanilla` (14 production sites), the 1.12 reference-surface gates and `reference/1.12-globals.tsv`, `reference_ui.rs` if unused, and every 1.12-only binding the census shows no Wrath VM reaches; replace the surface gate with the 12340 census | ui, app `ui_script` | 2, 8 | 5,000–15,000 | Native UI, last |
| 10 | Rename and tidy: `benilla-*` crate/module names that still say 1.12, docs that describe Vanilla behaviour, the `wrathplus` rename (`WRATH-NATIVE-PLAN.md`) | all | 1–9 | small | Scheduled cleanup |

Rough total: 75,000–100,000 lines removed, more than half of them 1.12 tests and fixtures.

## 4. Verification per step

- `cargo check --workspace --tests --examples --benches --keep-going` clean.
- `cargo test --no-fail-fast` for `--lib` and `--tests` of every touched crate plus `benilla-app`.
- Installed Wrath tests (`WOW_CLIENT_PROFILE=wrath …`, formats `--test 'wrath_*'`, the asset route,
  `wrath_layouts::tests::installed_every_mapped_table_parses_through_its_map`).
- From step 3 on: a recorded capture replayed through `replay_capture` and the app drain shows no
  new unhandled opcode or decode failure (`WRATH-PACKET-RECORDING.md`, "Replay").
- A manager-run live session after steps 3, 6 and 9.
