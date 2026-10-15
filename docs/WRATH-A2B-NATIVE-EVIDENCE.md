# a2b: native selector, opacity and environment-coordinate investigation

## Decision

Correction after review: Bevy visible points have negative view Z. Use the
eye-to-vertex incident vector `normalize(P_view)`, not its negation; the latter
produces a singularity for a head-on camera-facing surface. CPU vectors now use
visible points (z < 0), including finite head-on and grazing cases. Native shader
expressions require a view-convention transfer, not a literal sign copy.

Follow-up manager decision: implement the standard GL sphere-map equation below
for Wrath's environment-mapped second stage only. The manager confirms the shared
right-handed, camera-down-minus-Z view convention; live appearance remains a
manager-run check. UV0 / Vanilla behavior is unchanged. CPU known-reflection tests
and headless shader validation cover this bounded transfer. Exactly reflected -Z
remains the native sphere-map singularity; no invented denominator clamp is added.
The investigative limitations below remain historical evidence boundaries, not a
claim that the implementation has been visually certified.

Do **not** add independent second-stage transparency modulation. The initial
investigation deferred Classic texgen changes. Native evidence establishes selector
preparation and a batch opacity factor, and exposed the sphere-map difference now
corrected in the bounded stage-two path. It does not establish complete end-to-end
parity across render backends. The initial conclusion below describes that proof
boundary, not a parity certification of the remaining implementation.

Reference: read-only installed build-12340 `Wow.exe` and the effective Wrath archive
chain. Effective installed archives include expansion content; this is not a stock
archive certification. No executable was run and no proprietary payload is included
here. Addresses below are executable virtual addresses, not file offsets.

## Selectors: on-disk values are not final draw selectors

- `0x837a40` initializes shared model state by invoking `0x836980`, then
  `0x837680`, before calling batch selector `0x836c90`.
- `0x836980` constructs ordinary selectors from the combiner table at shared
  header `+0x134`. The first operation becomes opaque when material blend is zero.
  Coordinate lookups above two supply environment bits; a final coordinate of one
  sets selector bit `0x4000`.
- `0x836600` decodes packed operation nibbles (operation mask seven, environment
  bit eight) and chooses the `Diffuse_T1_T2`, `Diffuse_T1_Env`,
  `Diffuse_Env_T2` or `Diffuse_Env_Env` vertex family and a combiner pixel family.
- `0x837680` can fuse eligible adjacent batches into optimized high-bit selectors
  and marks donor batches `0x8000`. Its eligibility checks include equality of
  the first resolved weight lookup. Additional constraints are present; this is
  not a specification sufficient to reimplement the optimizer.
- `0x836c90` explicitly handles optimized variants one through three, with zero
  representing a removed batch. These values are not a modern generic shader enum.

**Correction to the gap census interpretation:** zero high-bit selectors in source
SKIN files does not imply zero optimized draws at runtime. The census measures
on-disk structure before the native initialization/optimization above.

## Opacity: proven batch factor, not independent stage modulation

The functions containing `0x821ed0`, `0x827b90` and `0x83478c` are respectively
`0x821a20`, `0x827a90` and `0x834660`. All three resolve a batch opacity factor:

1. Start with instance opacity at `+0x19c`.
2. If the batch color index (`batch +8`) is in range, multiply by the corresponding
   color opacity (instance color state `+0xa0`, stride `0x20`, value `+0x1c`).
3. If texture count (`batch +0x0e`) is nonzero, resolve the first weight combo
   (`batch +0x14`) through shared-header `+0x94` (u16 entries). Multiply by the
   resulting evaluated weight (instance `+0xa8`, stride twelve, value `+8`).

These paths skip removed batches and compare the factor against visibility or
submission thresholds. `0x821a20` stores the factor in the submitted record at
`+0x0c`; `0x827a90` counts eligible batches; `0x834660` applies additional material
eligibility checks before submission. The repeated blocks in `0x827a90` are
unrolled batch iteration, not iteration over texture stages.

No adjacent second-weight lookup occurs in these factor calculations. This proves
only the factor in these paths, **not** that every later draw or shader constant
path ignores second weights. Consequently the installed census of 1,946 batches
with distinct second weight indices / 1,749 with nonidentity tracks cannot justify
adding another multiply. Preserve the raw metadata; do not infer the rendering
equation from structural differences alone.

## Environment coordinates: backend-local equation found

`0x872d30`, reached from selector construction, requests vertex and pixel families
under `Shaders\\Vertex` and `Shaders\\Pixel`. The installed `arbvp1` files for
`Diffuse_T1_Env`, `Diffuse_Env_T2` and `Diffuse_Env_Env` each contain ninety program
variants. All ninety in each file contain the sphere-offset normalization sequence.
The inspected first `Diffuse_T1_Env` variant implements, in its shader-space basis:

- Transform position and normal with its model/view constants; normalize normal.
- Let `I = -normalize(P)` and `R = I - 2 * dot(I, N) * N`.
- Let `D = length(R + (0, 0, 1))`.
- Environment UV is `(0.5, 0.5) + 0.5 * R.xy / D`.

This is a mathematical conclusion from the local program, not a source dump.
The current `wow_model.wgsl` uses normalized reflected position with no sphere
normalization denominator and cites Vanilla addresses. The expressions differ in
the same basis, so current comments are not evidence of native Wrath parity.

However this investigation has **not** mapped native CPU-to-shader constant upload,
view handedness, texture orientation and variant selection onto Bevy, or decoded
and compared the installed Direct3D variants. At the singular denominator, native
and GPU reciprocal-square-root behavior also needs an explicit policy rather than
an invented clamp. A denominator/sign-only patch would therefore be premature.

## Blockers and next bounded implementation prerequisite

Before a rendering patch:

1. Follow the submitted batch record through draw dispatch and shader constants;
   establish whether any second weight is consumed after the proven batch factor.
2. Map native model/view constant construction to Bevy with non-axis-aligned test
   vectors and compare at least the relevant ARB and Direct3D variants.
3. Specify singular/degenerate behavior and add synthetic numeric regressions plus
   shader-composition tests; coordinate asset/shader ownership with Experience.
4. Implement only the proven scope and run full touched-crate libraries and the app
   gate under the shared lock. Manager-run visual comparison remains separate.

Until those prerequisites are met, the safe deliverable is this written boundary
of proof. Ordinary two-stage rendering remains the bounded candidate documented in
`WRATH-TWO-STAGE.md`, not full native a2b coverage.
