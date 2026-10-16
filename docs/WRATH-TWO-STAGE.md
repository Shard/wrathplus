# Wrath two-texture material candidate (a2)

This slice preserves ordinary two-stage v264/SKIN batches in one draw. It is not a
claim of complete native selector support or runtime visual parity.

## Data and scope

- Decode the second vertex UV pair in both MD20/MD21 adapters. Vanilla render
  descriptors retain their prior UV behavior.
- Resolve adjacent texture/coordinate/transform lookup entries for stage two;
  preserve texture path or Monster skin slot, wrap flags, combiner and UV loops.
- Use the Wrath combiner override array, not a modern shader-enum interpretation.
  Support ordinary first-stage opaque/modulate and second-stage operations 0–7.
  Reject missing required lookup entries, more than two stages, unsupported first
  operations, coordinate 2, and second-stage character/icon textures. Optimized
  high-bit shader selectors are not implemented by this slice.
- Installed checks cover weapon rack, Stormwind throne, zeppelin and Icebreaker
  paddlewheel, including stage paths, coordinate/wrap/combiner metadata and actual
  nonempty texture bytes. These fixtures do not prove every native combination.
- Independent second-stage transparency-weight tracks are not represented: the
  existing batch alpha lane remains authoritative. This is an explicit remaining
  native-coverage question, not proof that another per-stage multiply is required.
  The native batch factor uses the first weight; later draw consumption remains
  unproven (see `WRATH-A2B-NATIVE-EVIDENCE.md`).

## Material and readiness contract

`WowModelExt` adds `stage2`, `stage2_anim`, and `stage2_seed` after `anim_slots`:
ModelParams at material-group binding 100 is nine vec4s (144 bytes), mirrored in
forward and shadow WGSL. Binding 101 is the second image; 102 its sampler. Binding
90 and the shared light buffer layout/row offsets are unchanged. **No mesh-view
binding changed; binding 16 remains untouched for the AO work.**

Embedded second images are loaded through LoadContext and become model dependencies.
Monster2/3 variation images are resolved in display construction. Material dependency
visitation includes the extension image slot; the existing showcase readiness gate
therefore checks both effective image slots, including generated-image residency,
and reports failed server dependencies. This slice adds no universal world-image
readiness gate. A missing dynamic skin variation may still use the material fallback;
that is not certification of authored texture readiness.

## Forward/shadow coverage

Forward shading samples both stages and combines their RGB/texture-derived alpha
before source-cutout and fade coverage. Opaque first stages ignore first-texture
alpha but retain material/vertex opacity. Additive terms retain the existing lighting
separation and tint fold. Shadow sampling mirrors stage alpha operations, authored
cutout threshold (224/255), animated UV affine and vertex/material opacity. Environment
stage-two coordinates are view-dependent: these casters are conservatively discarded,
not evaluated using the light camera. Existing caster eligibility/exclusions remain.

Two-stage geometry does not enter first-texture-only static GX/merged paths. UV1 is
preserved in generic mesh construction. Material cache identity includes second-image,
combiner/coordinate state and geometry ownership to prevent independent animation
state aliasing. Stage-two UV table entries are independent from stage zero; entity
variants and placed terrain models register them. Off-world and particle paths carry
stage-two materials but do not add live second-stage animation registration; off-world
materials receive the baked seed. Full affine/animation parity for those lanes remains
follow-up work.

## AO and camera prepass handoff

Shadow PREPASS_PIPELINE dispatch remains separate and precedes forward specialization.
This slice does **not** enable camera NORMAL_PREPASS or provide complete normal/depth
coverage of merged, probe, clutter or environment-stage geometry. The shadow shader
must not be assumed to be a ready camera AO prepass. AO remains gated on review/merge
and requires its own camera-visible coverage design, with ambient-only attenuation and
no shared-light uniform expansion.

## Headless validation

- M2: 35 passed, 0 failed.
- Formats: 509 passed, 0 failed.
- Installed Wrath multitexture: 1 passed, 0 failed.
- Assets: 115 passed, 0 failed, 0 ignored.
- World (including shader composition): 578 passed, 0 failed, 6 ignored.
- App: 4,439 passed, 0 failed, 12 ignored.

No game client was launched. Visual parity and appearance require manager-run checks.

## Installed gap census (2026-10-14)

Read-only effective-install census: `WOW_CLIENT_PROFILE=wrath WOW_LOCALE=enUS
WOW_DATA=/Users/shannon/Games/Client/Data`, using native version-264 M2 records
and each model's first `00.skin` profile. This is **not** every LOD or a certified
stock-3.3.5 dataset: the effective archives include `EXPANSION03` paths.
22,112 native models were found; 30 lacked `00.skin`; remaining model/skin parser
failures: zero. Multi-stage geometry: 4,355 models / 7,795 batches.

| Structural gap | Models | Batches |
|---|---:|---:|
| Two textures, distinct non-sentinel stage-two weight lookup | 602 | 1,946 |
| High-bit optimized shader selector (all batches) | 0 | 0 |
| Environment-mapped stage two (conservatively excluded from shadows) | 3,676 | 4,958 |
| Animated stage-two UV, potentially affected in seed-only lanes | 185 | 567 |
| Animated stage-two UV assets referenced as particle geometry | 0 | 0 |

Second-stage weight tracks contain nonidentity keys in 1,749 batches. Distinct
lookup indices are structural evidence, **not proof of native per-stage weight
application**. Zero source-file optimized selectors is also not evidence of zero
runtime optimized draws: native shared initialization can fuse batches and rewrite
selectors. See `WRATH-A2B-NATIVE-EVIDENCE.md` for verified addresses and the remaining
weight/environment-coordinate proof boundary. Animation means a nonconstant translation, rotation or scaling
track with multiple keys. A model's runtime lane is not encoded in its M2:
185/567 are assets affected **if** instantiated through an offworld/particle lane,
not a claim that all currently appear there. Explicit particle geometry references
found in native emitter records intersected with those animated assets yield zero;
no example paths exist for that empty subset or the empty optimized-selector set.

Example paths (five distinct models per nonempty category):

- Independent weights: `Creature\BAND\BandTaurenMale.m2`,
  `Creature\BAND\BandTrollMale.m2`, `Creature\BAND\bandBloodElfMale.m2`,
  `Creature\BAND\bandUndeadMale.m2`, `Creature\FrostLord\FrostLord.m2`.
- Environment stage two: `Creature\BAND\BandTaurenMale.m2`,
  `Creature\BAND\BandTrollMale.m2`, `Creature\BAND\bandBloodElfMale.m2`,
  `Creature\BAND\bandOrcMale.m2`, `Creature\BAND\bandUndeadMale.m2`.
- Animated stage-two UV: `ITEM\OBJECTCOMPONENTS\Shoulder\LShoulder_Leather_RaidRogue_E_01.m2`,
  `ITEM\OBJECTCOMPONENTS\Shoulder\LShoulder_Robe_D_01.m2`,
  `ITEM\OBJECTCOMPONENTS\Shoulder\RShoulder_Leather_RaidRogue_E_01.m2`,
  `ITEM\OBJECTCOMPONENTS\Shoulder\RShoulder_Robe_D_01.m2`,
  `ITEM\OBJECTCOMPONENTS\WEAPON\Bow_2H_Crossbow_OutlandPVP_D_01.m2`.

Reproduce with the environment above and `cargo test -p benilla-formats --test
wrath_stage_gap_census -- --ignored --nocapture --test-threads 4`. Latest run:
1 passed, 0 failed. No proprietary payloads are emitted or committed.

## Second weight and off-world lanes (2026-10-16)

**Second texture weight: closed, not applied.** A read-only scan of every function in
`0x7e0000`–`0x890000` (the M2 module and the render backend, including the shader-family
selection at `0x872d30`) finds six readers of the evaluated texture-weight array (instance
`+0xa8`, stride 12): `0x81daf0`, `0x821a20`, `0x827a90`, `0x827e30`, `0x827f70` and `0x834660`.
Each indexes the weight lookup (shared `+0x94`) at the batch's first combo entry only
(`batch +0x14`); none reads the adjacent entry. The animate pass `0x82f0f0` samples the weights,
and the submitted record carries only the folded batch factor (`+0x0c`). So native 3.3.5 does
not multiply a second-stage weight, and the 1,946 batches with a distinct stage-two weight
index render with the first weight, as they already do here. Boundary: a reader that receives
the weight array through a parameter rather than the instance pointer would not match the scan.

**Stage-two UV animation in off-world lanes.** The world lanes (placed models, entity variants)
already registered stage two. Two lanes left it at its baked seed and now animate it:

- UI model tiles (`ui_models`): a two-stage batch's stage-two transform gets its own table rows
  (`stage2_anim.x`/`.z`) on the tile's material clone, sampled off the pane's play head like
  stage one.
- Glue scenes (`portrait/glue_booth`): stage two registers on the shared lane, as stage one does.

Spell-effect and GameObject per-sequence clones copy the shared material's stage-two row, so
they follow the shared stage-two clock rather than their host's sequence. No shipped particle
geometry references an animated stage-two asset (census above), so the particle lane needs none.
Runtime-fused optimized batches (`0x837680`) remain unmodelled.
