//! `ChatProfanity.dbc` and `SpamMessages.dbc`, the lists behind 1.12's `profanityFilter` and
//! `spamFilter`: an id and a plain-string pattern, one table for every language. The reference
//! compiles each row once at startup as a PCRE (`0x402b7f` → `0x6c91a0`; `0x71fba0` →
//! `pcre_compile` `0x720250`, CASELESS | UTF8 | NO_UTF8_CHECK) and skips one that fails, which no
//! shipped row does; `benilla_app::text_filter` compiles and applies them. Both come through the
//! patch chain: `dbc.MPQ` has 1512 `ChatProfanity` rows to `patch.MPQ`'s 2289 and no
//! `SpamMessages`, and the names carry no locale (`0x858348`, `0x859a84`), so a locale build
//! overrides them at the archive.

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::chain::Chain;
use crate::dbc::{parse, str_at, u32_at};

const CHAT_PROFANITY: &str = "DBFilesClient\\ChatProfanity.dbc";
const SPAM_MESSAGES: &str = "DBFilesClient\\SpamMessages.dbc";

/// One row: its id, which a failed compile names as the reference's error does, and its raw
/// PCRE pattern.
#[derive(Clone, Debug)]
pub struct FilterPattern {
    pub id: u32,
    pub pattern: String,
}

fn schema(name: &str) -> Schema {
    let mut s = Schema::new(name);
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("Pattern", FieldType::String));
    s.set_key_field("ID");
    s
}

fn load(chain: &mut Chain, path: &str, name: &str) -> Result<Vec<FilterPattern>> {
    let bytes = chain
        .read_file(path)
        .with_context(|| format!("reading {name}.dbc"))?;
    let wrath_language = if name == "ChatProfanity" {
        let crate::DataProfile::Wrath { locale } = chain.profile();
        // Language is a zero-based client locale slot, not a spoken-language enum.
        // Installed build-12340: English=0, Korean=1. Never apply foreign lists.
        Some(crate::cfg_categories::locale_slot(locale)? as u32)
    } else {
        None
    };
    patterns(&bytes, name, wrath_language)
}

fn patterns(bytes: &[u8], name: &str, language: Option<u32>) -> Result<Vec<FilterPattern>> {
    let mut schema = schema(name);
    if language.is_some() {
        schema.add_field(SchemaField::new("Language", FieldType::UInt32));
    }
    let rs = parse(bytes, schema, name)?;
    let mut out = Vec::with_capacity(rs.records().len());
    for r in rs.records() {
        // Native -1 is the all-languages sentinel; preserve it in every client list.
        if language.is_some_and(
            |language| !matches!(u32_at(r, 2), Some(v) if v == language || v == u32::MAX),
        ) {
            continue;
        }
        // File order is the list order, which masking depends on: patterns apply in list order,
        // not text order. Never sort.
        let (Some(id), Some(pattern)) = (u32_at(r, 0), str_at(&rs, r, 1)) else {
            continue;
        };
        if pattern.is_empty() {
            continue;
        }
        out.push(FilterPattern { id, pattern });
    }
    Ok(out)
}

/// `ChatProfanity.dbc`, the masker's list, in file order.
pub fn load_chat_profanity(chain: &mut Chain) -> Result<Vec<FilterPattern>> {
    load(chain, CHAT_PROFANITY, "ChatProfanity")
}

/// `SpamMessages.dbc`, the spam predicate's list, in file order. The reference then scans a
/// second list the server can push (SMSG `0x332`, handler `0x49e6e0`); vmangos never sends one.
pub fn load_spam_messages(chain: &mut Chain) -> Result<Vec<FilterPattern>> {
    load(chain, SPAM_MESSAGES, "SpamMessages")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_language_list_preserves_order_without_foreign_patterns() {
        let mut b = b"WDBC".to_vec();
        for n in [4u32, 3, 12, 3] {
            b.extend_from_slice(&n.to_le_bytes());
        }
        for row in [[9u32, 1, 0], [3, 1, 1], [8, 1, u32::MAX], [7, 1, 0]] {
            for v in row {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.extend_from_slice(b"\0x\0");
        assert_eq!(
            patterns(&b, "ChatProfanity", Some(0))
                .unwrap()
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            [9, 8, 7]
        );
    }

    /// The row counts the reference's PCRE compiles, which only the patched archives give.
    #[test]
    fn the_lists_come_off_the_priority_walk_at_their_patched_sizes() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");

        let profanity = load_chat_profanity(&mut chain).expect("load ChatProfanity");
        assert_eq!(
            profanity.len(),
            2289,
            "2289 is patch.MPQ's copy; 1512 would mean we read dbc.MPQ"
        );
        assert!(
            profanity.iter().any(|p| p.pattern == r"\<twat\>"),
            "the ASCII half uses the \\< \\> word-boundary anchors"
        );
        assert!(
            profanity.iter().any(|p| !p.pattern.is_ascii()),
            "the list is multi-language in one table"
        );

        let spam = load_spam_messages(&mut chain).expect("load SpamMessages");
        assert_eq!(spam.len(), 28, "dbc.MPQ has no SpamMessages at all");
        assert!(
            spam.iter().all(|p| p.pattern.contains(r"\s*")),
            "every shipped spam row spaces its letters to defeat spacing"
        );
    }
}
