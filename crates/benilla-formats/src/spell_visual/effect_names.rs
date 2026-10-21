//! `SpellVisualEffectName.dbc`, build 12340's seven columns: id, name, model path, area size and
//! the three scale floats a `CEffect` sizes its model by (`0x6f8c50`, `0x6f8ae0`).

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, parse, str_at, u32_at};
use crate::Chain;

const SPELL_VISUAL_EFFECT_NAME: &str = "DBFilesClient\\SpellVisualEffectName.dbc";

/// The world size below which `CEffect::UpdateAttachment` skips the bounds (`[0xa32b48]`, 1e-6).
const ATTACH_SIZE_EPSILON: f32 = 1e-6;

/// A row's model scale (field 4) and the bounds on the size it reaches in the world (fields 5-6,
/// `MinAllowedScale`/`MaxAllowedScale`). A missing row scales nothing and bounds nothing, as a
/// `CEffect` with no record (`[effect+0x20] == 0`) skips both.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectScale {
    pub scale: f32,
    pub min: f32,
    pub max: f32,
}

impl Default for EffectScale {
    fn default() -> Self {
        Self {
            scale: 1.0,
            min: 0.0,
            max: f32::INFINITY,
        }
    }
}

impl EffectScale {
    /// The local scale of an effect riding an attach point, `CEffect::UpdateAttachment`
    /// (`0x6f8c50`): `attached` (the unit's `CreatureModelData.AttachedEffectScale`, `+0x60`, or 1
    /// off a unit) × the row's scale, cut back or grown so that the size in the world, × `point`
    /// (the attach point's world scale, `0x831550`), lands inside the bounds.
    pub fn attached(&self, attached: f32, point: f32) -> f32 {
        let local = attached * self.scale;
        let world = point * local;
        if world > ATTACH_SIZE_EPSILON {
            if world > self.max {
                return local / world * self.max;
            }
            if world < self.min {
                return local * (self.min / world);
            }
        }
        local
    }

    /// The scale of an effect planted in the world, `0x6f8ae0`: `base` × the row's scale, held to
    /// the bounds, and 1 when that is not positive. `base` is 1, or for an effect planted at its
    /// owner (flag `0x200`) the owner's scale × `CreatureModelData.WorldEffectScale` (`+0x5c`,
    /// `0x6f7950`), whose footprint factor `max(1, min extent × [0xa32b2c])` is not applied here.
    pub fn world(&self, base: f32) -> f32 {
        let s = base * self.scale;
        let s = if s < self.min {
            self.min
        } else if s < self.max {
            s
        } else {
            self.max
        };
        if s > 0.0 {
            s
        } else {
            1.0
        }
    }
}

/// The table's rows, split by use.
pub(super) struct EffectNames {
    pub(super) paths: HashMap<u32, String>,
    /// The `"HARDCODED …"` rows, name → (id, path).
    pub(super) hardcoded: HashMap<String, (u32, String)>,
    pub(super) scales: HashMap<u32, EffectScale>,
}

fn schema() -> Schema {
    let mut s = Schema::new("SpellVisualEffectName");
    for i in 0..7 {
        let ty = match i {
            1 | 2 => FieldType::String,
            3.. => FieldType::Float32,
            _ => FieldType::UInt32,
        };
        s.add_field(SchemaField::new(format!("F{i}"), ty));
    }
    s
}

pub(super) fn load(chain: &mut Chain) -> Result<EffectNames> {
    let bytes = chain
        .read_file(SPELL_VISUAL_EFFECT_NAME)
        .context("reading SpellVisualEffectName.dbc")?;
    let set = parse(&bytes, schema(), "SpellVisualEffectName.dbc")?;
    let mut names = EffectNames {
        paths: HashMap::with_capacity(set.records().len()),
        hardcoded: HashMap::new(),
        scales: HashMap::with_capacity(set.records().len()),
    };
    for r in set.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        if let Some(path) = str_at(&set, r, 2) {
            // Only the rows the client's boot resolve can hit, those named "HARDCODED …".
            if let Some(name) = str_at(&set, r, 1).filter(|n| n.starts_with("HARDCODED ")) {
                names.hardcoded.insert(name, (id, path.clone()));
            }
            names.paths.insert(id, path);
        }
        let f = |i| f32_at(r, i).context("SpellVisualEffectName scale");
        names.scales.insert(
            id,
            EffectScale {
                scale: f(4)?,
                min: f(5)?,
                max: f(6)?,
            },
        );
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::EffectScale;

    const ROW: EffectScale = EffectScale {
        scale: 2.0,
        min: 0.5,
        max: 3.0,
    };

    #[test]
    fn an_attached_effect_is_held_to_its_world_size_bounds() {
        // In bounds: the model's scale × the row's.
        assert_eq!(ROW.attached(1.0, 1.0), 2.0);
        assert_eq!(ROW.attached(0.5, 1.0), 1.0);
        // A big host: 4 × 2 = 8 in the world, cut to the max 3.
        assert_eq!(ROW.attached(1.0, 4.0) * 4.0, 3.0);
        // A tiny host: 0.1 × 2 = 0.2, grown to the min 0.5.
        assert!((ROW.attached(1.0, 0.1) * 0.1 - 0.5).abs() < 1e-6);
        // A degenerate point skips the bounds.
        assert_eq!(ROW.attached(1.0, 0.0), 2.0);
        assert_eq!(EffectScale::default().attached(1.5, 40.0), 1.5);
    }

    #[test]
    fn a_world_effect_clamps_and_never_vanishes() {
        assert_eq!(ROW.world(1.0), 2.0);
        assert_eq!(ROW.world(10.0), 3.0);
        assert_eq!(ROW.world(0.1), 0.5);
        let unbounded = EffectScale {
            min: -1.0,
            ..EffectScale::default()
        };
        assert_eq!(
            unbounded.world(-2.0),
            1.0,
            "a non-positive scale falls back to 1"
        );
    }
}
