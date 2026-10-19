//! The item and enchant glow chain. An ItemVisuals id comes from the item's display
//! (`ItemDisplayInfo` column 22) or its enchant (`SpellItemEnchantment` field 22); its row holds
//! five `ItemVisualEffects` ids, each naming a glow model, and slot `i` hangs on the item model's
//! attachment `i` (`0x479700` calls `0x712f70(glow, item, i)`). The loaders check each table's
//! field count and record size (`0x548760`, `0x548530`, `0x54f6e0`).
//!
//! `0x479700` skips a visual or effect id that is negative, past the table's max id or a null
//! row, and an empty model path; ItemVisuals row 28 ships two garbage effect ids. The catalog
//! applies the same skips at load.

use std::collections::HashMap;

use crate::Chain;
use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, str_at, u32_at};

const ITEM_VISUALS: &str = "DBFilesClient\\ItemVisuals.dbc";
const ITEM_VISUAL_EFFECTS: &str = "DBFilesClient\\ItemVisualEffects.dbc";
const SPELL_ITEM_ENCHANTMENT: &str = "DBFilesClient\\SpellItemEnchantment.dbc";

/// An ItemVisuals row's effect slots, which are also the item model's attachment ids 0..4.
pub const ITEM_VISUAL_SLOTS: usize = 5;

/// ItemVisuals id to its five glow-model paths, raw `.mdx` as stored; `None` where the reference
/// skips the slot.
pub struct ItemVisualCatalog {
    visuals: HashMap<u32, [Option<String>; ITEM_VISUAL_SLOTS]>,
}

impl ItemVisualCatalog {
    /// The five slots for an ItemVisuals id, read signed as `0x479700` does, so `-1` and `0` name
    /// nothing.
    pub fn effects(&self, visual_id: i32) -> Option<&[Option<String>; ITEM_VISUAL_SLOTS]> {
        (visual_id > 0).then(|| self.visuals.get(&(visual_id as u32)))?
    }

    /// Build from an explicit map, for tests and fixtures.
    pub fn from_visuals(visuals: HashMap<u32, [Option<String>; ITEM_VISUAL_SLOTS]>) -> Self {
        ItemVisualCatalog { visuals }
    }

    pub fn len(&self) -> usize {
        self.visuals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.visuals.is_empty()
    }
}

/// `SpellItemEnchantment.dbc`'s visual, enUS name and `Flags`, one load for the glow chain, the
/// tooltip's enchant line and the bind confirms.
pub struct EnchantCatalog {
    visuals: HashMap<u32, i32>,
    names: HashMap<u32, String>,
    /// `Flags` for every row, 0 included, so its keys are the reference's `enchantTable[id] != 0`.
    flags: HashMap<u32, u32>,
}

/// `Flags & 0x1`: applying the enchant binds the item. It is the only gate on the bind confirm,
/// event 402 (`0x495d60`), and `0x5da2c0` reads it to ask whether an item already has one.
const FLAG_BINDS_THE_ITEM: u32 = 0x1;

/// `Flags & 0x2`: both tooltip enchant-line printers return before reading the name
/// (`0x6290e4`, `0x62923e`), though the replace confirm still reads it (`0x4960d0`). Set on the
/// totem imbues, Firestone and Orb of Fire.
const FLAG_TOOLTIP_HIDES_NAME: u32 = 0x2;

impl EnchantCatalog {
    /// The ItemVisuals id the enchant glows with; signed, as one shipped row carries `-1`.
    pub fn visual(&self, enchant_id: u32) -> Option<i32> {
        self.visuals.get(&enchant_id).copied()
    }

    /// The display name as stored (`"Agility +15"`, `"Crusader"`); `None` when empty.
    pub fn name(&self, enchant_id: u32) -> Option<&str> {
        self.names.get(&enchant_id).map(String::as_str)
    }

    /// Whether applying the enchant binds the item ([`FLAG_BINDS_THE_ITEM`]); `false` for an
    /// unknown id, as the reference skips a missing row.
    pub fn binds_the_item(&self, enchant_id: u32) -> bool {
        self.flag(enchant_id, FLAG_BINDS_THE_ITEM)
    }

    /// Whether the item tooltip prints no line for the enchant ([`FLAG_TOOLTIP_HIDES_NAME`]).
    pub fn tooltip_hides_name(&self, enchant_id: u32) -> bool {
        self.flag(enchant_id, FLAG_TOOLTIP_HIDES_NAME)
    }

    fn flag(&self, enchant_id: u32, bit: u32) -> bool {
        self.flags.get(&enchant_id).is_some_and(|f| f & bit != 0)
    }

    /// Whether the id names a row, the reference's null test after each `enchantTable[id]`; a
    /// missing row raises no confirm.
    pub fn has_row(&self, enchant_id: u32) -> bool {
        self.flags.contains_key(&enchant_id)
    }

    /// Build from explicit maps, for tests; `flags` is also the row set [`Self::has_row`] reads.
    pub fn from_rows(
        visuals: HashMap<u32, i32>,
        names: HashMap<u32, String>,
        flags: HashMap<u32, u32>,
    ) -> Self {
        EnchantCatalog {
            visuals,
            names,
            flags,
        }
    }

    /// `(enchant id, ItemVisuals id)` for the rows that carry one, in no order.
    pub fn iter_visuals(&self) -> impl Iterator<Item = (u32, i32)> + '_ {
        self.visuals.iter().map(|(k, v)| (*k, *v))
    }

    /// How many enchants carry a glow.
    pub fn visual_count(&self) -> usize {
        self.visuals.len()
    }

    /// How many enchants carry a name.
    pub fn name_count(&self) -> usize {
        self.names.len()
    }
}

/// `ItemVisuals.dbc`: 6 fields, 24-byte records.
pub(crate) fn item_visuals_schema() -> Schema {
    let mut s = Schema::new("ItemVisuals");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    for i in 0..ITEM_VISUAL_SLOTS {
        s.add_field(SchemaField::new(
            format!("Effect{i}"),
            FieldType::UInt32, // read signed, as the reference does
        ));
    }
    s
}

/// `ItemVisualEffects.dbc`: 2 fields, 8-byte records.
pub(crate) fn item_visual_effects_schema() -> Schema {
    let mut s = Schema::new("ItemVisualEffects");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("Model", FieldType::String));
    s
}

/// `SpellItemEnchantment.dbc`: 24 fields, 96-byte records. The reference reads the enUS name at
/// `+0x34` (field 13, `0x4960d0`), the visual at `+0x58` (field 22, `0x5d9be1`) and `Flags` at
/// `+0x5c` (field 23).
pub(crate) fn spell_item_enchantment_schema() -> Schema {
    let mut s = Schema::new("SpellItemEnchantment");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    for group in ["Effect", "EffectPointsMin", "EffectPointsMax", "EffectArg"] {
        for i in 0..3 {
            s.add_field(SchemaField::new(format!("{group}{i}"), FieldType::UInt32));
        }
    }
    for i in 0..8 {
        s.add_field(SchemaField::new(format!("Name{i}"), FieldType::String));
    }
    s.add_field(SchemaField::new("NameFlags", FieldType::UInt32));
    s.add_field(SchemaField::new("ItemVisual", FieldType::UInt32));
    s.add_field(SchemaField::new("Flags", FieldType::UInt32));
    s
}

/// Load and join `ItemVisuals.dbc` and `ItemVisualEffects.dbc`, applying the reference's skips.
pub fn load_item_visual_catalog(chain: &mut Chain) -> Result<ItemVisualCatalog> {
    let bytes = chain
        .read_file(ITEM_VISUAL_EFFECTS)
        .with_context(|| format!("reading {ITEM_VISUAL_EFFECTS}"))?;
    let rs = parse(&bytes, item_visual_effects_schema(), "ItemVisualEffects")?;
    // `str_at` drops an empty path, as the reference skips one.
    let mut effects: HashMap<u32, String> = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let (Some(id), Some(model)) = (u32_at(r, 0), str_at(&rs, r, 1)) else {
            continue;
        };
        effects.insert(id, model);
    }

    let bytes = chain
        .read_file(ITEM_VISUALS)
        .with_context(|| format!("reading {ITEM_VISUALS}"))?;
    let rs = parse(&bytes, item_visuals_schema(), "ItemVisuals")?;
    let mut visuals = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let slots = std::array::from_fn(|i| {
            let raw = u32_at(r, 1 + i).unwrap_or(0) as i32;
            (raw > 0)
                .then(|| effects.get(&(raw as u32)).cloned())
                .flatten()
        });
        visuals.insert(id, slots);
    }
    Ok(ItemVisualCatalog { visuals })
}

/// Load the [`EnchantCatalog`] from `SpellItemEnchantment.dbc`.
pub fn load_enchant_catalog(chain: &mut Chain) -> Result<EnchantCatalog> {
    let bytes = chain
        .read_file(SPELL_ITEM_ENCHANTMENT)
        .with_context(|| format!("reading {SPELL_ITEM_ENCHANTMENT}"))?;
    let rs = parse(
        &bytes,
        spell_item_enchantment_schema(),
        "SpellItemEnchantment",
    )?;
    let mut visuals = HashMap::new();
    let mut names = HashMap::new();
    let mut flags = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        // Every row, `Flags == 0` included: the keys are the row set.
        flags.insert(id, u32_at(r, 23).unwrap_or(0));
        let visual = u32_at(r, 22).unwrap_or(0) as i32;
        if visual != 0 {
            visuals.insert(id, visual);
        }
        // `str_at` drops an empty name.
        if let Some(name) = str_at(&rs, r, 13) {
            names.insert(id, name);
        }
    }
    Ok(EnchantCatalog {
        visuals,
        names,
        flags,
    })
}
