//! `GMTicketCategory.dbc`, the ten categories of the Help window's GM list, which
//! `GetGMTicketCategories()` pushes as `(id, name)` pairs in file order; the `TICKET_TYPE1..4`
//! strings feed the OpenTicket dropdown, which 1.12 never shows. The id is the wire value: the
//! clicked button stores it as `HelpFrameOpenTicket.ticketType` and `NewGMTicket` sends it as
//! `CMSG_GMTICKET_CREATE`'s category, so the file's ids are kept, never renumbered.

use anyhow::{Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, str_at, u32_at};
use crate::Chain;

const GM_TICKET_CATEGORY: &str = "DBFilesClient\\GMTicketCategory.dbc";

/// `GMTicketCategory.dbc` in file order, the order `GetGMTicketCategories()` pushes.
pub struct GmTicketCategoryCatalog {
    categories: Vec<GmTicketCategory>,
}

/// One row: the wire category id and its localized display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmTicketCategory {
    /// The id `CMSG_GMTICKET_CREATE` carries.
    pub id: u32,
    /// The button label, from the enUS `Name_Lang` slot.
    pub name: String,
}

impl GmTicketCategoryCatalog {
    /// The rows, in file order.
    pub fn categories(&self) -> &[GmTicketCategory] {
        &self.categories
    }

    pub fn len(&self) -> usize {
        self.categories.len()
    }

    pub fn is_empty(&self) -> bool {
        self.categories.is_empty()
    }
}

fn gm_ticket_category_schema() -> Schema {
    let mut s = Schema::new("GMTicketCategory");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    for i in 0..8 {
        s.add_field(SchemaField::new(format!("Name{i}"), FieldType::String));
    }
    s.add_field(SchemaField::new("NameFlags", FieldType::UInt32));
    s
}

/// Load `GMTicketCategory.dbc` from the patch chain. A row with an empty name is skipped, as it
/// would paint a dead button; 5875 has none.
pub fn load_gm_ticket_categories(chain: &mut Chain) -> Result<GmTicketCategoryCatalog> {
    let bytes = chain
        .read_file(GM_TICKET_CATEGORY)
        .with_context(|| format!("reading {GM_TICKET_CATEGORY}"))?;
    let rs = parse(&bytes, gm_ticket_category_schema(), "GMTicketCategory")?;
    let mut categories = Vec::with_capacity(rs.records().len());
    for r in rs.records() {
        let (Some(id), Some(name)) = (u32_at(r, 0), str_at(&rs, r, 1).filter(|n| !n.is_empty()))
        else {
            continue;
        };
        categories.push(GmTicketCategory { id, name });
    }
    Ok(GmTicketCategoryCatalog { categories })
}
