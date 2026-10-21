//! Authored records off the raw model bytes, in raw WoW model space: M2 lights (which `benilla-m2`
//! does not read), the WMO root's MOLT lights, and the M2 camera table.

use benilla_bytes::ByteExt;

/// An M2 light (the MD20 lights array, loaded by `0x70ebd0`), `position` relative to `bone` (`-1`
/// for the origin). The reference commits a point light on a placed prop as a hardware `GL_LIGHT`
/// with the fixed attenuation `1 / (0.7·d + 0.03·d²)`, diffuse only; the glue scenes light their
/// ambient through an ambient-only light, so both pairs are read. Colour and intensity are each
/// track's first value; [`Self::anim`] carries the tracks that move. The attenuation range is a
/// cull hint.
#[derive(Debug, Clone)]
pub struct M2Light {
    pub light_type: u16,
    pub bone: i16,
    pub position: [f32; 3],
    pub ambient_color: [f32; 3],
    pub ambient_intensity: f32,
    pub diffuse_color: [f32; 3],
    pub diffuse_intensity: f32,
    pub attenuation_start: f32,
    pub attenuation_end: f32,
    /// The light bone's +Z axis in model space at the track origin, a directional light's basis:
    /// `0x718a76` reads row 2 of the bone's pose matrix, never the def position.
    pub bone_z: [f32; 3],
    /// Both runtime gate bytes load as 1 and the animate loop rewrites the visibility byte only
    /// when the track has keys (`0x716413`), so a light is dark only when its first visibility key
    /// is 0. The values are bytes (`0x71646d`: `+0xec = visibilityValue[k0]`).
    pub visibility_off: bool,
    /// The diffuse and visibility tracks on their native clocks when any of them moves (a torch's
    /// flicker, a spell light keyed on later); `None` for a static light and the 1.12 parse.
    pub anim: Option<std::sync::Arc<M2LightAnim>>,
}

/// A 3.3.5 light's animated state, sampled every frame by the animate pass (`0x82f0f0`) and
/// re-gated from the visibility byte (`0x828a00`).
///
/// Unmodelled: the ambient pair (type 0 feeds the ambient term, not a point light) and the
/// model-alpha factor the reference multiplies into the diffuse product (`Model+0x198`).
#[derive(Debug, Clone)]
pub struct M2LightAnim {
    pub diffuse_color: crate::SeqTrack<[f32; 3]>,
    pub diffuse_intensity: crate::SeqTrack,
    /// The visibility byte as 0/1, stepped.
    pub visibility: crate::SeqTrack,
    pub sequences: crate::SeqTable,
}

impl M2LightAnim {
    /// Whether the visibility byte can ever read on.
    pub fn can_show(&self) -> bool {
        !self.visibility.is_keyed()
            || self.visibility.default > 0.0
            || self
                .visibility
                .slots
                .iter()
                .any(|l| l.keys.iter().any(|&(_, v)| v > 0.0))
    }

    /// `(diffuse colour, diffuse intensity)` at `clock`, intensity 0 while the byte is clear.
    pub fn diffuse_at(&self, clock: crate::SeqClock) -> ([f32; 3], f32) {
        let lit = self.visibility.sample(clock) > 0.0;
        (
            self.diffuse_color.sample(clock),
            if lit {
                self.diffuse_intensity.sample(clock)
            } else {
                0.0
            },
        )
    }
}

impl M2Light {
    /// A point light; type 0 feeds the ambient accumulator, not a GL light (`0x71bc70`).
    pub fn is_point(&self) -> bool {
        self.light_type == 1
    }

    /// The spawn gate: a point light its visibility track does not hold dark. An animated light
    /// spawns if its byte can ever read on, and its animator gates it live.
    pub fn casts(&self) -> bool {
        self.is_point()
            && self
                .anim
                .as_ref()
                .map_or(!self.visibility_off, |a| a.can_show())
    }
}

/// A WMO MOLT light (`SMOLight`, stride `0x30`), in WMO model space. The 1.12 client commits them
/// to the scene's dynamic lights every frame for every visible WMO (`0x695c00`, `0x71b650`), as
/// `colour × intensity` point lights at the fixed 0/0.7/0.03 falloff over the baked MOCV.
#[derive(Debug, Clone, Copy)]
pub struct WmoLight {
    pub light_type: u8,
    pub use_atten: bool,
    pub color: [f32; 3],
    pub position: [f32; 3],
    pub intensity: f32,
    pub attenuation_start: f32,
    pub attenuation_end: f32,
}

impl WmoLight {
    /// An omni light, the interior fixtures' type (1 is spot, 2 directional, 3 ambient).
    pub fn is_omni(&self) -> bool {
        self.light_type == 0
    }
}

/// Hamilton product `a·b` of two `[x, y, z, w]` quaternions (the vanilla M2 key layout).
pub(super) fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

/// `q · (0,0,1) · q⁻¹`, the +Z axis rotated by `q`, in closed form.
pub(super) fn quat_rotate_z(q: [f32; 4]) -> [f32; 3] {
    let [x, y, z, w] = q;
    [
        2.0 * (x * z + w * y),
        2.0 * (y * z - w * x),
        1.0 - 2.0 * (x * x + y * y),
    ]
}

/// The model's portrait camera, which the client renders every unit-frame portrait through:
/// the bake at `0x524f60` takes camera `cameraLookup[0]` and builds `lookAt(position, target,
/// up-from-roll)` with the diagonal-FOV perspective (`0x5c3cc0`, half-angle `(fov/2)/√(aspect²+1)`)
/// at a fixed 4:3, a vertical half-angle of `0.3·fov`, with nothing on top. Model space, radians.
#[derive(Debug, Clone, Copy)]
pub struct M2PortraitCamera {
    /// The diagonal opening angle, not fovy (`0x70ebd0` to `0x5c3cc0`).
    pub fov: f32,
    pub far_clip: f32,
    pub near_clip: f32,
    /// The eye, like `target` its base plus its track's first key; `roll` is a first key alone.
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub roll: f32,
}

/// The portrait camera, `cameraLookup[0]` into the camera table as the reference selects it;
/// `None` without one, the `0xffff` sentinel included.
pub fn parse_m2_portrait_camera(bytes: &[u8]) -> Option<M2PortraitCamera> {
    let model = benilla_m2::wrath::parse_wrath_m2(&mut std::io::Cursor::new(bytes)).ok()?;
    let idx = *model.camera_lookup().first()? as usize;
    still_camera(model.cameras().get(idx)?)
}

/// One camera record by table index, the glue screens' `Model:SetCamera(idx)` path: their one
/// camera has the `0xffff` sentinel in its lookup slot, so the client indexes the table directly.
/// The rig at the start of sequence 0.
pub fn parse_m2_camera(bytes: &[u8], index: usize) -> Option<M2PortraitCamera> {
    let model = benilla_m2::wrath::parse_wrath_m2(&mut std::io::Cursor::new(bytes)).ok()?;
    still_camera(model.cameras().get(index)?)
}

fn still_camera(camera: &benilla_m2::wrath::WrathCamera) -> Option<M2PortraitCamera> {
    let pose = super::wrath::sample_wrath_camera(camera, 0, 0, 0).ok()?;
    Some(M2PortraitCamera {
        fov: pose.fov,
        far_clip: pose.far_clip,
        near_clip: pose.near_clip,
        position: pose.position,
        target: pose.target,
        roll: pose.roll,
    })
}

/// One `SMOLight` at a bounds-checked `r`: type@0, useAtten@1, colour (BGRA)@4, position@8,
/// intensity@0x14, attenStart@0x28, attenEnd@0x2c. The attenuation the reference uses is the
/// tail; `+0x18..+0x28` hold four other floats (≈0, −0, −1, −0.5 on every vanilla light).
fn read_wmo_light(b: &[u8], r: usize) -> Option<WmoLight> {
    Some(WmoLight {
        light_type: b.u8_at(r)?,
        use_atten: b.u8_at(r + 1)? != 0,
        // CImVector is BGRA in memory.
        color: [
            f32::from(b.u8_at(r + 6)?) / 255.0,
            f32::from(b.u8_at(r + 5)?) / 255.0,
            f32::from(b.u8_at(r + 4)?) / 255.0,
        ],
        position: [b.f32_at(r + 8)?, b.f32_at(r + 12)?, b.f32_at(r + 16)?],
        intensity: b.f32_at(r + 0x14)?,
        attenuation_start: b.f32_at(r + 0x28)?,
        attenuation_end: b.f32_at(r + 0x2c)?,
    })
}

/// A WMO root file's MOLT lights, which `benilla-wmo` does not expose: the chunk list scanned for
/// `MOLT` (stored reversed, `TLOM`). The walk must not stop at a zero-size chunk: `MOVV(0)` and
/// `MOVB(0)` sit right before MOLT in vanilla roots. Hand-rolled, not `benilla_bytes::chunks()`:
/// each record is bounded by the whole buffer, not by a clamped chunk payload.
pub fn parse_wmo_lights(root_bytes: &[u8]) -> Vec<WmoLight> {
    let b = root_bytes;
    let mut o = 0usize;
    while o + 8 <= b.len() {
        let Some(size) = b.u32_at(o + 4).map(|v| v as usize) else {
            break;
        };
        let data = o + 8;
        if &b[o..o + 4] == b"TLOM" || &b[o..o + 4] == b"MOLT" {
            let n = size / 0x30;
            let mut out = Vec::with_capacity(n.min(256));
            for i in 0..n {
                let Some(r) = data.checked_add(i * 0x30).filter(|&r| r + 0x30 <= b.len()) else {
                    break;
                };
                match read_wmo_light(b, r) {
                    Some(light) => out.push(light),
                    None => break, // unreachable after the filter above
                }
            }
            return out;
        }
        o = match data.checked_add(size) {
            Some(x) => x,
            None => break,
        };
    }
    Vec::new()
}
