//! The unit tooltip's quest lines, 12340 `0x621070` after the Leader line (`0x621e66`) and before
//! the master looter: with `showQuestTrackingTooltips` on, each quest in the log the hovered
//! creature serves adds its title, then one ` - <objective>` line per objective it counts for (its
//! entry or a kill credit through `0x5e2370`, a quest item it drops through `0x5e2630`) and per
//! quest source item it drops while the quest is not counted out (`0x5e0ea0`). A finished objective
//! still shows: neither predicate's done word is read. The app builds the blocks; this draws them.
//! Labelled: the skip for a unit whose `UNIT_FIELD_SUMMON` word pair holds a player guid
//! (descriptor `+0x20`/`+0x24`) is not read: that descriptor is not yet projected. This slice
//! does not qualify summoned-unit parity.

use std::collections::HashMap;

use mlua::{Lua, Table};

use super::tooltip::append_line;
use super::{Model, UiScript};

/// `0xad2d2c` = `0xffffd200`, the quest title.
const TITLE: [f32; 4] = [1.0, 210.0 / 255.0, 0.0, 1.0];
/// `0xad2d30` = `0xffffffff`, each objective line.
const LINE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// One quest's block under a creature's tooltip: its title and the objective texts, unprefixed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitQuestBlock {
    pub title: String,
    pub lines: Vec<String>,
}

/// The blocks by creature entry, in quest log order, as the app last published them.
#[derive(Default)]
struct UnitQuestLines(HashMap<u32, Vec<UnitQuestBlock>>);

/// A published quest row as the tooltip reads it: quest id, title, whether the row is complete,
/// and each objective line's POI slot bit with its text.
pub type QuestRowLines = (u32, String, bool, Vec<(Option<u8>, String)>);

impl UiScript {
    /// Publish the quest blocks every creature entry draws on its tooltip.
    pub fn set_unit_quest_lines(&mut self, by_entry: HashMap<u32, Vec<UnitQuestBlock>>) {
        self.lua().set_app_data(UnitQuestLines(by_entry));
    }
}

/// The creature entry a guid carries, as `0x621070` reads it with no object: the low dword's top
/// byte and the high dword's low 20 bits, bits 24..52.
pub(super) fn entry_of(guid: u64) -> u32 {
    ((guid >> 24) & 0x0fff_ffff) as u32
}

/// Append `guid`'s quest blocks to the tooltip `this`, gated on `showQuestTrackingTooltips`.
pub(super) fn append(lua: &Lua, this: &Table, guid: u64) -> mlua::Result<()> {
    let on = {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        let value = model
            .cvars
            .get("showquesttrackingtooltips")
            .map(|c| c.value.clone());
        value.is_some_and(|v| v.trim().parse::<f64>().is_ok_and(|n| n as i64 != 0))
    };
    let blocks = lua
        .app_data_ref::<UnitQuestLines>()
        .and_then(|m| m.0.get(&entry_of(guid)).cloned())
        .filter(|_| on && guid != 0)
        .unwrap_or_default();
    for block in blocks {
        append_line(lua, this, (block.title, TITLE), None, false)?;
        for line in block.lines {
            append_line(lua, this, (format!(" - {line}"), LINE), None, false)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Kobold Vermin's spawn guid `0xF130000006001234` carries entry 6 (`0xF130 | entry << 24`).
    #[test]
    fn the_entry_is_bits_24_to_52_of_the_guid() {
        assert_eq!(super::entry_of(0xF130_0000_0600_1234), 6);
        assert_eq!(super::entry_of(0xF130_0012_B004_00AF), 0x12B0);
        assert_eq!(super::entry_of(0x29), 0, "a player guid carries no entry");
    }
}
