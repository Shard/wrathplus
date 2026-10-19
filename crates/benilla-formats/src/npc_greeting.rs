//! The NPC greeting voice, a per-display property apart from the body vocals of
//! [`crate::creature_sound`]: `UNIT_FIELD_DISPLAYID` → `CreatureDisplayInfo.dbc` field 11
//! (`NPCSoundID`, row `+0x2c`; loader `0x542e90`, 12 columns) → `NPCSounds.dbc` (`0x54afa0`: `ID`,
//! `hello`, `goodbye`, `pissed`, and `ack`, 0 on every row and never read). Each is a
//! `SoundEntries` kit of type 17, no-duplicates (`0x20`), cut off at 45 yd, a 3D emitter at the NPC
//! (`0x7a5b10`). `CreatureSoundData.NPCSoundID` is not on the path (`0x60c3b0`, `0x623910`).

use std::collections::HashMap;

use crate::Chain;
use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, u32_at};

/// The greeting kit set for one NPC (each a `SoundEntries` kit id; `0` = none).
#[derive(Clone, Copy, Debug)]
pub struct NpcGreeting {
    /// `NPCSounds.hello`, played on a left-click select (`0x60c270`) and once when an interaction
    /// window opens (`0x60c3b0(1)`).
    pub hello: u32,
    /// `NPCSounds.goodbye`, played when the interaction window closes to nothing (`0x60c3b0(0)`),
    /// not when it swaps to another NPC.
    pub goodbye: u32,
    /// `NPCSounds.pissed`: after five left-click hellos its variations play in turn, then the
    /// cycle restarts (`0x623910` reads row `+0xc`).
    pub pissed: u32,
}

/// Display id → greeting kits, joined through `CreatureDisplayInfo` field 11 → `NPCSounds`.
pub struct NpcGreetingCatalog {
    display_to_sound: HashMap<u32, u32>,
    rows: HashMap<u32, NpcGreeting>,
}

impl NpcGreetingCatalog {
    /// The greeting for a display id; `None` for a display without an `NPCSoundID`, as beasts and
    /// most non-character models are.
    pub fn for_display(&self, display_id: u32) -> Option<&NpcGreeting> {
        self.rows.get(self.display_to_sound.get(&display_id)?)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

fn npcsounds_schema() -> Schema {
    let mut s = Schema::new("NPCSounds");
    for i in 0..5 {
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

/// Read `NPCSounds.dbc` + `CreatureDisplayInfo.dbc` off the patch chain into the joined catalog.
pub fn load_npc_greeting_catalog(chain: &mut Chain) -> Result<NpcGreetingCatalog> {
    let bytes = chain
        .read_file("DBFilesClient\\NPCSounds.dbc")
        .context("reading NPCSounds.dbc")?;
    let rs = parse(&bytes, npcsounds_schema(), "NPCSounds")?;
    let mut rows = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        rows.insert(
            id,
            NpcGreeting {
                hello: u32_at(r, 1).unwrap_or(0),
                goodbye: u32_at(r, 2).unwrap_or(0),
                pissed: u32_at(r, 3).unwrap_or(0),
            },
        );
    }

    let bytes = chain
        .read_file("DBFilesClient\\CreatureDisplayInfo.dbc")
        .context("reading CreatureDisplayInfo.dbc")?;
    let rs = parse(&bytes, cdi_schema(), "CreatureDisplayInfo")?;
    let mut display_to_sound = HashMap::new();
    for r in rs.records() {
        let (Some(id), Some(npc_sound)) = (u32_at(r, 0), u32_at(r, 11)) else {
            continue;
        };
        if npc_sound != 0 {
            display_to_sound.insert(id, npc_sound);
        }
    }
    Ok(NpcGreetingCatalog {
        display_to_sound,
        rows,
    })
}
