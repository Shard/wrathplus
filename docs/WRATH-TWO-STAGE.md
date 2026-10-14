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
  native-coverage gap, not a claim that all stage weights are identical.

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
