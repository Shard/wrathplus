//! `GameTips.dbc`: the loading-screen tips, each with its own `|cffffd100Tip:|r` prefix and a
//! trailing `\r\n` in the data; FrameXML never reads them. The reference loads them in file order
//! into one array (`[0xc0dcd0]`, count `[0xc0dcd4]`, loader `0x545f90`), and
//! `CGlueMgr::EnterWorld` (`0x46b500`) stores the chosen record index, never an id, in the
//! `gameTip` CVar. Record order is not id order: the file opens on id 396.

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::chain::Chain;
use crate::dbc::{parse, str_at};
use crate::DataProfile;

const GAME_TIPS: &str = "DBFilesClient\\GameTips.dbc";

/// The tips in file record order, the order the persisted `gameTip` index counts through.
#[derive(Clone, Debug, Default)]
pub struct GameTipsCatalog {
    tips: Vec<String>,
}

impl GameTipsCatalog {
    /// A catalog over given tips, for a test without an install.
    pub fn from_tips(tips: Vec<String>) -> Self {
        GameTipsCatalog { tips }
    }

    /// The tip at a record index; an index saved under another chain can fall past the table.
    pub fn get(&self, index: usize) -> Option<&str> {
        self.tips.get(index).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.tips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tips.is_empty()
    }
}

// GameTipsRec is ID + langstringref. Build 12340 has 16 locale offsets and
// a mask (18 fields total).
// References: https://wowdev.wiki/DB/GameTips and https://wowdev.wiki/Localization.
fn text_layout(profile: &DataProfile) -> (usize, usize) {
    match profile {
        DataProfile::Wrath { locale } => {
            let slot = match locale.as_str() {
                "koKR" => 1,
                "frFR" => 2,
                "deDE" => 3,
                "zhCN" => 4,
                "zhTW" => 5,
                "esES" => 6,
                "esMX" => 7,
                "ruRU" => 8,
                _ => 0, // enUS/enGB share the first locale slot.
            };
            (16, slot)
        }
    }
}

fn schema(profile: &DataProfile) -> Schema {
    let mut s = Schema::new("GameTips");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new_array(
        "Text",
        FieldType::String,
        text_layout(profile).0,
    ));
    s.add_field(SchemaField::new("TextMask", FieldType::UInt32));
    s.set_key_field("ID");
    s
}

pub fn load_game_tips(chain: &mut Chain) -> Result<GameTipsCatalog> {
    let bytes = chain.read_file(GAME_TIPS).context("reading GameTips.dbc")?;
    parse_tips(&bytes, chain.profile())
}

fn parse_tips(bytes: &[u8], profile: &DataProfile) -> Result<GameTipsCatalog> {
    let rs = parse(bytes, schema(profile), "GameTips")?;
    let text_column = 1 + text_layout(profile).1;
    let mut tips = Vec::with_capacity(rs.records().len());
    for r in rs.records() {
        // A row with an empty text slot is dropped, not shown blank; the shipped file has none.
        match str_at(&rs, r, text_column) {
            Some(text) if !text.trim().is_empty() => tips.push(text),
            _ => {}
        }
    }
    Ok(GameTipsCatalog { tips })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(rows: &[Vec<u32>], strings: &[u8]) -> Vec<u8> {
        let fields = rows[0].len() as u32;
        let mut bytes = b"WDBC".to_vec();
        for word in [rows.len() as u32, fields, fields * 4, strings.len() as u32] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        for row in rows {
            for word in row {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
        }
        bytes.extend_from_slice(strings);
        bytes
    }

    #[test]
    fn vanilla_and_wrath_tip_layouts_preserve_file_order_and_locale() {
        let strings = b"\0English\0Francais\0Second\0";
        let mut first = vec![0; 18];
        first[0] = 99;
        first[1] = 1;
        first[3] = 9;
        let mut second = first.clone();
        second[0] = 1;
        second[3] = 18;
        let bytes = fixture(&[first, second], strings);
        let french = DataProfile::Wrath {
            locale: "frFR".into(),
        };
        let cat = parse_tips(&bytes, &french).unwrap();
        assert_eq!(cat.get(0), Some("Francais"));
        assert_eq!(cat.get(1), Some("Second"));
        let spanish = DataProfile::Wrath {
            locale: "esES".into(),
        };
        assert!(
            parse_tips(&bytes, &spanish).unwrap().is_empty(),
            "no fabricated English fallback for empty locale"
        );

        let short = vec![5, 1, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(parse_tips(&fixture(&[short], strings), &french).is_err());
    }

    #[test]
    fn the_real_table_is_seventy_four_tips_in_file_order() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_game_tips(&mut chain).expect("load GameTips");
        assert_eq!(cat.len(), 74, "the 5875 enGB chain ships 74 tips");
        assert!(
            cat.get(0).unwrap().starts_with("|cffffd100"),
            "the colour prefix is authored INTO the data, not added by the renderer: {:?}",
            cat.get(0)
        );
        assert!(
            cat.get(0).unwrap().contains("mini-map"),
            "record order, not id order — record 0 is id 396: {:?}",
            cat.get(0)
        );
        assert!(
            cat.get(74).is_none(),
            "one past the end is None, not a panic"
        );
    }
}
