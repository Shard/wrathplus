//! Wrath of the Lich King (`MD20` version 264) and external `SKIN` readers.
//!
//! Version 264 is deliberately parsed here rather than admitted by the vanilla header walker:
//! its header drops `PlayableAnimationLookup`, stores a skin-profile count instead of embedded
//! `M2View` arrays, and animation tracks contain arrays of per-sequence arrays.

use std::ffi::CString;
use std::io::Cursor;

use benilla_bytes::ByteExt;

use crate::{
    M2ArrayString, M2Attachment, M2BlendMode, M2Bone, M2BoneFlags, M2EventMarker, M2Format,
    M2Header, M2Material, M2Model, M2QuatTrack, M2RawData, M2RenderFlags, M2ScalarTrack, M2Texture,
    M2TextureTransform, M2TextureType, M2Track, M2Vec3Track, M2Vertex, SkinBatch, SkinSection, C2,
    C3,
};

#[path = "wrath_effects.rs"]
mod effects;
pub use effects::{WrathLight, WrathRibbon};

const VERSION: u32 = 264;
const HEADER_LEN: usize = 0x130;

/// A version-264 sequence record (64 bytes).
#[derive(Clone, Debug)]
pub struct WrathAnimation {
    pub id: u16,
    pub variation_index: u16,
    pub duration_ms: u32,
    pub move_speed: f32,
    pub flags: u32,
    pub frequency: i16,
    pub replay_min: u32,
    pub replay_max: u32,
    pub blend_time_ms: u32,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub bounds_radius: f32,
    pub next_animation: i16,
    pub alias_next: u16,
}

/// One sibling `.anim` file required by a v264 model. `sequence_index` is the non-alias
/// sequence whose id and variation form `<stem><id:04>-<variation:02>.anim`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrathAnimationDependency {
    pub sequence_index: usize,
    pub animation_id: u16,
    pub variation_index: u16,
}

/// The three version-264 animation tracks belonging to one bone.
#[derive(Clone, Debug)]
pub struct WrathBoneTracks {
    pub translation: M2Vec3Track,
    pub rotation: M2QuatTrack,
    pub scaling: M2Vec3Track,
}

/// One key in a version-264 nested track. Cubic camera tracks carry authored tangents; step and
/// linear tracks do not. This avoids coercing Wrath data into vanilla's flat camera tracks.
#[derive(Clone, Debug, PartialEq)]
pub struct WrathTrackKey<V> {
    pub timestamp_ms: u32,
    pub value: V,
    pub in_tangent: Option<V>,
    pub out_tangent: Option<V>,
}

/// A version-264 track, retaining each outer (sequence-indexed) array independently.
#[derive(Clone, Debug, PartialEq)]
pub struct WrathTrack<V> {
    pub interpolation: u16,
    pub global_sequence: u16,
    pub sequences: Vec<Vec<WrathTrackKey<V>>>,
}

/// A version-264 camera with its native nested tracks.
#[derive(Clone, Debug, PartialEq)]
pub struct WrathCamera {
    pub camera_type: i32,
    pub fov: f32,
    pub far_clip: f32,
    pub near_clip: f32,
    pub positions: WrathTrack<[f32; 3]>,
    pub position_base: [f32; 3],
    pub target: WrathTrack<[f32; 3]>,
    pub target_base: [f32; 3],
    pub roll: WrathTrack<f32>,
}

/// A parsed version-264 model plus semantics that the vanilla [`M2Model`] cannot represent.
pub struct WrathM2 {
    format: M2Format,
    skin_count: u32,
    animations: Vec<WrathAnimation>,
    bone_tracks: Vec<WrathBoneTracks>,
    cameras: Vec<WrathCamera>,
    camera_lookup: Vec<u16>,
    texture_combiner_combos: Vec<u16>,
    lights: Vec<WrathLight>,
    ribbons: Vec<WrathRibbon>,
}

/// A decoded external Wrath skin profile. Its records reuse the established skin semantics; the
/// container is separate because [`crate::Skin`] specifically denotes an embedded `M2View`.
pub struct WrathSkin {
    indices: Vec<u16>,
    triangles: Vec<u16>,
    submeshes: Vec<SkinSection>,
    batches: Vec<SkinBatch>,
}

impl WrathSkin {
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }
    pub fn triangles(&self) -> &[u16] {
        &self.triangles
    }
    pub fn submeshes(&self) -> &[SkinSection] {
        &self.submeshes
    }
    pub fn batches(&self) -> &[SkinBatch] {
        &self.batches
    }
}

impl WrathM2 {
    pub fn model(&self) -> &M2Model {
        self.format.model()
    }

    /// Number of external profiles (`00.skin`, `01.skin`, …) named by the v264 header.
    pub fn skin_count(&self) -> u32 {
        self.skin_count
    }

    pub fn animations(&self) -> &[WrathAnimation] {
        &self.animations
    }

    /// Parallel to [`M2Model::bones`].
    pub fn bone_tracks(&self) -> &[WrathBoneTracks] {
        &self.bone_tracks
    }

    pub fn lights(&self) -> &[WrathLight] {
        &self.lights
    }

    pub fn ribbons(&self) -> &[WrathRibbon] {
        &self.ribbons
    }

    pub fn cameras(&self) -> &[WrathCamera] {
        &self.cameras
    }

    pub fn camera_lookup(&self) -> &[u16] {
        &self.camera_lookup
    }

    /// Optional second-stage material overrides (global flag 0x08), indexed by the
    /// skin batch shader selector. Kept outside the vanilla model: these are Wrath semantics.
    pub fn texture_combiner_combos(&self) -> &[u16] {
        &self.texture_combiner_combos
    }

    pub fn into_format(self) -> M2Format {
        self.format
    }
}

/// Errors from the strict v264 readers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrathError {
    NotMd20,
    NotSkin,
    UnsupportedVersion(u32),
    Truncated(&'static str),
    MissingExternalAnimation(usize),
    UnsupportedField(&'static str),
}

impl std::fmt::Display for WrathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotMd20 => write!(f, "not an MD20 model"),
            Self::NotSkin => write!(f, "not a SKIN file"),
            Self::UnsupportedVersion(v) => {
                write!(f, "unsupported Wrath M2 version {v} (expected 264)")
            }
            Self::Truncated(what) => write!(f, "truncated {what}"),
            Self::MissingExternalAnimation(sequence) => {
                write!(f, "missing external animation for sequence {sequence}")
            }
            Self::UnsupportedField(what) => write!(f, "unsupported Wrath field: {what}"),
        }
    }
}

impl std::error::Error for WrathError {}

pub type Result<T> = std::result::Result<T, WrathError>;

#[cfg(test)]
#[path = "wrath_track_tests.rs"]
mod track_tests;

type Arr = (usize, usize);

struct SequenceSources<'a> {
    animations: &'a [WrathAnimation],
    external: &'a [Option<&'a [u8]>],
    require_external: bool,
}

impl<'a> SequenceSources<'a> {
    fn canonical(&self, mut sequence: usize) -> Result<usize> {
        for _ in 0..=self.animations.len() {
            let animation = self
                .animations
                .get(sequence)
                .ok_or(WrathError::UnsupportedField(
                    "animation alias references a missing sequence",
                ))?;
            if animation.flags & 0x40 == 0 {
                return Ok(sequence);
            }
            sequence = usize::from(animation.alias_next);
        }
        Err(WrathError::UnsupportedField("animation alias cycle"))
    }

    fn payload<'b>(&self, m2: &'b [u8], sequence: usize) -> Result<(usize, &'b [u8])>
    where
        'a: 'b,
    {
        let source = self.canonical(sequence)?;
        if self.animations[source].flags & 0x130 != 0 {
            return Ok((source, m2));
        }
        match self.external.get(source).and_then(|bytes| *bytes) {
            Some(bytes) => Ok((source, bytes)),
            None if self.require_external => Err(WrathError::MissingExternalAnimation(source)),
            None => Ok((source, &[])),
        }
    }
}

fn array(b: &[u8], at: usize, what: &'static str) -> Result<Arr> {
    Ok((
        b.u32_at(at).ok_or(WrathError::Truncated(what))? as usize,
        b.u32_at(at + 4).ok_or(WrathError::Truncated(what))? as usize,
    ))
}

fn table<'a>(b: &'a [u8], arr: Arr, stride: usize, what: &'static str) -> Result<&'a [u8]> {
    let len = arr
        .0
        .checked_mul(stride)
        .ok_or(WrathError::Truncated(what))?;
    b.bytes_at(arr.1, len).ok_or(WrathError::Truncated(what))
}

fn record<'a>(
    table: &'a [u8],
    index: usize,
    stride: usize,
    what: &'static str,
) -> Result<&'a [u8]> {
    let at = index
        .checked_mul(stride)
        .ok_or(WrathError::Truncated(what))?;
    table
        .bytes_at(at, stride)
        .ok_or(WrathError::Truncated(what))
}

fn c3(b: &[u8], at: usize, what: &'static str) -> Result<C3> {
    Ok(C3 {
        x: b.f32_at(at).ok_or(WrathError::Truncated(what))?,
        y: b.f32_at(at + 4).ok_or(WrathError::Truncated(what))?,
        z: b.f32_at(at + 8).ok_or(WrathError::Truncated(what))?,
    })
}

fn vec3(b: &[u8], at: usize, what: &'static str) -> Result<[f32; 3]> {
    let v = c3(b, at, what)?;
    Ok([v.x, v.y, v.z])
}

fn u16_table(b: &[u8], arr: Arr, what: &'static str) -> Result<Vec<u16>> {
    let raw = table(b, arr, 2, what)?;
    Ok(raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| u16::from_le_bytes(*v))
        .collect())
}

/// Read a v264 nested animation track. The outer arrays are sequence-indexed; each inner array is
/// concatenated into the existing semantic track and represented by one half-open range.
fn nested_track<V>(
    b: &[u8],
    at: usize,
    value_stride: usize,
    what: &'static str,
    sources: &SequenceSources<'_>,
    read_value: impl Fn(&[u8], usize) -> Result<V>,
) -> Result<M2Track<V>> {
    let interp = b.u16_at(at).ok_or(WrathError::Truncated(what))?;
    let gseq = b.u16_at(at + 2).ok_or(WrathError::Truncated(what))?;
    if interp > 1 {
        return Err(WrathError::UnsupportedField(
            "cubic v264 tracks require tangent-bearing semantic types",
        ));
    }
    let times_outer = array(b, at + 4, what)?;
    let values_outer = array(b, at + 12, what)?;
    if times_outer.0 != values_outer.0 {
        return Err(WrathError::UnsupportedField(
            "track timestamp/value outer-array counts differ",
        ));
    }
    let times_desc = table(b, times_outer, 8, what)?;
    let values_desc = table(b, values_outer, 8, what)?;
    let mut keys = Vec::new();
    let mut ranges = Vec::with_capacity(times_outer.0);
    for sequence in 0..times_outer.0 {
        // Descriptors stay in the M2; only their pointed-to key payload changes source. Aliases use
        // their canonical sequence's descriptor slot and payload.
        let (descriptor_sequence, payload) = if gseq == u16::MAX {
            let (source, payload) = sources.payload(b, sequence)?;
            if payload.is_empty() {
                ranges.push((u32::MAX, 0));
                continue;
            }
            (source, payload)
        } else {
            (sequence, b)
        };
        let ta = array(times_desc, descriptor_sequence * 8, what)?;
        let va = array(values_desc, descriptor_sequence * 8, what)?;
        if ta.0 != va.0 {
            return Err(WrathError::UnsupportedField(
                "track timestamp/value key counts differ",
            ));
        }
        if ta.0 == 0 {
            ranges.push((u32::MAX, 0));
            continue;
        }
        let times = table(payload, ta, 4, what)?;
        let values = table(payload, va, value_stride, what)?;
        let start = u32::try_from(keys.len()).map_err(|_| WrathError::Truncated(what))?;
        for key in 0..ta.0 {
            let time = times.u32_at(key * 4).ok_or(WrathError::Truncated(what))?;
            keys.push((time, read_value(values, key * value_stride)?));
        }
        let end = u32::try_from(keys.len()).map_err(|_| WrathError::Truncated(what))?;
        // M2Track ranges are inclusive brackets, unlike the nested array's count.
        ranges.push((start, end.saturating_sub(1).max(start)));
    }
    Ok(M2Track {
        interp,
        gseq,
        ranges,
        keys,
    })
}

fn track_vec3(
    b: &[u8],
    at: usize,
    what: &'static str,
    sources: &SequenceSources<'_>,
) -> Result<M2Vec3Track> {
    nested_track(b, at, 12, what, sources, |v, o| vec3(v, o, what))
}

fn track_fix16(
    b: &[u8],
    at: usize,
    what: &'static str,
    sources: &SequenceSources<'_>,
) -> Result<M2ScalarTrack> {
    nested_track(b, at, 2, what, sources, |v, o| {
        Ok(f32::from(v.u16_at(o).ok_or(WrathError::Truncated(what))? as i16) / 32767.0)
    })
}

fn track_quat16(
    b: &[u8],
    at: usize,
    what: &'static str,
    sources: &SequenceSources<'_>,
) -> Result<M2QuatTrack> {
    nested_track(b, at, 8, what, sources, |v, o| {
        let component = |n: usize| -> Result<f32> {
            let packed = v.u16_at(o + n * 2).ok_or(WrathError::Truncated(what))? as i16;
            // M2CompQuat's asymmetric signed-short mapping.
            let expanded = if packed < 0 {
                i32::from(packed) + 32768
            } else {
                i32::from(packed) - 32767
            };
            Ok(expanded as f32 / 32767.0)
        };
        Ok([component(0)?, component(1)?, component(2)?, component(3)?])
    })
}

fn nested_wrath_track<V: Clone>(
    b: &[u8],
    at: usize,
    value_stride: usize,
    what: &'static str,
    sources: &SequenceSources<'_>,
    read_value: impl Fn(&[u8], usize) -> Result<V>,
) -> Result<WrathTrack<V>> {
    let interpolation = b.u16_at(at).ok_or(WrathError::Truncated(what))?;
    let global_sequence = b.u16_at(at + 2).ok_or(WrathError::Truncated(what))?;
    if interpolation > 3 {
        return Err(WrathError::UnsupportedField(
            "v264 track has an unknown interpolation mode",
        ));
    }
    let times_outer = array(b, at + 4, what)?;
    let values_outer = array(b, at + 12, what)?;
    if times_outer.0 != values_outer.0 {
        return Err(WrathError::UnsupportedField(
            "track timestamp/value outer-array counts differ",
        ));
    }
    let times_desc = table(b, times_outer, 8, what)?;
    let values_desc = table(b, values_outer, 8, what)?;
    let stored_stride = if interpolation >= 2 {
        value_stride
            .checked_mul(3)
            .ok_or(WrathError::Truncated(what))?
    } else {
        value_stride
    };
    let mut sequences = Vec::with_capacity(times_outer.0);
    for sequence in 0..times_outer.0 {
        let (descriptor_sequence, payload) = if global_sequence == u16::MAX {
            let (source, payload) = sources.payload(b, sequence)?;
            if payload.is_empty() {
                sequences.push(Vec::new());
                continue;
            }
            (source, payload)
        } else {
            (sequence, b)
        };
        let ta = array(times_desc, descriptor_sequence * 8, what)?;
        let va = array(values_desc, descriptor_sequence * 8, what)?;
        if ta.0 != va.0 {
            return Err(WrathError::UnsupportedField(
                "track timestamp/value key counts differ",
            ));
        }
        let times = table(payload, ta, 4, what)?;
        let values = table(payload, va, stored_stride, what)?;
        let mut keys = Vec::with_capacity(ta.0);
        for key in 0..ta.0 {
            let value_at = key * stored_stride;
            let (in_tangent, out_tangent) = if interpolation >= 2 {
                (
                    Some(read_value(values, value_at + value_stride)?),
                    Some(read_value(values, value_at + value_stride * 2)?),
                )
            } else {
                (None, None)
            };
            keys.push(WrathTrackKey {
                timestamp_ms: times.u32_at(key * 4).ok_or(WrathError::Truncated(what))?,
                value: read_value(values, value_at)?,
                in_tangent,
                out_tangent,
            });
        }
        sequences.push(keys);
    }
    Ok(WrathTrack {
        interpolation,
        global_sequence,
        sequences,
    })
}

fn camera_vec3_track(
    b: &[u8],
    at: usize,
    what: &'static str,
    sources: &SequenceSources<'_>,
) -> Result<WrathTrack<[f32; 3]>> {
    nested_wrath_track(b, at, 12, what, sources, |v, o| vec3(v, o, what))
}

fn camera_scalar_track(
    b: &[u8],
    at: usize,
    what: &'static str,
    sources: &SequenceSources<'_>,
) -> Result<WrathTrack<f32>> {
    nested_wrath_track(b, at, 4, what, sources, |v, o| {
        v.f32_at(o).ok_or(WrathError::Truncated(what))
    })
}

fn cameras(b: &[u8], arr: Arr, sources: &SequenceSources<'_>) -> Result<Vec<WrathCamera>> {
    const STRIDE: usize = 100;
    let raw = table(b, arr, STRIDE, "Wrath camera table")?;
    (0..arr.0)
        .map(|i| {
            let camera = record(raw, i, STRIDE, "Wrath camera record")?;
            let at = arr.1 + i * STRIDE;
            Ok(WrathCamera {
                camera_type: camera
                    .u32_at(0)
                    .ok_or(WrathError::Truncated("Wrath camera type"))?
                    as i32,
                fov: camera
                    .f32_at(4)
                    .ok_or(WrathError::Truncated("Wrath camera fov"))?,
                far_clip: camera
                    .f32_at(8)
                    .ok_or(WrathError::Truncated("Wrath camera far clip"))?,
                near_clip: camera
                    .f32_at(12)
                    .ok_or(WrathError::Truncated("Wrath camera near clip"))?,
                positions: camera_vec3_track(b, at + 16, "camera position track", sources)?,
                position_base: vec3(camera, 36, "camera position base")?,
                target: camera_vec3_track(b, at + 48, "camera target track", sources)?,
                target_base: vec3(camera, 68, "camera target base")?,
                roll: camera_scalar_track(b, at + 80, "camera roll track", sources)?,
            })
        })
        .collect()
}

fn animations(b: &[u8], arr: Arr) -> Result<Vec<WrathAnimation>> {
    let raw = table(b, arr, 64, "Wrath animation table")?;
    (0..arr.0)
        .map(|i| {
            let a = record(raw, i, 64, "Wrath animation record")?;
            Ok(WrathAnimation {
                id: a
                    .u16_at(0)
                    .ok_or(WrathError::Truncated("Wrath animation"))?,
                variation_index: a
                    .u16_at(2)
                    .ok_or(WrathError::Truncated("Wrath animation"))?,
                duration_ms: a
                    .u32_at(4)
                    .ok_or(WrathError::Truncated("Wrath animation"))?,
                move_speed: a
                    .f32_at(8)
                    .ok_or(WrathError::Truncated("Wrath animation"))?,
                flags: a
                    .u32_at(12)
                    .ok_or(WrathError::Truncated("Wrath animation"))?,
                frequency: a
                    .u16_at(16)
                    .ok_or(WrathError::Truncated("Wrath animation"))?
                    as i16,
                replay_min: a
                    .u32_at(20)
                    .ok_or(WrathError::Truncated("Wrath animation"))?,
                replay_max: a
                    .u32_at(24)
                    .ok_or(WrathError::Truncated("Wrath animation"))?,
                blend_time_ms: a
                    .u32_at(28)
                    .ok_or(WrathError::Truncated("Wrath animation"))?,
                bounds_min: vec3(a, 32, "Wrath animation bounds")?,
                bounds_max: vec3(a, 44, "Wrath animation bounds")?,
                bounds_radius: a
                    .f32_at(56)
                    .ok_or(WrathError::Truncated("Wrath animation bounds"))?,
                next_animation: a
                    .u16_at(60)
                    .ok_or(WrathError::Truncated("Wrath animation"))?
                    as i16,
                alias_next: a
                    .u16_at(62)
                    .ok_or(WrathError::Truncated("Wrath animation"))?,
            })
        })
        .collect()
}

/// Discover the canonical sibling `.anim` files required by an `MD20` version-264 model.
/// Alias sequences (`flags & 0x40`) share the dependency selected by `alias_next`.
pub fn wrath_animation_dependencies(bytes: &[u8]) -> Result<Vec<WrathAnimationDependency>> {
    validate_header(bytes)?;
    let parsed = animations(bytes, array(bytes, 0x1c, "animations")?)?;
    let sources = SequenceSources {
        animations: &parsed,
        external: &[],
        require_external: false,
    };
    let mut dependencies = Vec::new();
    for sequence_index in 0..parsed.len() {
        let source = sources.canonical(sequence_index)?;
        let sequence = &parsed[source];
        if source == sequence_index && sequence.flags & 0x130 == 0 {
            dependencies.push(WrathAnimationDependency {
                sequence_index: source,
                animation_id: sequence.id,
                variation_index: sequence.variation_index,
            });
        }
    }
    Ok(dependencies)
}

fn validate_header(b: &[u8]) -> Result<()> {
    if b.len() < 8 || &b[..4] != b"MD20" {
        return Err(WrathError::NotMd20);
    }
    let version = b
        .u32_at(4)
        .ok_or(WrathError::Truncated("Wrath M2 version"))?;
    if version != VERSION {
        return Err(WrathError::UnsupportedVersion(version));
    }
    if b.len() < HEADER_LEN {
        return Err(WrathError::Truncated("Wrath M2 header"));
    }
    Ok(())
}

/// Parse an `MD20` version-264 model while retaining the compatibility behavior of leaving
/// external sequence slots empty. New loaders should discover dependencies and call
/// [`parse_wrath_m2_with_animations`] instead.
pub fn parse_wrath_m2(cursor: &mut Cursor<&[u8]>) -> Result<WrathM2> {
    parse_wrath_m2_impl(cursor.get_ref(), &[], false)
}

/// Parse a v264 model with sibling `.anim` payloads indexed by canonical sequence index.
/// Every required dependency must be present; each timestamp and value offset is bounded against
/// its selected M2 or `.anim` source independently.
pub fn parse_wrath_m2_with_animations(
    m2_bytes: &[u8],
    external_animations: &[Option<&[u8]>],
) -> Result<WrathM2> {
    parse_wrath_m2_impl(m2_bytes, external_animations, true)
}

fn parse_wrath_m2_impl<'a>(
    b: &'a [u8],
    external_animations: &'a [Option<&'a [u8]>],
    require_external: bool,
) -> Result<WrathM2> {
    validate_header(b)?;

    // Exact v264 offsets. In particular, +0x44 is a scalar skin count, not an M2Array/M2View.
    let global_sequences_arr = array(b, 0x14, "global sequences")?;
    let animations_arr = array(b, 0x1c, "animations")?;
    let animation_lookup_arr = array(b, 0x24, "animation lookup")?;
    let bones_arr = array(b, 0x2c, "bones")?;
    let _key_bone_lookup = u16_table(b, array(b, 0x34, "key bone lookup")?, "key bone lookup")?;
    let vertices_arr = array(b, 0x3c, "vertices")?;
    let skin_count = b
        .u32_at(0x44)
        .ok_or(WrathError::Truncated("skin profile count"))?;
    let colors_arr = array(b, 0x48, "colors")?;
    let textures_arr = array(b, 0x50, "textures")?;
    let weights_arr = array(b, 0x58, "texture weights")?;
    let transforms_arr = array(b, 0x60, "texture transforms")?;
    let _texture_replacements = u16_table(
        b,
        array(b, 0x68, "texture replacements")?,
        "texture replacements",
    )?;
    let materials_arr = array(b, 0x70, "materials")?;
    let _bone_lookup = u16_table(b, array(b, 0x78, "bone lookup")?, "bone lookup")?;
    let texture_lookup_arr = array(b, 0x80, "texture lookup")?;
    let texture_unit_lookup_arr = array(b, 0x88, "texture unit lookup")?;
    let transparency_lookup_arr = array(b, 0x90, "transparency lookup")?;
    let transform_lookup_arr = array(b, 0x98, "texture transform lookup")?;
    let bounding_triangles_arr = array(b, 0xd8, "bounding triangles")?;
    let bounding_vertices_arr = array(b, 0xe0, "bounding vertices")?;
    let attachments_arr = array(b, 0xf0, "attachments")?;
    let attachment_lookup_arr = array(b, 0xf8, "attachment lookup")?;
    let events_arr = array(b, 0x100, "events")?;
    let lights_arr = array(b, 0x108, "lights")?;
    let cameras_arr = array(b, 0x110, "cameras")?;
    let camera_lookup_arr = array(b, 0x118, "camera lookup")?;
    let ribbons_arr = array(b, 0x120, "ribbon emitters")?;
    let _particle_emitters = array(b, 0x128, "particle emitters")?;

    // wowdev M2 header: flag_use_texture_combiner_combos adds an M2Array<uint16> at 0x130.
    // Do not touch those bytes without the flag: unextended files can start payload there.
    let texture_combiner_combos = if b.u32_at(0x10).unwrap_or(0) & 0x08 != 0 {
        u16_table(
            b,
            array(b, HEADER_LEN, "texture combiner combos descriptor")?,
            "texture combiner combos",
        )?
    } else {
        Vec::new()
    };

    let vertices_raw = table(b, vertices_arr, 48, "vertex table")?;
    let mut vertices = Vec::with_capacity(vertices_arr.0);
    for i in 0..vertices_arr.0 {
        let v = record(vertices_raw, i, 48, "vertex record")?;
        vertices.push(M2Vertex {
            position: c3(v, 0, "vertex position")?,
            bone_weights: [v[12], v[13], v[14], v[15]],
            bone_indices: [v[16], v[17], v[18], v[19]],
            normal: c3(v, 20, "vertex normal")?,
            tex_coords: C2 {
                x: v.f32_at(32).ok_or(WrathError::Truncated("vertex UV"))?,
                y: v.f32_at(36).ok_or(WrathError::Truncated("vertex UV"))?,
            },
            tex_coords2: C2 {
                x: v.f32_at(0x28).ok_or(WrathError::Truncated("vertex UV1"))?,
                y: v.f32_at(0x2c).ok_or(WrathError::Truncated("vertex UV1"))?,
            },
        });
    }

    let textures_raw = table(b, textures_arr, 16, "texture table")?;
    let mut textures = Vec::with_capacity(textures_arr.0);
    for i in 0..textures_arr.0 {
        let t = record(textures_raw, i, 16, "texture record")?;
        let filename = array(t, 8, "texture filename")?;
        let raw = table(b, filename, 1, "texture filename")?;
        let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
        let string = CString::new(&raw[..end])
            .map_err(|_| WrathError::UnsupportedField("texture filename contains interior NUL"))?;
        let texture_flags = t.u32_at(4).ok_or(WrathError::Truncated("texture flags"))?;
        textures.push(M2Texture {
            texture_type: M2TextureType::from_u32(
                t.u32_at(0).ok_or(WrathError::Truncated("texture type"))?,
            ),
            wrap_x: texture_flags & 1 != 0,
            wrap_y: texture_flags & 2 != 0,
            filename: M2ArrayString { string },
        });
    }

    let materials_raw = table(b, materials_arr, 4, "material table")?;
    let mut materials = Vec::with_capacity(materials_arr.0);
    for i in 0..materials_arr.0 {
        let m = record(materials_raw, i, 4, "material record")?;
        materials.push(M2Material {
            flags: M2RenderFlags(m.u16_at(0).ok_or(WrathError::Truncated("material flags"))?),
            blend_mode: M2BlendMode(
                m.u16_at(2)
                    .ok_or(WrathError::Truncated("material blend mode"))?,
            ),
        });
    }

    let parsed_animations = animations(b, animations_arr)?;
    let sources = SequenceSources {
        animations: &parsed_animations,
        external: external_animations,
        require_external,
    };
    let lights = effects::lights(b, lights_arr, &sources)?;
    let ribbons = effects::ribbons(b, ribbons_arr, &sources)?;
    let bones_raw = table(b, bones_arr, 88, "bone table")?;
    let mut bones = Vec::with_capacity(bones_arr.0);
    let mut bone_tracks = Vec::with_capacity(bones_arr.0);
    for i in 0..bones_arr.0 {
        let bone_at = bones_arr.1 + i * 88;
        let bone = record(bones_raw, i, 88, "bone record")?;
        let mut pivot = c3(bone, 76, "bone pivot")?;
        if pivot.x.is_nan() {
            pivot.x = 0.0;
        }
        if pivot.y.is_nan() {
            pivot.y = 0.0;
        }
        if pivot.z.is_nan() {
            pivot.z = 0.0;
        }
        bones.push(M2Bone {
            key_bone: bone.u32_at(0).ok_or(WrathError::Truncated("key bone"))? as i32 as i16,
            flags: M2BoneFlags(bone.u32_at(4).ok_or(WrathError::Truncated("bone flags"))?),
            parent: bone.u16_at(8).ok_or(WrathError::Truncated("bone parent"))? as i16,
            pivot,
        });
        bone_tracks.push(WrathBoneTracks {
            translation: track_vec3(b, bone_at + 16, "bone translation track", &sources)?,
            rotation: track_quat16(b, bone_at + 36, "bone rotation track", &sources)?,
            scaling: track_vec3(b, bone_at + 56, "bone scaling track", &sources)?,
        });
    }

    let colors_raw = table(b, colors_arr, 40, "color table")?;
    let mut color_rgb_tracks = Vec::with_capacity(colors_arr.0);
    let mut color_alpha_tracks = Vec::with_capacity(colors_arr.0);
    for i in 0..colors_arr.0 {
        let at = colors_arr.1 + i * 40;
        record(colors_raw, i, 40, "color record")?;
        color_rgb_tracks.push(track_vec3(b, at, "color RGB track", &sources)?);
        color_alpha_tracks.push(track_fix16(b, at + 20, "color alpha track", &sources)?);
    }

    let weights_raw = table(b, weights_arr, 20, "texture weight table")?;
    let mut transparency_tracks = Vec::with_capacity(weights_arr.0);
    for i in 0..weights_arr.0 {
        record(weights_raw, i, 20, "texture weight record")?;
        transparency_tracks.push(track_fix16(
            b,
            weights_arr.1 + i * 20,
            "texture weight track",
            &sources,
        )?);
    }

    let transforms_raw = table(b, transforms_arr, 60, "texture transform table")?;
    let mut texture_transforms = Vec::with_capacity(transforms_arr.0);
    for i in 0..transforms_arr.0 {
        let at = transforms_arr.1 + i * 60;
        record(transforms_raw, i, 60, "texture transform record")?;
        texture_transforms.push(M2TextureTransform {
            translation: track_vec3(b, at, "texture translation track", &sources)?,
            rotation: track_quat16(b, at + 20, "texture rotation track", &sources)?,
            scaling: track_vec3(b, at + 40, "texture scaling track", &sources)?,
        });
    }

    let attachments_raw = table(b, attachments_arr, 40, "attachment table")?;
    let mut attachments = Vec::with_capacity(attachments_arr.0);
    let mut emitted_at = vec![u16::MAX; attachments_arr.0];
    for (i, emitted) in emitted_at.iter_mut().enumerate() {
        let a = record(attachments_raw, i, 40, "attachment record")?;
        let id = a.u32_at(0).ok_or(WrathError::Truncated("attachment id"))?;
        let bone = u32::from(
            a.u16_at(4)
                .ok_or(WrathError::Truncated("attachment bone"))?,
        );
        let (Ok(id), Ok(bone)) = (u16::try_from(id), u16::try_from(bone)) else {
            continue;
        };
        if bone as usize >= bones.len() {
            continue;
        }
        *emitted = u16::try_from(attachments.len()).unwrap_or(u16::MAX);
        attachments.push(M2Attachment {
            id,
            bone,
            position: vec3(a, 8, "attachment position")?,
        });
    }
    let raw_attachment_lookup = u16_table(b, attachment_lookup_arr, "attachment lookup")?;
    let attachment_lookup = raw_attachment_lookup
        .iter()
        .map(|&i| emitted_at.get(i as usize).copied().unwrap_or(u16::MAX))
        .collect::<Vec<_>>();
    let pivot_attach_z = attachment_lookup
        .get(17)
        .filter(|&&i| i != u16::MAX)
        .and_then(|&i| attachments.get(i as usize))
        .map(|a| a.position[2]);

    // A v264 event is 36 bytes: its positional prefix is unchanged; only its nested enabled track
    // differs. Preserve the represented positional semantics and validate every complete record.
    let events_raw = table(b, events_arr, 36, "event table")?;
    let mut event_markers = Vec::with_capacity(events_arr.0);
    for i in 0..events_arr.0 {
        let e = record(events_raw, i, 36, "event record")?;
        let bone = e.u32_at(8).ok_or(WrathError::Truncated("event bone"))?;
        let Ok(bone) = u16::try_from(bone) else {
            continue;
        };
        if bone as usize >= bones.len() {
            continue;
        }
        event_markers.push(M2EventMarker {
            ident: [e[0], e[1], e[2], e[3]],
            bone,
            position: vec3(e, 12, "event position")?,
        });
    }

    let global_sequences_raw = table(b, global_sequences_arr, 4, "global sequences")?;
    let global_sequences = global_sequences_raw
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| u32::from_le_bytes(*v))
        .collect();
    let animation_lookup = u16_table(b, animation_lookup_arr, "animation lookup")?;
    let texture_lookup_table = u16_table(b, texture_lookup_arr, "texture lookup")?;
    let texture_unit_lookup = u16_table(b, texture_unit_lookup_arr, "texture unit lookup")?;
    let transparency_lookup = u16_table(b, transparency_lookup_arr, "transparency lookup")?;
    let texture_transform_lookup = u16_table(b, transform_lookup_arr, "transform lookup")?;
    let parsed_cameras = cameras(b, cameras_arr, &sources)?;
    let camera_lookup = u16_table(b, camera_lookup_arr, "camera lookup")?;
    if camera_lookup
        .iter()
        .any(|&index| index != u16::MAX && usize::from(index) >= parsed_cameras.len())
    {
        return Err(WrathError::UnsupportedField(
            "camera lookup references a missing camera",
        ));
    }
    let bounding_triangles = table(b, bounding_triangles_arr, 2, "bounding triangles")?.to_vec();
    let bounding_vertices = table(b, bounding_vertices_arr, 12, "bounding vertices")?.to_vec();

    let format = M2Format {
        model: M2Model {
            vertices,
            textures,
            materials,
            color_alpha_tracks,
            color_rgb_tracks,
            transparency_tracks,
            transparency_lookup,
            texture_unit_lookup,
            texture_transforms,
            texture_transform_lookup,
            global_sequences,
            bones,
            cameras: Vec::new(),
            camera_lookup: Vec::new(),
            raw_data: M2RawData {
                texture_lookup_table,
                bounding_triangles,
                bounding_vertices,
            },
            header: M2Header {
                bounding_box_min: vec3(b, 0xa0, "bounding box")?,
                bounding_box_max: vec3(b, 0xac, "bounding box")?,
                bounding_sphere_radius: b
                    .f32_at(0xb8)
                    .ok_or(WrathError::Truncated("bounding sphere"))?,
                collision_box_min: vec3(b, 0xbc, "collision box")?,
                collision_box_max: vec3(b, 0xc8, "collision box")?,
                collision_sphere_radius: b
                    .f32_at(0xd4)
                    .ok_or(WrathError::Truncated("collision sphere"))?,
            },
            pivot_attach_z,
            attachments,
            attach_lookup: attachment_lookup,
            event_markers,
            animation_lookup,
            playable_animation_lookup: Vec::new(),
            // Kept only as metadata. Calling the vanilla embedded-skin parser remains impossible
            // because v264 has no view offset and parse_wrath_skin is the explicit API.
        },
    };

    Ok(WrathM2 {
        format,
        skin_count,
        animations: parsed_animations,
        bone_tracks,
        cameras: parsed_cameras,
        camera_lookup,
        texture_combiner_combos,
        lights,
        ribbons,
    })
}

/// Parse one external Wrath `SKIN` profile.
pub fn parse_wrath_skin(bytes: &[u8]) -> Result<WrathSkin> {
    if bytes.len() < 4 || &bytes[..4] != b"SKIN" {
        return Err(WrathError::NotSkin);
    }
    if bytes.len() < 48 {
        return Err(WrathError::Truncated("SKIN header"));
    }
    let indices_arr = array(bytes, 4, "SKIN indices")?;
    let triangles_arr = array(bytes, 12, "SKIN triangles")?;
    let properties_arr = array(bytes, 20, "SKIN bone properties")?;
    let sections_arr = array(bytes, 28, "SKIN sections")?;
    let batches_arr = array(bytes, 36, "SKIN texture units")?;

    let indices = u16_table(bytes, indices_arr, "SKIN indices")?;
    let triangles = u16_table(bytes, triangles_arr, "SKIN triangles")?;
    if triangles
        .iter()
        .any(|&index| usize::from(index) >= indices.len())
    {
        return Err(WrathError::UnsupportedField(
            "SKIN triangle references a missing index",
        ));
    }
    // Not exposed by the current semantic Skin type, but require the declared byte table to fit.
    table(bytes, properties_arr, 4, "SKIN bone properties")?;

    let sections_raw = table(bytes, sections_arr, 48, "SKIN sections")?;
    let mut submeshes = Vec::with_capacity(sections_arr.0);
    for i in 0..sections_arr.0 {
        let s = record(sections_raw, i, 48, "SKIN section")?;
        let triangle_start = s
            .u16_at(8)
            .ok_or(WrathError::Truncated("SKIN section triangle start"))?;
        let triangle_count = s
            .u16_at(10)
            .ok_or(WrathError::Truncated("SKIN section triangle count"))?;
        let end = usize::from(triangle_start)
            .checked_add(usize::from(triangle_count))
            .ok_or(WrathError::Truncated("SKIN section triangle range"))?;
        if end > triangles.len() {
            return Err(WrathError::UnsupportedField(
                "SKIN section triangle range exceeds triangle table",
            ));
        }
        submeshes.push(SkinSection {
            id: s
                .u16_at(0)
                .ok_or(WrathError::Truncated("SKIN section id"))?,
            triangle_start,
            triangle_count,
        });
    }

    let batches_raw = table(bytes, batches_arr, 24, "SKIN texture units")?;
    let mut batches = Vec::with_capacity(batches_arr.0);
    for i in 0..batches_arr.0 {
        let u = record(batches_raw, i, 24, "SKIN texture unit")?;
        let skin_section_index = u
            .u16_at(4)
            .ok_or(WrathError::Truncated("SKIN texture-unit section"))?;
        if usize::from(skin_section_index) >= submeshes.len() {
            return Err(WrathError::UnsupportedField(
                "SKIN texture unit references a missing section",
            ));
        }
        batches.push(SkinBatch {
            flags: u
                .u16_at(0)
                .ok_or(WrathError::Truncated("SKIN batch flags"))?,
            shader_id: u.u16_at(2).ok_or(WrathError::Truncated("SKIN shader id"))?,
            skin_section_index,
            color_index: u.u16_at(8).ok_or(WrathError::Truncated("SKIN color"))?,
            material_index: u
                .u16_at(0x0a)
                .ok_or(WrathError::Truncated("SKIN material"))?,
            texture_count: u
                .u16_at(0x0e)
                .ok_or(WrathError::Truncated("SKIN texture count"))?,
            texture_combo_index: u
                .u16_at(0x10)
                .ok_or(WrathError::Truncated("SKIN texture combo"))?,
            texture_coord_combo_index: u
                .u16_at(0x12)
                .ok_or(WrathError::Truncated("SKIN texture coordinate combo"))?,
            weight_combo_index: u
                .u16_at(0x14)
                .ok_or(WrathError::Truncated("SKIN weight combo"))?,
            texture_transform_combo_index: u
                .u16_at(0x16)
                .ok_or(WrathError::Truncated("SKIN transform combo"))?,
        });
    }

    Ok(WrathSkin {
        indices,
        triangles,
        submeshes,
        batches,
    })
}
