//! Native build-12340 ItemLimitCategory metadata, not inventory/roll policy.
//! Loader 0x6451c0 checks 20 columns/80 bytes; raw reader 0x8ba2d0 maps
//! ID 0, localized name 1..16/mask 17, quantity 18, flags 19 to runtime
//! +0/+4/+8/+12. Roll consumer 0x754dd0 reads quantity/name, not flags.
use crate::{Chain, DataProfile};
use anyhow::{ensure, Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemLimitCategory {
    raw: [u32; 20],
    /// Selected archive locale only; empty is not replaced with another language.
    pub name: String,
}
impl ItemLimitCategory {
    pub fn id(&self) -> u32 {
        self.raw[0]
    }
    pub fn quantity(&self) -> u32 {
        self.raw[18]
    }
    /// Uninterpreted native word; no inventory/equipped-mode policy here.
    pub fn flags(&self) -> u32 {
        self.raw[19]
    }
    pub fn raw_columns(&self) -> &[u32; 20] {
        &self.raw
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemLimitCategoryCatalog(Vec<ItemLimitCategory>);
impl ItemLimitCategoryCatalog {
    /// Source rows, including duplicate IDs. Native indexed lookup is last-row-wins.
    pub fn rows(&self) -> &[ItemLimitCategory] {
        &self.0
    }
    pub fn get(&self, id: u32) -> Option<&ItemLimitCategory> {
        self.0.iter().rev().find(|row| row.id() == id)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn text(strings: &[u8], offset: u32) -> Result<String> {
    let tail = strings
        .get(offset as usize..)
        .context("string offset out of bounds")?;
    let end = tail
        .iter()
        .position(|&b| b == 0)
        .context("unterminated DBC string")?;
    Ok(std::str::from_utf8(&tail[..end])
        .context("invalid DBC UTF-8")?
        .to_owned())
}

pub fn parse_item_limit_categories(bytes: &[u8], locale: &str) -> Result<ItemLimitCategoryCatalog> {
    let slot = crate::cfg_categories::locale_slot(locale)?;
    ensure!(
        bytes.len() >= 20 && &bytes[..4] == b"WDBC",
        "invalid WDBC header"
    );
    let word = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    ensure!(
        word(8) == 20 && word(12) == 80,
        "wrong ItemLimitCategory shape"
    );
    let end = (word(4) as usize)
        .checked_mul(80)
        .and_then(|n| n.checked_add(20))
        .context("WDBC size overflow")?;
    ensure!(
        end.checked_add(word(16) as usize) == Some(bytes.len()),
        "WDBC size mismatch"
    );
    let strings = &bytes[end..];
    let mut rows = Vec::new();
    for record in bytes[20..end].chunks_exact(80) {
        let raw = std::array::from_fn(|i| {
            u32::from_le_bytes(record[i * 4..i * 4 + 4].try_into().unwrap())
        });
        // Invalid inactive-locale offsets must not silently survive a winning override.
        for offset in &raw[1..17] {
            text(strings, *offset)?;
        }
        rows.push(ItemLimitCategory {
            name: text(strings, raw[1 + slot])?,
            raw,
        });
    }
    Ok(ItemLimitCategoryCatalog(rows))
}

/// Uses the boot-resolved chain only. Required read/decode failures propagate.
pub fn load_item_limit_categories(chain: &mut Chain) -> Result<ItemLimitCategoryCatalog> {
    let DataProfile::Wrath { locale } = chain.profile();
    let locale = locale.clone();
    let bytes = chain
        .read_file("DBFilesClient\\ItemLimitCategory.dbc")
        .context("reading ItemLimitCategory")?;
    parse_item_limit_categories(&bytes, &locale).context("ItemLimitCategory")
}

#[cfg(test)]
mod tests;
