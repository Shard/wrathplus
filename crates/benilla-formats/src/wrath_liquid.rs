//! Lossless build-12340 liquid metadata, not liquid rendering policy.
//! Header loaders 0x6474a0/0x647700 and decoders 0x8b2720/0x8b2aa0
//! establish 45/3 direct words. Consumers 0x7c5690/0x793d20 join type
//! word 14 to material ID, then read material word 1 as a render selector.
use crate::{Chain, DataProfile};
use anyhow::{ensure, Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiquidType {
    raw: [u32; 45],
    pub name: String,
    /// Six native string references; no filename/animation policy is inferred.
    pub strings: [String; 6],
}
impl LiquidType {
    pub fn id(&self) -> u32 {
        self.raw[0]
    }
    pub fn flags(&self) -> u32 {
        self.raw[2]
    }
    pub fn material_id(&self) -> u32 {
        self.raw[14]
    }
    pub fn raw_columns(&self) -> &[u32; 45] {
        &self.raw
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiquidMaterial {
    raw: [u32; 3],
}
impl LiquidMaterial {
    pub fn id(&self) -> u32 {
        self.raw[0]
    }
    /// Native render selector, not a water/lava enum or MH2O vertex format.
    pub fn raw_render_selector(&self) -> u32 {
        self.raw[1]
    }
    pub fn raw_columns(&self) -> &[u32; 3] {
        &self.raw
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiquidCatalogs {
    types: Vec<LiquidType>,
    materials: Vec<LiquidMaterial>,
}
impl LiquidCatalogs {
    /// Source order, including duplicate IDs. ID lookups are last-row-wins.
    pub fn types(&self) -> &[LiquidType] {
        &self.types
    }
    pub fn materials(&self) -> &[LiquidMaterial] {
        &self.materials
    }
    pub fn liquid_type(&self, id: u32) -> Option<&LiquidType> {
        self.types.iter().rev().find(|r| r.id() == id)
    }
    pub fn material(&self, id: u32) -> Option<&LiquidMaterial> {
        self.materials.iter().rev().find(|r| r.id() == id)
    }
    /// Missing joins stay missing; no synthesized/default water material.
    pub fn material_for(&self, liquid: &LiquidType) -> Option<&LiquidMaterial> {
        self.material(liquid.material_id())
    }
}

fn records<const N: usize>(bytes: &[u8]) -> Result<(Vec<[u32; N]>, &[u8])> {
    ensure!(
        bytes.len() >= 20 && &bytes[..4] == b"WDBC",
        "invalid WDBC header"
    );
    let word = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    ensure!(
        word(8) as usize == N && word(12) as usize == N * 4,
        "wrong liquid DBC shape"
    );
    let end = (word(4) as usize)
        .checked_mul(N * 4)
        .and_then(|n| n.checked_add(20))
        .context("WDBC size overflow")?;
    ensure!(
        end.checked_add(word(16) as usize) == Some(bytes.len()),
        "WDBC size mismatch"
    );
    let rows = bytes[20..end]
        .chunks_exact(N * 4)
        .map(|r| {
            std::array::from_fn(|i| u32::from_le_bytes(r[i * 4..i * 4 + 4].try_into().unwrap()))
        })
        .collect();
    Ok((rows, &bytes[end..]))
}
fn text(strings: &[u8], offset: u32) -> Result<String> {
    let tail = strings
        .get(offset as usize..)
        .context("liquid string offset out of bounds")?;
    let end = tail
        .iter()
        .position(|&b| b == 0)
        .context("unterminated liquid string")?;
    Ok(std::str::from_utf8(&tail[..end])
        .context("invalid liquid UTF-8")?
        .to_owned())
}

pub fn parse_liquid_catalogs(types: &[u8], materials: &[u8]) -> Result<LiquidCatalogs> {
    let (type_rows, strings) = records::<45>(types).context("LiquidType")?;
    let mut types = Vec::with_capacity(type_rows.len());
    for raw in type_rows {
        let name = text(strings, raw[1]).context("LiquidType name")?;
        let mut values = Vec::with_capacity(6);
        for &offset in &raw[15..21] {
            values.push(text(strings, offset).context("LiquidType strings")?);
        }
        types.push(LiquidType {
            raw,
            name,
            strings: values.try_into().unwrap(),
        });
    }
    let (rows, _) = records::<3>(materials).context("LiquidMaterial")?;
    let materials = rows.into_iter().map(|raw| LiquidMaterial { raw }).collect();
    Ok(LiquidCatalogs { types, materials })
}

/// Uses only the boot-resolved chain. Required read/decode errors propagate.
pub fn load_liquid_catalogs(chain: &mut Chain) -> Result<LiquidCatalogs> {
    ensure!(
        matches!(chain.profile(), DataProfile::Wrath { .. }),
        "liquid catalogs require Wrath data"
    );
    let types = chain
        .read_file("DBFilesClient\\LiquidType.dbc")
        .context("reading LiquidType")?;
    let materials = chain
        .read_file("DBFilesClient\\LiquidMaterial.dbc")
        .context("reading LiquidMaterial")?;
    parse_liquid_catalogs(&types, &materials)
}

#[cfg(test)]
mod tests;
