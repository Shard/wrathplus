//! Creature voice: `CreatureDisplayInfo.SoundID` to `CreatureSoundData.dbc`, 30 `u32` columns of
//! per-display voice kits whose names are the community's, since a DBC carries none. Column 22
//! (`NPCSoundID`) is empty on every row and unread. A display whose own `SoundID` is 0, nearly
//! every one, resolves through `CreatureModelData.SoundID` (col 13), as the client does.

use std::collections::HashMap;

use crate::Chain;
use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, u32_at};

/// One `CreatureSoundData` row; every field is a `SoundEntries` kit id (0 for none) unless noted.
pub struct CreatureVoice {
    /// Attack grunt `[normal, critical]`.
    pub exertion: [u32; 2],
    /// Wound vocal `[normal, critical, crushing]`.
    pub injury: [u32; 3],
    pub death: u32,
    pub stun: u32,
    /// Fired by the `$FDX` anim event.
    pub stand: u32,
    /// The footstep class (`FootstepTerrainLookup.CreatureFootstepID`), not a kit id.
    pub footstep_class: u32,
    pub aggro: u32,
    /// Fired by `$WNG` / `$WGG`.
    pub wing_flap: u32,
    pub wing_glide: u32,
    pub alert: u32,
    /// Fired by `$FD1`..`$FD4`.
    pub fidget: [u32; 4],
    /// Fired by `$AH0`..`$AH3`.
    pub custom_attack: [u32; 4],
    /// A looping body sound (SoundEntries type 27).
    pub loop_sound: u32,
    /// Melee impact material (`WeaponImpactSounds`): 0 flesh, 1 stone, 2 wood, 3 ethereal.
    pub impact_type: u32,
    pub jump_start: u32,
    pub jump_end: u32,
    /// Column 27, the bark on an attack order: `SMSG_PET_ACTION_SOUND`'s `PET_TALK_ATTACK`, bark
    /// state 2 of `0x623a40` (`0x623ad3` reads `[row+0x6c]`).
    pub pet_attack: u32,
    /// Column 28, the bark acknowledging an ordered spell: `PET_TALK_SPECIAL_SPELL`, bark state 1
    /// (`0x623ac8` reads `[row+0x70]`).
    pub pet_order: u32,
    /// Column 29, `SMSG_PET_DISMISS_SOUND`'s parting line: `0x604140` reads `[row+0x74]`
    /// (`0x6041c4`, `0x6041dc`) and plays it at a bare world position, since the pet is gone.
    pub pet_dismiss: u32,
}

/// Voice rows by display id through `CreatureDisplayInfo.SoundID`, and by model id through
/// `CreatureModelData.SoundID` for the dismiss sound.
pub struct CreatureVoiceCatalog {
    display_to_sound: HashMap<u32, u32>,
    model_to_sound: HashMap<u32, u32>,
    rows: HashMap<u32, CreatureVoice>,
}

impl CreatureVoiceCatalog {
    /// The voice set for a creature display id (`UNIT_FIELD_DISPLAYID`).
    pub fn for_display(&self, display_id: u32) -> Option<&CreatureVoice> {
        self.rows.get(self.display_to_sound.get(&display_id)?)
    }

    /// The voice set for a `CreatureModelData` id through its own `SoundID` (col 13), with no
    /// display step and no fallback: `SMSG_PET_DISMISS_SOUND` names a model, and `0x604140`
    /// resolves it so (`[[0xc0de68] + id*4] + 0x34`, then `[[0xc0de54] + sound*4]`).
    pub fn for_model(&self, model_id: u32) -> Option<&CreatureVoice> {
        self.rows.get(self.model_to_sound.get(&model_id)?)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

fn csd_schema() -> Schema {
    let mut s = Schema::new("CreatureSoundData");
    for i in 0..30 {
        s.add_field(SchemaField::new(format!("f{i}"), FieldType::UInt32));
    }
    s
}

fn cdi_schema() -> Schema {
    let mut s = Schema::new("CreatureDisplayInfo");
    for i in 0..12 {
        s.add_field(SchemaField::new(format!("f{i}"), FieldType::UInt32));
    }
    s
}

fn cmd_schema() -> Schema {
    let mut s = Schema::new("CreatureModelData");
    for i in 0..16 {
        s.add_field(SchemaField::new(format!("f{i}"), FieldType::UInt32));
    }
    s
}

/// Read the three tables off the patch chain into the joined catalog.
pub fn load_creature_voice_catalog(chain: &mut Chain) -> Result<CreatureVoiceCatalog> {
    let bytes = chain
        .read_file("DBFilesClient\\CreatureSoundData.dbc")
        .context("reading CreatureSoundData.dbc")?;
    let rs = parse(&bytes, csd_schema(), "CreatureSoundData")?;
    let mut rows = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let g = |i: usize| u32_at(r, i).unwrap_or(0);
        rows.insert(
            id,
            CreatureVoice {
                exertion: [g(1), g(2)],
                injury: [g(3), g(4), g(5)],
                death: g(6),
                stun: g(7),
                stand: g(8),
                footstep_class: g(9),
                aggro: g(10),
                wing_flap: g(11),
                wing_glide: g(12),
                alert: g(13),
                fidget: [g(14), g(15), g(16), g(17)],
                custom_attack: [g(18), g(19), g(20), g(21)],
                loop_sound: g(23),
                impact_type: g(24),
                jump_start: g(25),
                jump_end: g(26),
                pet_attack: g(27),
                pet_order: g(28),
                pet_dismiss: g(29),
            },
        );
    }

    let bytes = chain
        .read_file("DBFilesClient\\CreatureModelData.dbc")
        .context("reading CreatureModelData.dbc")?;
    let rs = parse(&bytes, cmd_schema(), "CreatureModelData")?;
    let mut model_to_sound = HashMap::new();
    for r in rs.records() {
        if let (Some(id), Some(sound)) = (u32_at(r, 0), u32_at(r, 13)) {
            if sound != 0 {
                model_to_sound.insert(id, sound);
            }
        }
    }

    let bytes = chain
        .read_file("DBFilesClient\\CreatureDisplayInfo.dbc")
        .context("reading CreatureDisplayInfo.dbc")?;
    let rs = parse(&bytes, cdi_schema(), "CreatureDisplayInfo")?;
    let mut display_to_sound = HashMap::new();
    for r in rs.records() {
        let (Some(id), Some(sound), Some(model)) = (u32_at(r, 0), u32_at(r, 2), u32_at(r, 1))
        else {
            continue;
        };
        // The display's own `SoundID` wins; 0 falls back to the model's.
        let sound = if sound != 0 {
            Some(sound)
        } else {
            model_to_sound.get(&model).copied()
        };
        if let Some(sound) = sound {
            display_to_sound.insert(id, sound);
        }
    }
    Ok(CreatureVoiceCatalog {
        display_to_sound,
        model_to_sound,
        rows,
    })
}
