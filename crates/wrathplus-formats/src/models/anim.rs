//! M2 skeleton and animation parsing: the rest skeleton and each sequence's per-bone keyframes.

use std::collections::HashMap;

use crate::BoneSpin;

/// A rest-skeleton bone, raw WoW model space. Vanilla M2 has no inverse bind matrices: the rest
/// pose is identity and the pivot is the bind position (`0x714260`).
#[derive(Debug, Clone, Copy)]
pub struct SkeletonBone {
    pub parent: i16,
    pub pivot: [f32; 3],
    /// `KeyBoneID` (`-1` none; 0/1 left/right arm, 2/3 left/right shoulder, …): how the per-arm
    /// masks find roots.
    pub key_bone: i16,
    /// The billboard arm (flags `0x08/0x10/0x20/0x40`); children inherit it.
    pub billboard: Option<crate::BillboardKind>,
    /// Bone flags `0x1/0x2/0x4`: how the parent matrix is rebuilt ([`crate::ParentArm`]).
    pub parent_arm: Option<crate::ParentArm>,
}

/// A model's bones in M2 file order, which a vertex's `joints` index.
#[derive(Debug, Clone, Default)]
pub struct Skeleton {
    pub bones: Vec<SkeletonBone>,
}

impl Skeleton {
    /// The nearest bone at or above `bone` with a billboard arm. Its rows become the camera basis
    /// and children multiply onto it (`0x7151f9`), so every descendant is camera-dependent, emitter
    /// positions too (`0x7190a9`); a nearer host discards what an outer one wrote.
    pub fn billboard_host(&self, bone: u16) -> Option<usize> {
        let mut i = usize::from(bone);
        // Bounded by the bone count: the guard against a malformed parent cycle.
        for _ in 0..=self.bones.len() {
            let b = self.bones.get(i)?;
            if b.billboard.is_some() {
                return Some(i);
            }
            i = usize::try_from(b.parent).ok()?;
        }
        None
    }
}

/// One attachment point (`0` shield, `1`/`2` right/left hand), position in raw WoW model space.
#[derive(Debug, Clone, Copy)]
pub struct M2Attachment {
    pub id: u16,
    pub bone: u16,
    pub position: [f32; 3],
}

/// An event record's position half, looked up by 4CC, first match (`0x7130e0`/`0x7131b0`): the
/// cast-release points `$CSL`/`$CSR`/`$CST` (`0x60c9b0`) and the ranged release `$BWR`.
#[derive(Debug, Clone, Copy)]
pub struct EventMarker {
    /// The identifier 4CC, stored forward (`*b"$CSL"`).
    pub ident: [u8; 4],
    pub bone: u16,
    pub position: [f32; 3],
}

/// A bow's `$WTT`/`$WTB` event records, `(bone, position)`, which the string drawer `0x611ff0`
/// spans top to bottom, each posed by its limb-tip bone (`0x7131b0`).
#[derive(Clone, Copy)]
pub struct StringAnchors {
    pub top: (u16, [f32; 3]),
    pub bottom: (u16, [f32; 3]),
}

/// One row of the M2's PlayableAnimationLookup, the resolution of a clip the model lacks.
#[derive(Debug, Clone, Copy)]
pub struct PlayableAnim {
    /// The `AnimationData.dbc` id this model actually plays for the row's requested id.
    pub resolved_id: u16,
    /// Direction/variant playback code; carried, not applied.
    pub dir_flags: u16,
}

/// The parentless bones whose stand sequence (id 0) spins them by rotation alone, with at least
/// two keys: a skybox's turning layers. `external` holds the sibling `.anim` payloads by sequence.
pub fn m2_bone_spins(bytes: &[u8], external: &[Option<&[u8]>]) -> HashMap<u16, BoneSpin> {
    let mut out = HashMap::new();
    let Ok(model) = wrathplus_m2::wrath::parse_wrath_m2_with_animations(bytes, external) else {
        return out;
    };
    let Some(stand) = model.animations().iter().position(|a| a.id == 0) else {
        return out;
    };
    let duration = model.animations()[stand].duration_ms as f32 / 1000.0;
    let window = |ranges: &[(u32, u32)], len: usize| -> std::ops::Range<usize> {
        match ranges.get(stand) {
            Some(&(lo, hi)) if lo != u32::MAX && (hi as usize) < len => {
                lo as usize..hi as usize + 1
            }
            _ => 0..0,
        }
    };
    for (idx, tracks) in model.bone_tracks().iter().enumerate() {
        let rotation =
            &tracks.rotation.keys[window(&tracks.rotation.ranges, tracks.rotation.keys.len())];
        let moved = |ranges: &[(u32, u32)], len: usize| !window(ranges, len).is_empty();
        // Rotation only and at least two keys: one key is a pose, not a spin.
        if rotation.len() < 2
            || moved(&tracks.translation.ranges, tracks.translation.keys.len())
            || moved(&tracks.scaling.ranges, tracks.scaling.keys.len())
        {
            continue;
        }
        // Parentless: nothing above composes another motion in.
        let Some(bone) = model.model().bones.get(idx).filter(|b| b.parent < 0) else {
            continue;
        };
        out.insert(
            idx as u16,
            BoneSpin {
                pivot: [bone.pivot.x, bone.pivot.y, bone.pivot.z],
                duration,
                interp: tracks.rotation.interp != 0,
                keys: rotation
                    .iter()
                    .map(|&(ms, q)| (ms as f32 / 1000.0, q))
                    .collect(),
            },
        );
    }
    out
}

/// One bone's keys for a sequence, raw WoW model space, seconds from its start, rotations
/// `[x, y, z, w]`; a bone absent from [`ModelAnimation::bones`] holds its bind pose.
#[derive(Debug, Clone)]
pub struct BoneKeys {
    pub bone: u16,
    pub translation: Vec<(f32, [f32; 3])>,
    pub rotation: Vec<(f32, [f32; 4])>,
    pub scale: Vec<(f32, [f32; 3])>,
}

/// A bone channel on a global sequence, sampled at the scene clock less the instance's attach
/// time, mod `period_ms` (`0x714352`). Keys are absolute ms, ascending.
#[derive(Debug, Clone)]
pub struct GlobalSeqChannel<T> {
    pub period_ms: u32,
    pub keys: Vec<(u32, T)>,
}

/// A bone's global-sequence channels, composed over the sequence pose at runtime.
#[derive(Debug, Clone)]
pub struct GlobalSeqBone {
    pub bone: u16,
    pub translation: Option<GlobalSeqChannel<[f32; 3]>>,
    pub rotation: Option<GlobalSeqChannel<[f32; 4]>>,
    pub scale: Option<GlobalSeqChannel<[f32; 3]>>,
}

/// One animation event key: a 4CC-tagged trigger on the sequence timeline.
#[derive(Debug, Clone, Copy)]
pub struct AnimEvent {
    /// Seconds from the sequence start, as the bone keys.
    pub time: f32,
    /// The identifier 4CC, stored forward (`*b"$FL0"`).
    pub ident: [u8; 4],
    /// The payload: a SoundEntries id for `$SND`/`$DSL`/`$DSO`, else 0.
    pub data: u32,
    /// The record's bone (`+8`), carried on the key because a 4CC repeats: every player model
    /// authors `$CSD` six times, so a lookup by 4CC finds the wrong point.
    pub bone: u16,
    /// The record's point (`+12`); the event kernel (`0x719370`) snapshots
    /// `placement · boneMatrix[bone] · position` into the callback record.
    pub position: [f32; 3],
}

/// One sequence's per-bone keyframes. Stand is `anim_id` 0 (`0x70ebd0`) but not always record 0,
/// so consumers select by `anim_id`.
#[derive(Debug, Clone)]
pub struct ModelAnimation {
    /// `AnimationData.dbc` id, the selection key (0 Stand, 4 Walk, 5 Run, 13 WalkBackwards, …).
    pub anim_id: u16,
    /// The file slot, which indexes every track's key ranges; zero-length sequences are dropped.
    pub seq_index: usize,
    /// The absolute band (`M2Sequence` `+0x04`/`+0x08`, ms) sequence tracks are cut to.
    pub start_ms: u32,
    pub end_ms: u32,
    pub duration: f32,
    pub looping: bool,
    /// `moveSpeed` (`+0x0c`, yd/s): a locomotion clip plays at `unitSpeed / (moveSpeed · scale)`
    /// (`0x5fe2f0`); `0.0` plays at 1×.
    pub move_speed: f32,
    /// `blendTime` (`+0x20`, seconds): the cross-fade in from the live pose (`0x7121a0`).
    pub blend_time: f32,
    /// The box centre (`+0x24`/`+0x30`): the pick broad phase tests this sphere for the current
    /// animation, placed and scaled, no pad (`0x7089c0`).
    pub bounds_center: [f32; 3],
    /// The sphere radius (`+0x3c`); `0.0` falls back to the header sphere.
    pub bounds_radius: f32,
    /// The box's min corner (`+0x24`). The blob shadow sizes from the Stand sequence's box, clamped
    /// to ±5 per axis (`0x711a20`, `0x6992c0`/`0x699250`).
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    /// `frequency` (`+0x14`): this variation's weight in the per-play roll (`0x7121a0`).
    pub frequency: u16,
    /// The replay range (`+0x18`/`+0x1c`): each arm rolls a play count
    /// `R = max(1, min + ⌊rand·(max−min)/32768⌋)` (`0x712692..0x7126cd`) into the play window
    /// (`0x7126d8`); a looping sequence ignores it (`0x7145f1`).
    pub min_replay: u32,
    pub max_replay: u32,
    pub bones: Vec<BoneKeys>,
    /// Event keys in this sequence's band, seconds from its start.
    pub events: Vec<AnimEvent>,
}

impl ModelAnimation {
    /// Does every bone hold the identity for the whole band? Then the doodad gate builds no rig,
    /// though the reference still arms the sequence and its other bakes still need it.
    pub fn is_rest_pose(&self) -> bool {
        const EPS: f32 = 1e-4;
        !self.bones.iter().any(|b| {
            b.translation.len() > 1
                || b.rotation.len() > 1
                || b.scale.len() > 1
                || b.translation
                    .iter()
                    .any(|(_, v)| v.iter().any(|c| c.abs() > EPS))
                || b.rotation
                    .iter()
                    .any(|(_, q)| (q[3].abs() - 1.0).abs() > EPS)
                || b.scale
                    .iter()
                    .any(|(_, s)| s.iter().any(|c| (c - 1.0).abs() > EPS))
        })
    }
}

#[cfg(test)]
mod doodad_sound_tests {}
