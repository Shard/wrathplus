//! The M2 asset, loaded by [`WrathM2ModelLoader`]: one [`ModelSubmesh`] per render batch, with its
//! decoded geometry and a texture dependency but no `Mesh` (see [`ModelSubmesh`]), plus the
//! model's rig, animations, effects and cameras. Materials and creature skins are applied at the
//! spawn site.

use std::sync::Arc;

use wrathplus_formats::{CollisionMesh, M2Bounds, M2Light, ParticleEmitterDef};
use bevy::asset::Asset;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy::reflect::TypePath;

#[path = "wrath_m2.rs"]
mod wrath_m2;
pub use wrath_m2::WrathM2ModelLoader;

use crate::coords::wow_to_bevy;
use crate::model::{ModelAnimations, ModelAttachment, ModelSkeleton, ModelSubmesh};

/// A loaded M2: its batches, bounds, collision hull, rig, animations, effects and cameras.
#[derive(Asset, TypePath, Clone)]
pub struct M2Model {
    pub submeshes: Vec<ModelSubmesh>,
    /// The authored bounds (model-local yards), the reference's distance-fade size.
    pub bounds: Option<M2Bounds>,
    /// The collision hull in WoW model space; `None` means the model does not collide.
    pub collision: Option<CollisionMesh>,
    /// Particle emitters, positions in WoW model space.
    pub emitters: Vec<ModelEmitter>,
    /// Ribbon emitters (trails), the reference's `CRibbonEmitter` (simulated at `0x7b7e60`).
    pub ribbons: Vec<ModelRibbon>,
    /// M2 light blocks in WoW model space; fire props and held torches author a point light.
    pub lights: Vec<ModelLight>,
    /// The rest skeleton in Bevy space; empty for a boneless model.
    pub skeleton: ModelSkeleton,
    /// The inverse bind poses, `translate(−pivot)` per bone, as a labeled sub-asset.
    pub inverse_bindposes: Handle<SkinnedMeshInverseBindposes>,
    /// The animations; `None` when no bone, global sequence, emitter or material loop animates.
    pub animations: Option<ModelAnimations>,
    /// The first sequence's length in seconds, the spell-fx completion clock: the reference ends
    /// an effect after one pass of its armed sequence, whatever its bones or loop flag
    /// (`0x7194f8`). The reference arms `animationLookup[0]`'s sequence, not the first; the two
    /// coincide on every effect model checked.
    pub first_seq_span: Option<f32>,
    /// Every parsed sequence in file order, listed even when [`Self::animations`] is `None`: the
    /// UI model pane's clock reads them.
    pub sequences: Vec<M2SequenceInfo>,
    /// Attachment points (hands, sheaths), each on its bone.
    pub attachments: Vec<ModelAttachment>,
    /// The event table's positional markers, queried by 4CC, first match (`0x7130e0`/`0x7131b0`).
    pub markers: Vec<crate::ModelMarker>,
    /// The unit-frame portrait camera, `cameraLookup[0]` (`0x713540`), rendered as authored:
    /// `lookAt` and the record's perspective, with no framing on top (`0x7ac640`).
    pub portrait_camera: Option<PortraitCamera>,
    /// The camera table's first record, the glue scenes' `Model:SetCamera(0)`; their
    /// `cameraLookup[0]` is the `0xffff` none sentinel.
    pub camera0: Option<PortraitCamera>,
    /// The camera table's second record, which a `<PlayerModel>` (paper doll, inspect, pet page)
    /// renders through: raw index 1, not through `cameraLookup` (`0x505b30`, chooser `0x505890`,
    /// frozen at `0x7acf10`). `None` with fewer than two cameras, where the reference builds a
    /// fixed camera.
    pub pane_camera: Option<PortraitCamera>,
    /// The whole camera table by raw index, for `SetCamera(n)`; an index past its end leaves the
    /// pane on the orthographic leg.
    pub cameras: Vec<PaneCamera>,
    /// A bow's `$WTT`/`$WTB` string ends `[top, bottom]` as (bone, Bevy-space point), for the
    /// string drawer `0x611ff0`.
    pub string_anchors: Option<[(u16, Vec3); 2]>,
    /// The fishing pole's `$CCH` rod tip, Bevy space, where the line draw `0x61f780` starts.
    pub cch_marker: Option<Vec3>,
    /// The header's `GlobalModelFlags` (`+0x10`); bits `&3` conform the model to the ground
    /// (`0x7106c0`): 1 pitch, 3 pitch and roll, else level.
    pub global_flags: u32,
}

/// One sequence as the file's table has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct M2SequenceInfo {
    /// The `AnimationData.dbc` id (`M2Sequence+0x00`).
    pub anim_id: u16,
    /// The sequence's file slot, which the per-sequence bakes key on.
    pub seq_index: usize,
    /// `end − start` on the file's timeline, ms.
    pub duration_ms: u32,
    /// The band's start on the file's timeline, ms (`M2Sequence+0x04`): add it to a cursor before
    /// sampling an absolute track, such as a camera's.
    pub start_ms: u32,
    /// Loops (flag bit 0 clear); a clamped sequence holds its last frame.
    pub looping: bool,
}

/// An authored camera in Bevy space, at model scale 1 as the reference bakes portraits.
#[derive(Clone, Copy)]
pub struct PortraitCamera {
    pub eye: Vec3,
    pub target: Vec3,
    /// Roll about the eye-to-target axis, radians.
    pub roll: f32,
    /// The record's field of view in radians, a diagonal angle: at aspect 1, `fovy = fov/√2`.
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

/// One camera-table record: [`Self::still`] in Bevy space, tracks in WoW space behind [`Self::at`].
#[derive(Clone)]
pub struct PaneCamera {
    /// The record's `type` (0 portrait, 1 characterinfo, -1 fly-bys and glue); never selected on.
    pub camera_type: i32,
    /// The rig at rest: sequence 0 at its start.
    pub still: PortraitCamera,
    /// The v264 record, kept only when a track moves (the fly-bys and the animated glue stages,
    /// such as `UI_Scourge`); its tracks are per sequence, like the bones'.
    pub(crate) tracks: Option<Arc<wrathplus_formats::WrathCamera>>,
}

impl PaneCamera {
    /// The rig `ms` into file sequence `sequence`, Bevy space: each track composed against its
    /// base ([`wrathplus_formats::sample_wrath_camera`]). A track keyed on a global sequence reads
    /// the same `ms`. The still rig answers for a static camera or a sequence the record lacks.
    pub fn at(&self, sequence: usize, ms: u32) -> PortraitCamera {
        let Some(camera) = self.tracks.as_deref() else {
            return self.still;
        };
        match wrathplus_formats::sample_wrath_camera(camera, sequence, ms, ms) {
            Ok(pose) => PortraitCamera {
                eye: wow_to_bevy(pose.position),
                target: wow_to_bevy(pose.target),
                roll: pose.roll,
                ..self.still
            },
            Err(_) => self.still,
        }
    }
}

/// The billboard bone an emitter's chain reaches; see [`ModelEmitter::billboard`].
#[derive(Clone, Copy, Debug)]
pub struct EmitterBillboard {
    pub kind: wrathplus_formats::BillboardKind,
    /// The bone's pivot, WoW model space.
    pub pivot: [f32; 3],
    /// The bone, so a rigged consumer seats the frame on its joint, the chain above folded in.
    pub bone: u16,
}

/// One particle emitter: its parsed def and its texture dependency.
#[derive(Clone)]
pub struct ModelEmitter {
    pub def: ParticleEmitterDef,
    pub texture: Option<Handle<Image>>,
    /// The host bone's pivot, WoW model space: `def.position − bone_pivot` is the emitter in the
    /// bone's frame. Zero for an out-of-range bone.
    pub bone_pivot: [f32; 3],
    /// The billboard bone this emitter's chain reaches. The reference folds the position through
    /// the replaced palette matrix (`0x718960` at `0x7190a9`-`0x71910c`, written by `0x715868`),
    /// so the origin is `pivot + camBasis·(def.position − pivot)`. A rigged world host has it in
    /// its palette already; unrigged item models and booth bakes, whose pose drops the camera
    /// arms, need it.
    pub billboard: Option<EmitterBillboard>,
    /// The recursion model, whose emitters become this one's children (`0x7b5dd0`).
    pub recursion: Option<Handle<M2Model>>,
    /// The geometry model, drawn per particle in place of a quad (`0x7b4840`).
    pub geometry: Option<Handle<M2Model>>,
    /// How far the owner model's transparent batches sort from its origin, model-local yards:
    /// the owner-last draw-order rung's size ([`wrathplus_formats::m2_owner_reach`]).
    pub owner_reach: f32,
    /// The owner model's bound sphere for the water-plane side, Bevy model space: the reference
    /// classifies once per model, above when `d ≥ −r`, `r = |world matrix row 0| × radius`
    /// (`0x7083df`-`0x7084dd`). `(ZERO, 0)` without header bounds.
    pub water_bound: (Vec3, f32),
    /// The owner's loader-idle file slot ([`crate::ModelAnimations::idle_seq`]), baked here for
    /// the lanes with no rig to ask; 0 when the model builds no animations.
    pub idle_seq: usize,
}

/// A model path inside an M2 record (`.mdx`/`.mdl`, backslashes) as its `mpq://….m2` URL.
pub(crate) fn m2_dep_url(raw: &str) -> String {
    let p = raw.to_ascii_lowercase().replace('\\', "/");
    let stem = p
        .strip_suffix(".mdx")
        .or_else(|| p.strip_suffix(".mdl"))
        .or_else(|| p.strip_suffix(".m2"))
        .unwrap_or(&p);
    format!("mpq://{stem}.m2")
}

/// One ribbon emitter, riding its bone like [`ModelEmitter::bone_pivot`] (`0x718960`).
#[derive(Clone)]
pub struct ModelRibbon {
    pub def: wrathplus_formats::RibbonEmitterDef,
    pub texture: Option<Handle<Image>>,
    pub bone_pivot: [f32; 3],
    /// As [`ModelEmitter::owner_reach`]: a trail takes its model's owner-last rung too.
    pub owner_reach: f32,
    /// As [`ModelEmitter::water_bound`]; the ribbon leg reads the model's verdict too (`0x7081f1`).
    pub water_bound: (Vec3, f32),
}

/// One M2 light and its bone's rest pivot: the reference moves the light with the live bone each
/// frame (`0x718960`), so a torch's glow follows the hand.
#[derive(Clone)]
pub struct ModelLight {
    pub def: M2Light,
    pub bone_pivot: [f32; 3],
}
