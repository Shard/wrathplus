//! `SpellChainEffects.dbc`: the beams of Chain Lightning, Drain Life, Mind Flay, Chain Heal. A kit
//! reaches it only through a CharProc slot of type 0 or 12, both of which the dispatcher
//! (`0x60d7c0`, table `0x60dc20`) routes to one case (`0x60da79`) and on to `CreateChainVisual`
//! (`0x6ecbd0`). Ids skip 14 and 16, so an id is a lookup, never an index.
//! `wrathplus-extract <Data> chaincensus` prints the table with every kit that draws a beam.

use std::collections::HashMap;

use anyhow::{ensure, Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, parse, str_at, u32_at};
use crate::Chain;

const SPELL_CHAIN_EFFECTS: &str = "DBFilesClient\\SpellChainEffects.dbc";
const SPELL_CHAIN_EFFECTS_FIELDS: usize = 8;

/// The client's clamp on a chain proc's beam count (`0x6ecbd0`); only kit 6397, Chain Burn, asks
/// for more than one.
pub const CHAIN_MAX_BEAMS: u32 = 3;

/// One row: the shape and animation of one beam. Fields 4-6 are not what their community names
/// (`TexCoordScale`, `SegDuration`, `SegDelay`) say.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainEffect {
    /// Field 1: the target segment length; a hop gets `trunc(length / this + 2.0)` segments, so
    /// never fewer than two (`0x7af713`).
    pub avg_seg_len: f32,
    /// Field 2: the half-width; the ribbon spans twice this in yards.
    pub half_width: f32,
    /// Field 3: the jitter amplitude as a fraction of the beam's length, re-rolled every frame
    /// (`0x7b0950`) and blended 0.75 old to 0.25 new; 0.01 on most rows, 0.04 on Chain Lightning's.
    pub noise_scale: f32,
    /// Field 4: the texture's scroll period in seconds, `u = -(phase / this)` with `phase`
    /// advancing by `dt` modulo this (`0x7af9d7`). A negative period reverses the scroll: four
    /// rows, the drains among them, ship -0.5, flowing back toward the caster.
    pub scroll_period_s: f32,
    /// Field 5: how long one hop burns in ms (`0x6ec9eb`), and so the beam's expiry,
    /// `now + hops × this` (`0x6ecd30`); a channel beam ignores it and lives until swept.
    pub bolt_life_ms: u32,
    /// Field 6: the stagger between hops in ms, hop `i` lighting at `t0 + i × this` (`0x6ec9da`),
    /// so a 3-hop cast arcs outward; a channel beam bypasses it.
    pub bolt_stagger_ms: u32,
    /// Field 7: the texture path as stored (`Textures\SpellChainEffects\*.blp`).
    pub texture: String,
}

/// A kit's chain CharProc, its params read through [`super::char_proc_small_int`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainProc {
    /// `CharParamZero`, the [`ChainEffect`] id; never 0, as the client no-ops an id with no row
    /// (`0x6ecc0e`).
    pub effect_id: u32,
    /// `CharParamOne`, clamped to [`CHAIN_MAX_BEAMS`]; the client draws nothing for 0.
    pub beams: u32,
    /// `CharParamTwo` as a bool (`setne`): set on channel-stage kits, clear on cast-stage ones, and
    /// the only real difference between the two chain types.
    pub flag: bool,
    /// The stored type, 0 or 12; behaviour keys off [`Self::flag`], as the dispatcher never forks.
    pub ty: i32,
}

fn chain_effects_schema() -> Schema {
    let mut s = Schema::new("SpellChainEffects");
    let mut add = |name: &str, ty| s.add_field(SchemaField::new(name.to_string(), ty));
    add("ID", FieldType::UInt32);
    add("AvgSegLen", FieldType::Float32);
    add("HalfWidth", FieldType::Float32);
    add("NoiseScale", FieldType::Float32);
    add("ScrollPeriod", FieldType::Float32);
    add("BoltLife", FieldType::UInt32);
    add("BoltStagger", FieldType::UInt32);
    add("Texture", FieldType::String);
    debug_assert_eq!(s.fields.len(), SPELL_CHAIN_EFFECTS_FIELDS);
    s
}

/// Read `SpellChainEffects.dbc`; a textureless row stays, as the client's constructor never reads
/// the texture.
pub(super) struct LoadedChains {
    pub effects: HashMap<u32, ChainEffect>,
    pub raw: HashMap<u32, Vec<u8>>,
}

pub(super) fn load(chain: &mut Chain) -> Result<LoadedChains> {
    let bytes = chain
        .read_file(SPELL_CHAIN_EFFECTS)
        .with_context(|| format!("reading {SPELL_CHAIN_EFFECTS}"))?;
    if matches!(chain.profile(), crate::DataProfile::Wrath { .. }) {
        return wrath_packed(&bytes);
    }
    let set = parse(&bytes, chain_effects_schema(), "SpellChainEffects.dbc")?;
    let mut rows = HashMap::with_capacity(set.records().len());
    for r in set.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        rows.insert(
            id,
            ChainEffect {
                avg_seg_len: f32_at(r, 1).unwrap_or(0.0),
                half_width: f32_at(r, 2).unwrap_or(0.0),
                noise_scale: f32_at(r, 3).unwrap_or(0.0),
                scroll_period_s: f32_at(r, 4).unwrap_or(0.0),
                bolt_life_ms: u32_at(r, 5).unwrap_or(0),
                bolt_stagger_ms: u32_at(r, 6).unwrap_or(0),
                texture: str_at(&set, r, 7).unwrap_or_default(),
            },
        );
    }
    Ok(LoadedChains {
        effects: rows,
        raw: HashMap::new(),
    })
}

/// WoWDBDefs build 12340: 48 logical fields, 177 bytes, NOT 48 four-byte words.
/// Fields 0..38 are words; Alpha/R/G/B/BlendMode are bytes 156..160, then four
/// unaligned words at 161..176. Only the original eight fields feed our renderer;
/// keep the entire packed row verbatim instead of inventing meanings for extensions.
fn wrath_packed(bytes: &[u8]) -> Result<LoadedChains> {
    const SIZE: usize = 177;
    ensure!(
        bytes.len() >= 20 && &bytes[..4] == b"WDBC",
        "invalid SpellChainEffects header"
    );
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    ensure!(
        word(8) == 48 && word(12) == SIZE as u32,
        "expected build-12340 SpellChainEffects 48 fields/177 bytes"
    );
    let count = word(4) as usize;
    let strings_at = count
        .checked_mul(SIZE)
        .and_then(|v| v.checked_add(20))
        .context("SpellChainEffects record size overflow")?;
    let end = strings_at
        .checked_add(word(16) as usize)
        .context("SpellChainEffects string size overflow")?;
    ensure!(
        end == bytes.len(),
        "truncated or trailing SpellChainEffects data"
    );
    let strings = &bytes[strings_at..end];
    let mut loaded = LoadedChains {
        effects: HashMap::with_capacity(count),
        raw: HashMap::with_capacity(count),
    };
    for row in bytes[20..strings_at].as_chunks::<SIZE>().0 {
        let w = |at: usize| u32::from_le_bytes(row[at..at + 4].try_into().unwrap());
        let f = |at| f32::from_bits(w(at));
        let offset = w(28) as usize;
        let tail = strings
            .get(offset..)
            .context("SpellChainEffects texture offset outside string block")?;
        let len = tail
            .iter()
            .position(|&b| b == 0)
            .context("unterminated SpellChainEffects texture")?;
        let texture = std::str::from_utf8(&tail[..len])
            .context("invalid SpellChainEffects texture UTF-8")?
            .to_owned();
        let id = w(0);
        ensure!(
            !loaded.effects.contains_key(&id),
            "duplicate SpellChainEffects id {id}"
        );
        loaded.effects.insert(
            id,
            ChainEffect {
                avg_seg_len: f(4),
                half_width: f(8),
                noise_scale: f(12),
                scroll_period_s: f(16),
                bolt_life_ms: w(20),
                bolt_stagger_ms: w(24),
                texture,
            },
        );
        loaded.raw.insert(id, row.to_vec());
    }
    Ok(loaded)
}

#[cfg(test)]
mod native_tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut bytes = b"WDBC".to_vec();
        for value in [2u32, 48, 177, 5] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for id in [8u32, 1] {
            let mut row = [0xa5u8; 177];
            for (i, value) in [
                id,
                2.78f32.to_bits(),
                0.5f32.to_bits(),
                0.04f32.to_bits(),
                (-0.5f32).to_bits(),
                1000,
                300,
                1,
            ]
            .into_iter()
            .enumerate()
            {
                row[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
            row[156..161].copy_from_slice(&[11, 22, 33, 44, 55]);
            row[161..165].copy_from_slice(&17.5f32.to_le_bytes());
            bytes.extend_from_slice(&row);
        }
        bytes.extend_from_slice(b"\0abc\0");
        bytes
    }
    #[test]
    fn packed_stride_and_raw_extensions_are_lossless() {
        let loaded = wrath_packed(&fixture()).unwrap();
        assert_eq!(loaded.effects.len(), 2);
        let row = &loaded.effects[&1];
        assert_eq!(
            (
                row.avg_seg_len,
                row.half_width,
                row.noise_scale,
                row.scroll_period_s
            ),
            (2.78, 0.5, 0.04, -0.5)
        );
        assert_eq!(
            (row.bolt_life_ms, row.bolt_stagger_ms, row.texture.as_str()),
            (1000, 300, "abc")
        );
        assert_eq!(&loaded.raw[&8][156..161], &[11, 22, 33, 44, 55]);
        assert_eq!(&loaded.raw[&1][161..165], &17.5f32.to_le_bytes());
        assert_eq!(&loaded.raw[&1][165..], &[0xa5; 12]);
    }
    #[test]
    fn bad_native_shapes_offsets_and_truncation_fail() {
        let b = fixture();
        for len in [0, 19, 20, b.len() - 1] {
            assert!(wrath_packed(&b[..len]).is_err());
        }
        let mut bad = b.clone();
        bad[12..16].copy_from_slice(&192u32.to_le_bytes());
        assert!(wrath_packed(&bad).is_err());
        let mut bad = b.clone();
        bad[48..52].copy_from_slice(&5u32.to_le_bytes());
        assert!(wrath_packed(&bad).is_err());
        let mut bad = b.clone();
        *bad.last_mut().unwrap() = b'x';
        assert!(wrath_packed(&bad).is_err());
        let mut bad = b;
        bad[197..201].copy_from_slice(&8u32.to_le_bytes());
        assert!(wrath_packed(&bad).is_err());
    }
}
