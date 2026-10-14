# Client experience

Experience is independent of `WOW_CLIENT_PROFILE` (installed data) and the session protocol.
Without configuration, **Classic** is selected: every Plus feature is disabled, even if a feature
entry says `true`. This is the evaluation default. `WOW_EXPERIENCE=classic|plus` overrides only
the profile; invalid values stop boot rather than silently selecting a different experience.

The existing `BENILLA_HOME/config.toml` supports:

```toml
[experience]
profile = "plus"
packs = ["base-art", "my-art"] # ascending priority, later packs win
allow_classic_overrides = false

[experience.features]
asset_overrides = true
shadows = true
fog = true
bloom = true
tone_mapping = true
colour_grading = true
```

All registered features default on in Plus; each can be disabled separately. These are seams:
a registered feature only has an effect once its consumer is implemented. Unknown feature names
are rejected rather than silently misspelling a toggle. CVar saves preserve the experience table.
Capture runs ignore persistent player configuration, as they do for CVars; an explicit
`WOW_EXPERIENCE=plus` is needed to select Plus for a capture.

`experience::Experience` is the app's Bevy resource and the only feature-query API. Runtime callers
use `set_profile` and `set_feature`; switching back to Classic masks existing feature overrides.
Runtime changes are session-only and do not overwrite the boot configuration. Asset packs are
mounted at boot, not hot-reloaded. Classic asset overrides require a separate explicit
`allow_classic_overrides = true`; that permission never enables graphics features under Classic.

## Asset packs

Put enabled packs in `<home>/mods/<pack>/`, with loose paths mirroring MPQ paths, for example
`mods/my-art/World/KhazModan/Ironforge/PassiveDoodads/Trees/IronforgeLeaves01.blp`.
The `packs` list is ascending priority: its last copy wins, then the original archive chain is the
fallback. Paths compare case-insensitively with either slash separator. Pack names must be single
folder names; traversal, absolute paths, symlinks and ambiguous case-folded names are rejected.
Pack files are local trusted content, not sandboxed plugins. Mount names are snapshotted at boot;
do not move or mutate the pack tree while the client runs. An unreadable winning file is an error,
not a silent fallback to different art. Byte reads log the winning loose file's provenance.

The policy is resolved once by app boot. Classic never even scans configured packs without the
separate `allow_classic_overrides` permission. Ordinary `Chain::open`/`open_profile` calls and bare
UI test harnesses remain archive-only. Packs are not hot-mounted by runtime profile setters;
restart after changing pack configuration.

### Reader audit

- `Chain::read` (and its `read_file` alias), `contains`, `list` and `list_filtered` share one policy;
  enumeration includes pack-only paths and the winning loose file's size.
- The boot `mpq://` reader receives that policy before Bevy's asset plugin builds. BLP, M2, WMO,
  ADT and WDT loaders use it, including ADT layer byte dependencies and sampler-marked doodad
  textures. Directory enumeration on the Bevy source remains unsupported as before; these
  loaders request explicit paths, while chain clients use `list_filtered`.
- World startup applies the same policy to `WorldAssets`; its DBC/model/texture/audio reads all
  use that chain. Vanilla reference UI has an explicit pre-VM boot initializer for its independent
  once-per-process chain; standalone test VMs do not discover player packs.
- Skin compositing no longer uses a direct archive handle. Ambience streams select a loose file
  before taking the original MPQ streaming branch; Classic retains sector-at-a-time MPQ audio.
- Public archive-only asset-source registration functions intentionally keep their original
  defaults; the production launcher calls the explicit override registration function instead.
- Remaining independent chain opens in asset/world/model/UI/animation test modules and the
  `cfg(test)` WMO/light/liquid audit modules deliberately validate original installed data. The
  direct archive handle in the sound mixer is likewise in its installed-data test module.
- Addon-owned loose-file fallback is a separate existing mechanism, after the chain lookup; it
  does not shadow client archive files or enable a mod pack.

Live replacement captures (texture/model and Classic before/after) are performed by the manager;
the worker does not launch the client. Synthetic tests cover policy resolution and the full Bevy
texture-loading pipeline without installed Blizzard assets.

## Plus lighting (first stage)

`fog`, `bloom`, `tone_mapping` and `colour_grading` are independent runtime switches. The app
publishes their resolved policy to the renderer each frame; the renderer used without the app
starts with every switch off. Classic masks all four, including saved explicit overrides.
`shadows` is reserved for the next stage: no real shadow rendering is claimed by this stage.

The Plus postprocess is a separate shader/pipeline family. It runs on the world image before UI
composition, both for a direct world camera and the backdrop behind the UI. Only `WorldCamera`
views opt in; portrait and glue/character panes retain the original FFX shader. Cache keys include
the three postprocess switches so runtime changes cannot reuse a previous look. Plus filters work
in linear colour, bloom adds a bounded bright-region blur, the filmic curve applies restrained
exposure, and grading cools shadows/desaturates the scene. The final result is encoded for the UI
backdrop or decoded for sRGB presentation as appropriate. Stock death-state colour remains last.
The original Classic FFX shader is unchanged.

Plus fog adds radial extinction with exponential height density to terrain, animated and static
models, liquids and world effects. It respects the existing fog toggle, interior fog span/colour
and material fog policy. Height density is relative to the camera (base eight world units below
it, falloff 0.025), not an assumed continent sea level. WDL's distant hull deliberately retains its
full-fog endpoint to avoid exposing the far seam. Existing light-buffer spare lanes carry the
policy: row 5.z enables it, row 19.z/w supply height base/falloff. Under Classic those lanes remain
zero and the original planar fog arithmetic is taken; new calculations occur only inside the
zero-disabled branch. No light layout or portrait/glue buffer changes are required.

Headless policy/packing and shader checks complement the full app suite; they cannot certify
Classic pixel equality or Plus appearance. The manager performs runtime switches and side-by-side
captures, including portrait/glue isolation, before accepting the rendering result.

### Plus shadow implementation / material extension seam

The world-only shadow sun exists only while the Shadows feature is effective and a world camera
exists. It adds no illumination (illuminance zero), uses layer 0 and two bounded cascades at 30/120
yards. Classic never spawns it. `ModernLighting` writes the receiver gate in shared light row 12.w;
row 12.xyz and all other existing field offsets remain unchanged. Booth buffers leave this gate zero.
Terrain and exterior model receivers attenuate sunlight, not ambient, point lights, emission or
interior probes. Classic retains its original lighting expressions in the zero-gate branch.

`benilla-assets/src/materials.rs` dispatches PREPASS_PIPELINE before forward specialization and
uses `shaders/wow_shadow.wgsl`, with the child helper `shadow_material.rs`. Forward attribute and
blend/depth logic must never run on this shadow path. The caster duplicates the complete
WowLight/ModelParams ABI: future two-stage material fields must be mirrored there. Palette skinning,
rig origins and animated UV affine match the visible model. Prepass inputs are position 0, UV 1/2,
color 7 and custom joints/weights 10/11 (not forward UV 2/3 and color 5).

Caster scope is canonical opaque/mask geometry plus source-cutout fade twins (marker bit 10,
224/255). Unknown opaque-source Blend twins are conservatively excluded: a future caller may
supply WOW_SHADOW_SOURCE_OPAQUE only with authored provenance included in its cache key.
Excluded keys: clutter, additive, no-depth-write/test, modulate/modulate2x, zfill, far-side, sky;
excluded mesh lanes: merged fade sphere and merged probe slot. Uniform exclusions are marker
mask 0x3b87 (bits 0/1 no depth, 2 glow, 7/8 multiply, 9 zfill, 11 far-side, 12 env-map, 13 sky),
clutter_fade.w and UI clip anim_slots.w. Authored translucent geometry does not cast. WMO MOCV
alpha is lighting data, not coverage. Shadow coverage never uses player-distance or MeshTag fade.
Merged geometry, clutter and unsupported translucent/effect casters remain outside this first
bounded shadow implementation. Runtime visual/bias quality and Classic pixel equality require
manager-run live captures; worker headless tests do not establish those results.
