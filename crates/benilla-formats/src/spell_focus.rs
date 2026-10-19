//! `SpellFocusObject.dbc`: the object a spell must be cast near (Anvil, Forge, …), named by the
//! crafting book's "Requires:" line from `Spell.dbc` `RequiresSpellFocus`. The server does the
//! proximity check (`Spell::CheckCast`); the client only names it.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, str_at, u32_at};
use crate::Chain;

const SPELL_FOCUS_OBJECT: &str = "DBFilesClient\\SpellFocusObject.dbc";
const SPELL_FOCUS_FIELDS: usize = 10;
const COL_NAME_ENUS: usize = 1;

/// `SpellFocusObject` id → its enUS name.
pub struct SpellFocusCatalog {
    names: HashMap<u32, String>,
}

impl SpellFocusCatalog {
    /// The name for a `RequiresSpellFocus` id; `None` for 0, which draws no requirement line.
    pub fn name(&self, focus_id: u32) -> Option<&str> {
        self.names.get(&focus_id).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

fn schema() -> Schema {
    let mut s = Schema::new("SpellFocusObject");
    for i in 0..SPELL_FOCUS_FIELDS {
        let ty = if i == COL_NAME_ENUS {
            FieldType::String
        } else {
            FieldType::UInt32
        };
        s.add_field(SchemaField::new(format!("F{i}"), ty));
    }
    s
}

/// Load SpellFocusObject.dbc from the patch chain into a [`SpellFocusCatalog`].
pub fn load_spell_focus_catalog(chain: &mut Chain) -> Result<SpellFocusCatalog> {
    let bytes = chain
        .read_file(SPELL_FOCUS_OBJECT)
        .with_context(|| format!("reading {SPELL_FOCUS_OBJECT}"))?;
    let rs = parse(&bytes, schema(), "SpellFocusObject.dbc")?;
    let mut names = HashMap::new();
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        if let Some(name) = str_at(&rs, r, COL_NAME_ENUS) {
            names.insert(id, name);
        }
    }
    Ok(SpellFocusCatalog { names })
}
