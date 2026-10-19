# Quality gates: fresh baselines and activation plan

Measured 2026-10-19 on `azerothbench/wrath-hud` 4a007e65 (plus the formats clippy fix 95a121eb)
by Wire B, for `WRATH-NATIVE-PLAN.md` "Tidy-up and quality gates". The tooling is
`scripts/quality/` (README there). Since 2026-10-20 the clippy ratchet is enabled
(step 1 below); no hook is installed and the other ratchets stay opt-in.

## Tools

| Tool | Version | Notes |
|---|---|---|
| `cargo-machete` | 0.9.2 | installed 2026-10-19 (`cargo install --locked`, approved) |
| `typos` (`typos-cli`) | 1.51.1 | installed 2026-10-19 (approved) |
| `cargo-llvm-cov` | 0.9.1 | needs `llvm-tools-preview` for the toolchain; added for 1.98.1 (approved) |
| `cargo-crap` | 0.5.0 | |
| toolchain | 1.98.1 (aarch64-apple-darwin) | |

`python3 scripts/quality/test_gate.py`: 4 tests, OK. Every check runs on today's tree.

Script issues found (all fixed in step 3):
1. `run.sh` defaults `CARGO_TARGET_DIR` to `/tmp/azb-target-experience`, another worker's
   tree. It should default to the caller's own target, or refuse without one.
2. `cargo llvm-cov` prompts interactively when `llvm-tools-preview` is missing, which hangs an
   unattended run. `run.sh crap` should check for the component and fail fast.
3. `cargo machete` reports an IO error for the missing `third_party/kira/tests`. It is harmless,
   but should be excluded (`third_party/`).
4. `clippy -D warnings` stops at the first crate that fails, so a red crate hides every
   downstream diagnostic. The baseline below uses a warnings-mode run; the ratchet run goes
   crate by crate until the tree compiles under `-D`.

## Baselines

| Gate | Committed baseline (base 68529487) | Now (4a007e65) |
|---|---|---|
| LoC: tracked `.rs` files / lines | 1,766 / 786,664 | 2,187 / 901,960 |
| LoC: files > 500 lines | 530 | 582 (205 > 1,000, 30 > 2,000; max 6,142) |
| LoC vs baseline | n/a | 52 new offenders, 260 grown, 9 shrunk, 0 fixed |
| clippy (warnings, workspace, all targets) | 30 diagnostics, 12 lints | 121 unique diagnostics, 37 lints, 89 files |
| CRAP > 10 | 5,198 (static zero-coverage upper bound) | 3,187 of 16,271 functions (measured) |
| line coverage (lib tests) | none | mean 64.6% per function; 4,991 functions at 0% |
| unused dependencies (machete) | not measured | 0 |
| typos | not measured | 2,779 findings |

**LoC.** The ratchet was never enforced, so it drifted. Offenders by crate: app 293, ui 126,
protocol 52, world 49, formats 39, others ≤ 9. The largest file is the generated
`benilla-protocol/.../layout_generated.rs` (6,142 lines), which should be exempt as generated.
The largest growth is in `net/io/wrath.rs` (+2,950), `wrath_authority.rs` (+863),
`wrath_ground.rs` (+493) and `ui_chat/feed.rs` (+383). The largest new offenders are
`wrath_lfg/queue.rs` (1,405), `wrath_calendar/bindings.rs` (1,386), `ui_calendar.rs` (1,061)
and `wrath_admission_tests.rs` (961).

**clippy.** `gates.sh`'s clippy rung and CI's `cargo clippy --workspace --all-targets -D warnings`
are red on wrath-hud. benilla-formats stopped the run with 8 errors; 95a121eb fixes those. After
it, the `-D` run still stops in formats' `wrath_material` test, benilla-protocol (7) and
benilla-ui (11). The 121 warnings-mode diagnostics by crate: app 61, ui 21, formats 18, world 13,
protocol 8. The top lints are `type_complexity` 18, `chunks_exact_to_as_chunks` 16,
`field_reassign_with_default` 8, `needless_update` 7, `manual_is_multiple_of` 6,
`result_large_err` 6 and `dead_code` 6. Most are mechanical; the toolchain's newer lints
(`chunks_exact_to_as_chunks`, `manual_is_multiple_of`) account for a good share.

**CRAP.** Measured with `cargo llvm-cov --lib` over the workspace (about 18 minutes under the
lock, 19.7 MB of LCOV), scored by `cargo crap --missing pessimistic`. Offenders by crate: app
1,495, formats 553, protocol 443, ui 322, world 270, assets 38, others ≤ 18. The top scores are
the 1.12 codec that Vanilla-removal step 3 deletes (`messages/parse.rs` `parse_server_body`
300,883, `ServerPacket::name`, `events/decode.rs` `decode`). After those come `bin/`/`examples/`
mains (pessimistic, not run by lib tests), `player/controller.rs` `control`,
`ui_chat/input` `drain_chat_input` and `net/io/wrath.rs` `run`. Integration tests do not count
towards coverage (`--lib`). The report is in a `/tmp/azb-quality-report.*` directory, not
committed.

**typos.** Most findings are domain words, not typos:
- `lod` ×2,018 (`LOD` 51, `Lod` 21);
- Blizzard API spellings: `Hilight` 52 (`hilight` 20), `SUPERCEDED`/`superceded` 47
  (`SMSG_SUPERCEDED_SPELL`), `Compleated` 17, and probably `Portait` 25;
- two-letter hex and abbreviation fragments: `fo`, `iy`, `toi`, `anc`, `ba`, `inh`.

1,998 of the findings are in `reference/1.12-globals.tsv`. Likely real typos include `Encrypter`
(23, `benilla-srp`), `unparseable` (48), `hander` (14), `boxs` (9) and `numer` (8). Per the
manager, these are recorded, not fixed.

With `_typos.toml` (step 2: `reference/*.tsv`, `third_party/` and the baselines excluded; `lod`,
the Blizzard API names `Hilight`, `Compleated`, `Portait`, `SUPERCEDED`, `Laguages`
(`GetNumLaguages`), `QUITING` (`PLAYER_QUITING`), and short code identifiers allowed), the count
is **210**. Those 210 are the typos baseline, recorded not fixed. They are mostly real misspellings
and truncations: `unparseable` 45, `mis` 28, `hander` 14, `hilights` 13, `boxs` 9, `childs`,
`Proccess`, `precessed`, `texure`, `THREHOLD`, `permanantly`, `Enviromental`.

**machete.** Clean (scanning `crates/`, step 3).

## Activation plan

Order: after Vanilla-removal step 3 (it deletes the top CRAP offenders and thousands of lines)
and the rename (`WRATHPLUS-RENAME.md`). Per the README, the rename is followed by one explicit,
reviewed path migration of the baselines, never a silent reset. Steps 1-3 can happen now.

1. **clippy back to green** (done, 2026-10-20). Mechanical fixes crate by crate (95a121eb,
   94f4e77f, c27b4957), the owner-held items with their owners (f7b57e65), then one regeneration
   of `clippy-baseline.json` from a warnings-mode workspace run: its only entry is
   `SessionDialect::Vanilla5875`'s `dead_code`, which App Migration's last step-4 batch deletes.
   The clippy rung of `gates.sh`, `check.sh` and CI is now `scripts/quality/run.sh clippy`: every
   warning, rustc's or clippy's, fails unless that baseline names it, so the count only shrinks.
2. **typos config** (done). Add `_typos.toml`:
   - exclude `reference/*.tsv` and `third_party/`;
   - allow `lod`/`LOD`/`Lod` and the Blizzard API spellings (`Hilight`, `SUPERCEDED`,
     `Compleated`, verified against `GlobalStrings.lua`/FrameXML);
   - ignore short hex identifiers.
   Then record the remainder as the baseline, and fix real typos with their owners.
3. **Script fixes** (done). Make `run.sh` default to the caller's own target dir, fail fast
   without `llvm-tools-preview`, and exclude `third_party/` from machete. Exempt generated files
   (`layout_generated.rs`) from LoC by an explicit list.
4. **Re-baseline LoC and CRAP** after step 3 of the Vanilla removal, from measured runs on that
   tree. CRAP from LCOV, replacing the static upper bound as the README asks; LoC from
   `run.sh loc`. Commit them as the ratchet budgets.
5. **Path migration** after the rename: one reviewed commit that rewrites the baselines' paths
   `benilla-*` → `wrathplus-*`. Hashes stay valid, since file contents are unchanged by a move.
6. **Wire into `gates.sh` and CI**, not a parallel system:
   - add `loc`, `clippy-report` (the allowlist filter on the existing clippy rung), `machete`
     and `typos` rungs to `gates.sh`, with the bare-runner ones mirrored in CI;
   - CRAP stays on demand and pre-merge (`run.sh crap -p <touched crates>`), out of the hook and
     the fast chain;
   - then install `scripts/quality/pre-commit` (LoC, rustfmt, typos on the staged snapshot).
7. **Burn-down.** Split oversized files by responsibility, crate by crate, starting with
   `net/io/wrath.rs` and the new offenders. Cover or simplify high-CRAP functions as the units
   touch them; the baselines may only shrink.
