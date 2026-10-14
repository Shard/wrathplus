//! Offline showcase path data, independent of stock DBC cinematics and eval captures.
//! This unit has no plugin, environment reads, camera writes or clock overrides.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathFile {
    map: u32,
    /// Fixed light-band minute, not elapsed simulation time.
    minute: f32,
    keyframes: Vec<Keyframe>,
    #[serde(default)]
    actors: Vec<ShowcaseActor>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Keyframe {
    at: f64,
    /// Absolute WoW coordinates, yards. Conversion belongs to the camera driver.
    eye: [f64; 3],
    target: [f64; 3],
    #[serde(default = "default_fov")]
    fov_degrees: f64,
}

/// Visual-only installed CreatureDisplayInfo id, never a server creature entry or GUID.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShowcaseActor {
    pub(crate) display_id: u32,
    pub(crate) position: [f64; 3],
    /// WoW heading radians, bounded to one turn; the visual spawner owns coordinate conversion.
    #[serde(default)]
    pub(crate) yaw: f64,
    /// Multiplier on the display's installed model scale, not an absolute game unit size.
    #[serde(default = "default_actor_scale")]
    pub(crate) scale: f64,
}

fn default_actor_scale() -> f64 {
    1.0
}

fn default_fov() -> f64 {
    60.0
}

pub(crate) const FPS: u32 = 60;
pub(crate) const WIDTH: u32 = 1920;
pub(crate) const HEIGHT: u32 = 1080;
const MAX_FRAMES: u32 = FPS * 600;
const MAX_COORD: f64 = 100_000.0;

/// Only validated data reaches a future driver; frame times never accumulate float deltas.
#[derive(Debug)]
pub(crate) struct ShowcasePath {
    pub(crate) map: u32,
    pub(crate) minute: f32,
    pub(crate) actors: Vec<ShowcaseActor>,
    keys: Vec<Keyframe>,
    frames: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CameraSample {
    pub(crate) seconds: f64,
    pub(crate) eye: [f64; 3],
    pub(crate) target: [f64; 3],
    pub(crate) fov_degrees: f64,
}

fn usable_view(eye: [f64; 3], target: [f64; 3]) -> bool {
    eye.into_iter()
        .zip(target)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        > 1e-8
        && eye
            .into_iter()
            .zip(target)
            .map(|(a, b)| ((a as f32) - (b as f32)).powi(2))
            .sum::<f32>()
            > 1e-8
}

impl ShowcasePath {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let file: PathFile = toml::from_str(text).map_err(|e| e.to_string())?;
        if !file.minute.is_finite() || !(0.0..1440.0).contains(&file.minute) {
            return Err("minute must be finite and in [0, 1440)".into());
        }
        if !(2..=256).contains(&file.keyframes.len()) {
            return Err("a path needs 2..256 keyframes".into());
        }
        if file.actors.len() > 16 {
            return Err("a showcase supports at most 16 visual actors".into());
        }
        for actor in &file.actors {
            if actor.display_id == 0
                || !actor
                    .position
                    .into_iter()
                    .all(|v| v.is_finite() && v.abs() <= MAX_COORD)
                || !actor.yaw.is_finite()
                || actor.yaw.abs() > std::f64::consts::TAU
                || !actor.scale.is_finite()
                || !(0.1..=4.0).contains(&actor.scale)
            {
                return Err(
                    "actor needs nonzero display_id, bounded finite position/yaw and scale 0.1..4"
                        .into(),
                );
            }
        }
        let mut previous = -1.0;
        for (index, key) in file.keyframes.iter().enumerate() {
            if !key.at.is_finite()
                || key.at < 0.0
                || key.at <= previous
                || (index == 0 && key.at != 0.0)
            {
                return Err("keyframe times must start at zero and increase strictly".into());
            }
            previous = key.at;
            if !key
                .eye
                .into_iter()
                .chain(key.target)
                .all(|v| v.is_finite() && v.abs() <= MAX_COORD)
                || !usable_view(key.eye, key.target)
            {
                return Err("camera coordinates must be finite, bounded and eye != target".into());
            }
            if !key.fov_degrees.is_finite() || !(10.0..=120.0).contains(&key.fov_degrees) {
                return Err("fov_degrees must be in [10, 120]".into());
            }
        }
        let frame_span = previous * f64::from(FPS);
        if frame_span < 1.0
            || frame_span > f64::from(MAX_FRAMES)
            || (frame_span - frame_span.round()).abs() > 1e-7
        {
            return Err(
                "duration must be 1/60..600 seconds and a whole number of 60fps frames".into(),
            );
        }
        let path = Self {
            map: file.map,
            minute: file.minute,
            actors: file.actors,
            keys: file.keyframes,
            frames: frame_span.round() as u32,
        };
        // A route can cross eye and target between valid endpoints. Refuse such output instead
        // of passing a zero direction to looking_at; there are at most 36,001 samples to check.
        for index in 0..=path.frames {
            path.sample_time(f64::from(index) / f64::from(FPS))?;
        }
        Ok(path)
    }

    /// N frames cover [0, duration). No duplicate final endpoint extends the encoded clip.
    pub(crate) fn frame_count(&self) -> u32 {
        self.frames
    }

    pub(crate) fn sample_frame(&self, index: u32) -> Result<CameraSample, String> {
        if index >= self.frames {
            return Err("frame index is outside the sequence".into());
        }
        self.sample_time(f64::from(index) / f64::from(FPS))
    }

    fn sample_time(&self, seconds: f64) -> Result<CameraSample, String> {
        let right = self
            .keys
            .partition_point(|k| k.at <= seconds)
            .min(self.keys.len() - 1)
            .max(1);
        let a = &self.keys[right - 1];
        let b = &self.keys[right];
        let u = ((seconds - a.at) / (b.at - a.at)).clamp(0.0, 1.0);
        // Cubic ease-in/out, C1 continuous at every authored waypoint, without spline overshoot.
        // Deliberately comes to rest at each key: authors can use a two-key flight for no stops.
        let weight = u * u * (3.0 - 2.0 * u);
        let blend = |x: f64, y: f64| x + (y - x) * weight;
        let eye = std::array::from_fn(|i| blend(a.eye[i], b.eye[i]));
        let target = std::array::from_fn(|i| blend(a.target[i], b.target[i]));
        if !usable_view(eye, target) {
            return Err(format!("eye and target coincide at {seconds:.6}s"));
        }
        Ok(CameraSample {
            seconds,
            eye,
            target,
            fov_degrees: blend(a.fov_degrees, b.fov_degrees),
        })
    }
}

#[cfg(test)]
mod tests;

pub(crate) mod driver;
