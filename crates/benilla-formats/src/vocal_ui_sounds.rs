//! `VocalUISounds.dbc`, the error speech a character says in its own race and sex voice when the
//! client refuses something. One row per `(race, line)`, the line being the message catalog's
//! `type_tag` (the record's `+0x0c`), so a refusal reaching `CGGameUI::DisplayError` names it.
//!
//! Layout: 7 `u32` fields in 28-byte records (the builder `0x45813d` strides `0x1c`): `ID`, the
//! line (`0x45815c`), the race (`0x45816b`), the male and female kits (`0x458173`, `0x458180`, to
//! `[0xb06240 + line*12]` and `[0xb06570 + line*12]`), then the annoyed pair (`0x458190`,
//! `0x4581a3`, to `[0xb06244 + line*12]` and `[0xb06574 + line*12]`). A kit id of `0` or
//! `0xFFFF_FFFF` misses in the kit lookup (`0x45cda0`), so that line is silent.

use crate::Chain;
use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, u32_at};

const VOCAL_UI_SOUNDS: &str = "DBFilesClient\\VocalUISounds.dbc";

/// The reference table's line count, the bound its builder (`0x45815f`) and player (`0x45829c`)
/// test. A `type_tag` of exactly `0x44` means no speech: play the record's named cue instead.
pub const VOCAL_UI_LINES: usize = 0x44;

/// One `VocalUISounds.dbc` row: the kits one race says one line with, per sex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VocalUiSound {
    pub id: u32,
    /// `VocalUIEnum`, the message catalog's `type_tag`; the builder drops `VOCAL_UI_LINES` and up.
    pub line: u32,
    /// `ChrRaces` id (the descriptor's `UNIT_FIELD_BYTES_0` race byte).
    pub race: u32,
    /// The line's ordinary delivery, by sex (`0` male, `1` female). `0`/`-1` = none.
    pub normal: [u32; 2],
    /// The annoyed delivery by sex, played after four consecutive plays of the same line; no
    /// shipped id is in `SoundEntries.dbc`, so it is never heard in 5875.
    pub pissed: [u32; 2],
}

impl VocalUiSound {
    /// The ordinary line's kit at `sex`, `0` and `-1` folded to none. The reference refuses a
    /// `sex` past 1 at its entry (`0x458254`/`0x45825b`).
    pub fn normal_kit(&self, sex: u32) -> Option<u32> {
        kit(self.normal, sex)
    }

    /// The annoyed line's kit at `sex`, folded the same way.
    pub fn pissed_kit(&self, sex: u32) -> Option<u32> {
        kit(self.pissed, sex)
    }
}

fn kit(pair: [u32; 2], sex: u32) -> Option<u32> {
    let id = *pair.get(sex as usize)?;
    (id != 0 && id != u32::MAX).then_some(id)
}

/// The whole table as rows, from which consumers build the reference's per-race lookup.
pub struct VocalUiSoundCatalog {
    rows: Vec<VocalUiSound>,
}

impl VocalUiSoundCatalog {
    /// Every kept row in file order. The reference builds its table walking the file backwards
    /// (`0x458140`), so on a duplicate `(race, line)` the lowest-indexed row wins.
    pub fn rows(&self) -> &[VocalUiSound] {
        &self.rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Load `VocalUISounds.dbc` off the patch chain, dropping the rows the reference's builder drops
/// (`VocalUIEnum >= 0x44`).
pub fn load_vocal_ui_sounds(chain: &mut Chain) -> Result<VocalUiSoundCatalog> {
    let bytes = chain
        .read_file(VOCAL_UI_SOUNDS)
        .with_context(|| format!("reading {VOCAL_UI_SOUNDS}"))?;
    let mut schema = Schema::new("VocalUISounds");
    for i in 0..7 {
        schema.add_field(SchemaField::new(format!("f{i}"), FieldType::UInt32));
    }
    let rs = parse(&bytes, schema, "VocalUISounds")?;
    let mut rows = Vec::new();
    for r in rs.records() {
        let (Some(id), Some(line), Some(race)) = (u32_at(r, 0), u32_at(r, 1), u32_at(r, 2)) else {
            continue;
        };
        if line as usize >= VOCAL_UI_LINES {
            continue;
        }
        let f = |i| u32_at(r, i).unwrap_or(0);
        rows.push(VocalUiSound {
            id,
            line,
            race,
            normal: [f(3), f(4)],
            pissed: [f(5), f(6)],
        });
    }
    Ok(VocalUiSoundCatalog { rows })
}
