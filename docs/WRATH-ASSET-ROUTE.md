# Headless Wrath asset route

`crates/wrathplus-formats/tests/wrath_asset_route.rs` is an environment-gated, read-only regression
against the effective installed archive chain. Run with `WOW_CLIENT_PROFILE=wrath`,
`WOW_LOCALE=enUS`, and `WOW_DATA` pointing to the installed `Data` directory:

```sh
cargo test -p wrathplus-formats --test wrath_asset_route -- --nocapture --test-threads 4
```

It visits waypoint tiles at Northshire, Goldshire, Stormwind, Valiance Keep, Howling Fjord and
Dalaran. It meshes terrain with matching WDT alpha flags, adapts placed v264 M2 models with
external skin 00 and all required external animation dependencies, parses every referenced WMO
group, traverses all WMO doodads (including inactive sets), and decodes resolved terrain/model/
ribbon texture mip chains. Missing WMO groups fail explicitly rather than following the older
convenience loader's skip behavior. Diagnostics include the owning tile/model and cumulative
feature counts. Deduplication is case-insensitive within the route.

This is **not** a continuous walk, GPU render test, async Bevy asset-loader test, stock-install
certification, or proof of native visual parity. Dynamic character/creature texture slots remain
unresolved by design. It does not cover all maps, skin LODs, particles, dynamic spell models,
transport destinations, runtime track clocks or world-spawn behavior. Those require separate
feature tests. Successful format adaptation must not be represented as complete ribbon/light/
two-stage runtime parity.

## Archive storage uncovered by the route

The first probe reached the first five waypoints without load failures. Dalaran exposed four
files in the effective `lichking.MPQ` rejected by the old SINGLE_UNIT restriction. Removing that
restriction exposed bzip2 (mask `0x10`) in three of them. The bounded reader implementation now:

- retains the block table's packed byte count;
- treats SINGLE_UNIT as one payload without a sector-offset table;
- handles raw payloads (including COMPRESS-marked data that did not shrink), zlib `0x02`, and
  bzip2 `0x10`, using the same bounded decode path for streaming reads and seeks;
- refuses packed lengths beyond archive bounds, lengths larger than declared output, empty
  payloads for nonempty files, and stored payload/output length mismatches;
- limits resident SINGLE_UNIT output to 256 MiB as an explicit safety policy, not a claimed
  native-client rejection threshold;
- leaves encryption, combined compression masks and other codecs unsupported rather than
  guessing. Existing delete-marker semantics are unchanged.

Synthetic tests cover raw and compressed single-unit reads, seek/re-read, short output, truncated
bzip2, unsupported masks and excessive declared output. Proprietary archive bytes are not test
fixtures and are never committed.

## Validated effective-chain result

The final route regression passed with 0 dependency failures: 1,329 distinct M2 paths, 31 WMO
roots, 487 groups, 1,658 texture paths, 4 adapted ribbons, 5 lights and 107 stage-two submeshes.
These counters describe visited content, not visual acceptance. Feature-unit gates: MPQ library
19 passed; formats library 545 passed; installed route 1 passed; app library 4,489 passed,
12 ignored; zero failures. No game client was launched.
