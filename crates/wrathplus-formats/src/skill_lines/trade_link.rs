//! Build 12340's trade link, `|cffffd000|Htrade:<spell>:<rank>:<max>:<guid>:<mask>|h[<name>]|h|r`
//! (`GetTradeSkillListLink`, `0x5db060`), and its reading when a click opens the linked window
//! (`0x5de120`). The guid is `%I64X`. The mask is one bit per `SkillLineAbility` row of the line,
//! in the line's row order, six bits to a base64 character, low bit first; a bit is set when the
//! linker knows the row's spell and the row's masks admit the linker's race and class.
//!
//! Labelled: the link-click parser (`0x80cce0`) is not decompiled; [`parse_trade_link`] reads the
//! fields the builder writes. A character outside the base64 alphabet reads as no bits.

use super::LineAbility;

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// A trade link's fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TradeLink {
    /// The profession opener (effect 47).
    pub spell_id: u32,
    pub rank: u32,
    pub max_rank: u32,
    pub guid: u64,
    pub mask: String,
}

/// `0x810320` with no inversion: a zero mask admits all, else the 1-based race and class bits.
pub fn race_class_admits(race: u8, class: u8, race_mask: u32, class_mask: u32) -> bool {
    let bit = |n: u8| 1u32 << (u32::from(n).wrapping_sub(1) & 0x1f);
    (race_mask == 0 || race_mask & bit(race) != 0)
        && (class_mask == 0 || class_mask & bit(class) != 0)
}

/// The mask characters a line needs: one per six rows, rounded up.
pub fn trade_mask_len(rows: usize) -> usize {
    rows.div_ceil(6)
}

/// The linker's mask over `rows` (`0x5db060`).
pub fn encode_trade_mask(
    rows: &[LineAbility],
    known: impl Fn(u32) -> bool,
    race: u8,
    class: u8,
) -> String {
    rows.chunks(6)
        .map(|chunk| {
            let bits = chunk.iter().enumerate().fold(0u8, |acc, (i, row)| {
                if known(row.spell_id)
                    && race_class_admits(race, class, row.race_mask, row.class_mask)
                {
                    acc | 1 << i
                } else {
                    acc
                }
            });
            char::from(BASE64[usize::from(bits)])
        })
        .collect()
}

/// The spells a mask marks, in row order (`0x5dd5a0`); `None` when its length is not the line's
/// (`0x5de120` then opens nothing).
pub fn decode_trade_mask(rows: &[LineAbility], mask: &str) -> Option<Vec<u32>> {
    if mask.len() != trade_mask_len(rows.len()) {
        return None;
    }
    let bytes = mask.as_bytes();
    Some(
        rows.iter()
            .enumerate()
            .filter(|(i, _)| {
                let value = BASE64.iter().position(|&c| c == bytes[i / 6]).unwrap_or(0);
                value & (1 << (i % 6)) != 0
            })
            .map(|(_, row)| row.spell_id)
            .collect(),
    )
}

/// The link `GetTradeSkillListLink` returns; `name` is the opener spell's.
pub fn trade_link(link: &TradeLink, name: &str) -> String {
    format!(
        "|cffffd000|Htrade:{}:{}:{}:{:X}:{}|h[{}]|h|r",
        link.spell_id, link.rank, link.max_rank, link.guid, link.mask, name
    )
}

/// A link's fields, from a bare `trade:…` or an escaped `|Htrade:…|h[…]|h` link.
pub fn parse_trade_link(link: &str) -> Option<TradeLink> {
    let start = link.find("trade:")? + "trade:".len();
    let body = &link[start..];
    let body = body.split('|').next().unwrap_or(body);
    let mut fields = body.splitn(5, ':');
    let spell_id = fields.next()?.parse().ok()?;
    let rank = fields.next()?.parse().ok()?;
    let max_rank = fields.next()?.parse().ok()?;
    let guid = u64::from_str_radix(fields.next()?, 16).ok()?;
    let mask = fields.next().unwrap_or("").to_string();
    Some(TradeLink {
        spell_id,
        rank,
        max_rank,
        guid,
        mask,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(spell_id: u32, race_mask: u32, class_mask: u32) -> LineAbility {
        LineAbility {
            spell_id,
            race_mask,
            class_mask,
        }
    }

    #[test]
    fn the_mask_packs_six_rows_a_character_low_bit_first() {
        let rows: Vec<_> = (1..=8).map(|s| row(s, 0, 0)).collect();
        // Rows 1, 2 and 7 known: 0b000011 = 'D', then 0b000001 = 'B'.
        let mask = encode_trade_mask(&rows, |s| matches!(s, 1 | 2 | 7), 1, 1);
        assert_eq!(mask, "DB");
        assert_eq!(decode_trade_mask(&rows, &mask), Some(vec![1, 2, 7]));
        assert_eq!(
            decode_trade_mask(&rows, "D"),
            None,
            "a short mask opens nothing"
        );
    }

    #[test]
    fn a_row_the_linker_race_or_class_cannot_take_stays_clear() {
        // Race 2 (Orc) is bit 1; class 8 (Mage) is bit 7.
        let rows = [row(10, 0b10, 0), row(11, 0b01, 0), row(12, 0, 1 << 7)];
        assert_eq!(encode_trade_mask(&rows, |_| true, 2, 8), "F");
        assert!(race_class_admits(2, 8, 0, 0));
        assert!(!race_class_admits(2, 1, 0, 1 << 7));
    }

    #[test]
    fn the_link_round_trips_through_its_text() {
        let link = TradeLink {
            spell_id: 51304,
            rank: 450,
            max_rank: 450,
            guid: 0x0000_0000_0012_ABCD,
            mask: "D/".into(),
        };
        let text = trade_link(&link, "Alchemy");
        assert_eq!(
            text,
            "|cffffd000|Htrade:51304:450:450:12ABCD:D/|h[Alchemy]|h|r"
        );
        assert_eq!(parse_trade_link(&text), Some(link.clone()));
        assert_eq!(
            parse_trade_link("trade:51304:450:450:12ABCD:D/"),
            Some(link)
        );
        assert_eq!(parse_trade_link("item:25"), None);
    }
}
