//! `CameraShakes.dbc`, the 24 shipped camera-shake presets, and `SpellEffectCameraShakes.dbc`,
//! the 9 groups of up to three presets fired at one point.
//!
//! Two id spaces, never conflated: the creature footstep and death-thud columns
//! (`CreatureModelData` fields 11 and 12, read through
//! [`CreatureCatalog::footstep_shake`](crate::CreatureCatalog::footstep_shake)) name a preset
//! directly, while `SpellVisualKit` field 14 and the `$SHK` animation event name a group, never a
//! preset (`[0xc0d814]`, bound `0xc0d818`). Column names follow vmangos `DBCStructure.h`; what
//! `ShakeType` and `Direction` select is the evaluator's, `wrathplus-app`'s `camera_shake`.

use std::collections::HashMap;

use anyhow::{Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, parse, u32_at};
use crate::Chain;

const CAMERA_SHAKES: &str = "DBFilesClient\\CameraShakes.dbc";
const SPELL_EFFECT_CAMERA_SHAKES: &str = "DBFilesClient\\SpellEffectCameraShakes.dbc";

/// One `CameraShakes.dbc` row: a shake's authored shape, in the table's own units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraShake {
    pub id: u32,
    /// `ShakeType`: 0 or 1 as shipped; every creature row is 1.
    pub shake_type: u32,
    /// `Direction`: 0, 1 or 2, one per member of a spell-side triple; every creature row is 2.
    pub direction: u32,
    pub amplitude: f32,
    pub frequency: f32,
    /// Seconds.
    pub duration: f32,
    pub phase: f32,
    pub coefficient: f32,
}

/// One `SpellEffectCameraShakes.dbc` row: up to three [`CameraShake`] ids fired at one point. The
/// reference walks the three slots in order and skips a zero (`0x6ecb40`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellShakeGroup {
    pub id: u32,
    /// As shipped, zeros included; [`Self::shakes`] skips them.
    pub slots: [u32; 3],
}

impl SpellShakeGroup {
    /// The populated slots in walk order. A duplicate (group 4 is `15, 14, 15`) is kept: the slots
    /// are slots, not axes, and it loses the evaluator's strict `>` tie-break.
    pub fn shakes(&self) -> impl Iterator<Item = u32> + '_ {
        self.slots.iter().copied().filter(|&id| id != 0)
    }
}

/// Both camera-shake tables, presets and groups each keyed by their own id. `Default` is the empty
/// catalog, which is what a failed load means to every consumer.
#[derive(Default)]
pub struct CameraShakeCatalog {
    rows: HashMap<u32, CameraShake>,
    groups: HashMap<u32, SpellShakeGroup>,
}

impl CameraShakeCatalog {
    /// One preset by id; 0, the creature columns' "no shake", is never a row.
    pub fn get(&self, id: u32) -> Option<&CameraShake> {
        self.rows.get(&id)
    }

    /// Every row, unordered.
    pub fn iter(&self) -> impl Iterator<Item = &CameraShake> {
        self.rows.values()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// One group by id; 0, the kit column's "no shake", is never a row.
    pub fn group(&self, id: u32) -> Option<&SpellShakeGroup> {
        self.groups.get(&id)
    }

    /// Every group, unordered.
    pub fn groups(&self) -> impl Iterator<Item = &SpellShakeGroup> {
        self.groups.values()
    }

    pub fn group_len(&self) -> usize {
        self.groups.len()
    }

    /// Seed one preset, for fixtures that run the evaluator without an install.
    #[must_use]
    pub fn with_row(mut self, row: CameraShake) -> Self {
        self.rows.insert(row.id, row);
        self
    }

    /// Seed one group, for fixtures.
    #[must_use]
    pub fn with_group(mut self, group: SpellShakeGroup) -> Self {
        self.groups.insert(group.id, group);
        self
    }
}

fn camera_shakes_schema() -> Schema {
    let mut s = Schema::new("CameraShakes");
    for (name, ty) in [
        ("ID", FieldType::UInt32),
        ("ShakeType", FieldType::UInt32),
        ("Direction", FieldType::UInt32),
        ("Amplitude", FieldType::Float32),
        ("Frequency", FieldType::Float32),
        ("Duration", FieldType::Float32),
        ("Phase", FieldType::Float32),
        ("Coefficient", FieldType::Float32),
    ] {
        s.add_field(SchemaField::new(name, ty));
    }
    s
}

fn spell_effect_camera_shakes_schema() -> Schema {
    let mut s = Schema::new("SpellEffectCameraShakes");
    for name in ["ID", "CameraShake1", "CameraShake2", "CameraShake3"] {
        s.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    s
}

/// Read both camera-shake tables off the patch chain. A missing or malformed table is an error;
/// the caller's fallback is the empty catalog, no shakes at all.
pub fn load_camera_shakes(chain: &mut Chain) -> Result<CameraShakeCatalog> {
    let bytes = chain
        .read_file(CAMERA_SHAKES)
        .with_context(|| format!("reading {CAMERA_SHAKES}"))?;
    let rs = parse(&bytes, camera_shakes_schema(), "CameraShakes")?;
    let mut rows = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        rows.insert(
            id,
            CameraShake {
                id,
                shake_type: u32_at(r, 1).unwrap_or(0),
                direction: u32_at(r, 2).unwrap_or(0),
                amplitude: f32_at(r, 3).unwrap_or(0.0),
                frequency: f32_at(r, 4).unwrap_or(0.0),
                duration: f32_at(r, 5).unwrap_or(0.0),
                phase: f32_at(r, 6).unwrap_or(0.0),
                coefficient: f32_at(r, 7).unwrap_or(0.0),
            },
        );
    }

    let bytes = chain
        .read_file(SPELL_EFFECT_CAMERA_SHAKES)
        .with_context(|| format!("reading {SPELL_EFFECT_CAMERA_SHAKES}"))?;
    let gs = parse(
        &bytes,
        spell_effect_camera_shakes_schema(),
        "SpellEffectCameraShakes",
    )?;
    let mut groups = HashMap::with_capacity(gs.records().len());
    for r in gs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let mut slots = [0; 3];
        for (i, slot) in slots.iter_mut().enumerate() {
            *slot = u32_at(r, 1 + i).unwrap_or(0);
        }
        groups.insert(id, SpellShakeGroup { id, slots });
    }

    Ok(CameraShakeCatalog { rows, groups })
}
