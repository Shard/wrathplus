# Wrath world-format audit

Status: initial source and read-only installed-data audit, 2026-10-15.
This is a gap inventory, not a claim of native rendering parity. Fixes follow in
small independently tested units; Vanilla behavior remains explicitly separate.

## Reference and evidence limits

Reference data is the effective enUS Wrath archive chain opened with
`Chain::open_wrath` at `/Users/shannon/Games/Client/Data`. It is not certified
untouched stock build-12340 data. Samples below are bounded, not a full census.
The native build-12340 executable is examined statically, never launched.
Binary-derived working files stay outside the repository. Native addresses below
establish only the stated observations; a filename accessor is not schema proof.

## Priority by likely visual impact

| Rank | Area | Current implementation | Gap / next evidence |
| --- | --- | --- | --- |
| 1 | ADT MH2O water/lava | `benilla-adt` parses bounded layer instances, masks, vertex formats and attributes; formats and assets retain them | No conversion into rendered/collision liquid meshes. Resolve LiquidType/Material before integration; verify native format selection and heights first. |
| 1 dependency | LiquidType / LiquidMaterial DBC | No dedicated supplied-chain catalog found | Establish native disk/runtime mapping, joins and render-class dispatch; retain raw unknown fields rather than guess policy. |
| 2 | MCCV terrain vertex colours | No MCCV field in `McnkChunk` or terrain `ChunkMesh` | Authored colour is not propagated. Verify disk order, channel interpretation and native lighting equation before shader changes. |
| 2 | WMO v17 flags/liquids | Shared root/group parsing, MOCV, MLIQ geometry and legacy liquid-nibble conversion exist | Version number alone cannot distinguish 1.12/3.3.5 semantics. Audit root/group flags and DBC-based liquid routing; do not interpret a Wrath liquid ID as a legacy nibble. |
| 3 | WDT MPHD / big alpha | Raw flags retained by assets; Wrath ADT loader reads matching WDT and selects big alpha with `flags & (0x4 | 0x80)` | Already partly supported, not missing wholesale. Verify native flag consumers and installed outputs; avoid version-only inference. |
| 3 | MCAL compression / edge repair | Packed 4-bit, raw 8-bit and MCLY `0x200` RLE paths exist; Wrath honors MCNK `0x8000` edge-fix opt-out | Decoder silently pads malformed/truncated RLE output. Validate native boundaries and per-layer offsets before changing error policy. |
| 3 | MCSH | Fixed 512-byte bitmap parsed and expanded; short inputs are zero-padded | No alternate compression certified here. Distinguish MCAL compression from shadow storage; audit native and installed lengths. |
| Unranked pending evidence | Height texturing | No dedicated terrain height-blend path identified | Establish whether/where the effective data and build-12340 renderer use it. Do not infer support from a conventional MPHD bit label or another expansion. |

Rank reflects missing visible surfaces first, then authored colour/material
fidelity. It is not a measured screenshot comparison. AO liquid exclusions and
Plus lighting limitations remain as documented in `EXPERIENCE.md` and
`WRATH-TWO-STAGE.md`; this audit does not expand their coverage.

## Current loader path

- `benilla-adt/src/lib.rs`: `parse_adt` reads tile-level MH2O; MCNK fields
  currently retain heights, normals, layers, alpha, shadow and legacy MCLQ.
  Subchunk locations use header offsets, not a sequential scan (MCNR padding and
  noncontiguous chunks make a generic sequential walk unreliable).
- `benilla-formats/src/terrain.rs`: `adt_to_tile_mesh_with_options` retains
  MH2O on `TileMesh`, separately from legacy `ChunkMesh::liquids`.
- `benilla-assets/src/adt.rs`: `WrathAdtLoader` reads the matching WDT, derives
  alpha options and calls the shared mesh builder. `AdtTile` explicitly retains
  MH2O for future renderer integration; it is not rendered today.
- `benilla-assets/src/wdt.rs`: raw MPHD flags and MAIN tile existence are
  exposed. The parser's `WowVersion::Classic` name is not itself evidence that
  matching terrain uses the Vanilla MCAL path: the Wrath ADT loader supplies
  explicit options separately.
- `benilla-formats/src/models/wmo/group.rs`: MLIQ mesh conversion uses the
  existing group override/legacy nibble contract. Wrath flags and DBC IDs need
  separate evidence before reusing that conversion for new liquid types.

## Installed observations

Header dimensions: LiquidType **26 rows × 45 fields × 180 bytes**;
LiquidMaterial **3 rows × 3 fields × 12 bytes**. Dimensions do not establish
field meaning or runtime structure layout.

WDT raw MPHD first words: Northrend **0x0e**, Azeroth **0**, Kalimdor **0**,
Expansion01 **0**. Their MAIN tile-presence counts in this effective chain are
1131, 687, 988 and 800 respectively. These are observations, not named-bit
certification or whole-map rendering coverage.

A raw MH2O header/instance sample finds 256 layers with liquid ID 2 in each of
`Northrend_11_9` and `Northrend_49_37`. Dalaran-area candidate tiles
`Northrend_30_21` and `Northrend_31_21` contain respectively 24 and 40 layers
with liquid ID 5. The nearby `30_22` and `31_22` samples contain no layers.
These establish data presence, not semantic liquid class, successful mesh
conversion, correct world orientation or native appearance. Older-continent
samples also contain MH2O: this is not exclusively a Northrend concern.

No MCNK subchunk counts from a sequential scan are accepted as evidence.
A corrected sample checks reversed magic at both header-offset conventions
(`offset` and `offset - 8`). All seven sampled Northrend tiles have 256 MCCV,
MCAL and MCLY headers; the nine sampled older-continent tiles have no MCCV
headers at the tested offset and 256 MCAL/MCLY headers each. MCSH header counts
vary from zero to 256 across these samples (for example 234 and 242 on
`Northrend_30_21` and `31_21`). Header presence is not payload validation:
especially MCLQ headers do not establish nonempty legacy liquid blocks.
Installed tests must exercise production readers, not just duplicate this raw
inspection.

## Native evidence established so far

- `0x008b2710` returns `DBFilesClient\\LiquidType.dbc`; callers include
  `0x006474af` and the surrounding loader's diagnostics.
- `0x008b2a90` returns `DBFilesClient\\LiquidMaterial.dbc`; callers include
  `0x0064770f` and surrounding loader diagnostics.

Header loaders `0x006474a0` (LiquidType) and `0x00647700`
(LiquidMaterial) validate WDBC and respectively **45 columns / 180 bytes** and
**3 columns / 12 bytes**, agreeing with the installed headers. Data references
to the functions occur at `0x00a2825c` and `0x00a28284`; both loaders dispatch
raw loading through the object's vtable slot `+0x0c`. Constructor assignments
and raw-loader targets are not yet established by this extraction.

These bindings and header checks do not yet establish column semantics, MH2O
material selection, or height/depth interpretation. Raw decoder and consumer
tracing is the next evidence step, rather than implementing a public schema by
analogy.

## Small-unit implementation order and acceptance

1. Verify LiquidType/Material native mappings and the MH2O vertex-format
   selector/height contract. Add lossless supplied-chain readers plus synthetic
   and independently inspected installed-row regressions.
2. Convert supported MH2O layers into liquid geometry with exact rectangles,
   coverage, heights and preserved metadata; test multilayer, zero-offset,
   flat, sparse and malformed inputs. Missing/unsupported types must not
   silently become water. Preserve Vanilla MCLQ fixtures.
3. Coordinate render-path changes with Experience, then integrate supported
   surfaces, residency and collision/query behavior in a separate unit. Retained
   parser data alone is not completion. Native shader/material parity may require
   further units; document any bounded subset honestly.
4. Audit/implement MCCV and WMO flag/liquid differences, then harden alpha/shadow
   variants using native evidence and installed regressions. Height texturing
   remains evidence-gated, not an assumed feature.

Every reported unit runs its touched crates' full library suites and
`cargo test -p benilla-app --lib` under the shared heavy-build lock, with counts
recorded at handoff. Installed regressions use Wrath/enUS explicitly; ordinary
library tests clear client-profile/locale and WOW_DATA. No live certification is
claimed here. Northrend and Dalaran water/lava checks are manager-owned live
tests; this worker never launches the game client.

Shader changes require prior Experience coordination. Preserve stage binding
100's nine-vec4 ABI, second-image bindings 101/102, shared-light ABI and Classic
disabled-path bytes; do not change model/terrain lighting while adding parsing.

## Liquid metadata catalog evidence (build 12340)

`benilla_formats::wrath_liquid::load_liquid_catalogs(&mut Chain)` reads the
required tables through the caller's resolved Wrath chain. It preserves all
45/3 serialized words, source row order and duplicate rows. ID lookup follows
native last-duplicate-wins behavior; absent IDs/joins remain `None`. This is
separate from the existing Vanilla MCLQ geometry module `liquid`.

LiquidType raw loader `0x65d9b0` calls `0x8b2720` at 180-byte stride;
LiquidMaterial loader `0x65b780` calls `0x8b2aa0` at 12-byte stride. Both construct
a zero-filled ID index, overwriting earlier duplicate pointers. The latter
loader is shared; its liquid binding comes from vtable `0xa28280`, not stride
alone. Type decoder preserves word order and rebases string words 1 and 15–20;
Material decoder reads three words directly. Static primary-vtable references
bind Type object `0xad4064` to `0xa28258`, Material `0xad4088` to `0xa28280`.
The routines `0x665930`/`0x665980` and `0x9d8830`/`0x9d8870` are cleanup, not
constructors/initializers. Functions `0x65ffc0`/`0x65e370` copy by ordinal, not ID.

Terrain consumer `0x7c5690` and WMO consumer `0x793d20` join Type `+0x38`
(column 14) to Material ID, then use Material `+4` (column 1) as a render
selector. `0x79b870` accepts selectors 0/2 and uses Type `+0xa4` to index a
pointer table. Type `+8` is consumed as flags (for example `0x793d20` tests
`0x200`). These observations do **not** establish numeric water/lava labels,
MH2O vertex-format policy, texture animation equations or complete rendering
parity. Catalog accessors expose the verified join, raw flags and raw selector;
remaining words stay uninterpreted. Required read/string/decode errors propagate.

### MH2O accessor contract correction

Build-12340 MH2O vtable `0xa594b8` dispatches height/depth/UV accessors
`0x8a3130`, `0x8a3160`, `0x8a31b0`. With payload base B, vertex offset O,
vertex count N and index i: selectors 0/1 read height at B+O+4i;
selector 2 returns height zero. Selector 0 reads depth at B+O+4N+i;
selector 2 reads B+O+i, explicitly returning 255 when O=0. Selector 1
returns UV storage B+O+4(N+i); other selectors return static zero UVs.
These bodies establish four UV bytes per vertex, not component normalization.

The parser now accepts only selectors 0–2, rejecting the later combined layout
3 rather than inventing unsupported native behavior. Selector 2 with no payload
retains explicit depth 255 and zero heights. Selectors 0/1 with O=0 fail closed:
the native height accessor has no missing-offset guard, so a `min_height` plane
was an unsupported invention. Rejection is a bounded safety policy, not a claim
that the native client rejects malformed files identically. No rendering changes
are included in this correction.

A bounded effective-enUS sample of 16 ADTs across Northrend, Azeroth, Kalimdor
and Expansion01 contains 1,683 layers in nine tiles: selector 0/nonzero offset
147; selector 2/nonzero offset 93; selector 2/zero offset 1,443. Seven tiles have
no MH2O layers. No selector 1, selector 3, or selector 0/zero-offset example was
observed; this is not a complete installed-chain census. The installed regression
walks raw chunk headers independently and compares every sampled height/depth
against the production parser, including all 1,443 default-depth layers.

### Raw MH2O surface geometry (formats-only)

`wrath_liquid_surface::build_wrath_liquid_surface` is an explicit-placement seam,
not renderer integration. It keeps the complete layer and full parent attributes,
including raw liquid ID, vertex selector, UV words and depth bytes. The caller
supplies parent XY and signed row-X/column-Y steps; absolute source heights are
retained. No DBC classification, UV/depth normalization, Z offset, lattice snap,
layer merging, clipping, material selection or asset loading is performed.

Native `0x7cdf80` iterates rows outside columns with inclusive rectangle bounds;
its local spacing is the negative chunk-coordinate difference times 0.125,
and its Z is accessor height minus owner Z. Native `0x7ce1f0` flattens rectangle
cells row-major; `0x95da20` tests LSB-first bits. Assembly at `0x7ce5d0` establishes
triangle offsets `(0,S,S+1)` and `(0,S+1,1)` for row stride S. The new surface
uses these triangles for admitted cells without the native outcode clipping.
Malformed public layers/nonfinite positions fail closed as a safety policy.

UV consumer `0x7ce390` branches on the **material render selector**, not the MH2O
vertex selector: render selector 1 fetches unsigned u16 pairs through provider
vtable +0xC and multiplies by 3/256 (`0xa40414`); otherwise transformed XY is
multiplied by approximately 0.06 (`0xa40410`). These facts do not justify selecting
materials from vertex formats; this unit deliberately leaves UV words raw.
Native local-coordinate arithmetic/rounding is not reproduced by the explicit
placement API. Renderer integration must supply and verify placement separately.
