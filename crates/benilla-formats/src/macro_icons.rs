//! The macro icon chooser's catalog, the list `GetNumMacroIcons`/`GetMacroIconInfo` serve. The
//! reference's `BuildMacroIconList` (`0x4f0090`) enumerates the files under `Interface\Icons\`; it
//! does not read `SpellIcon.dbc`.

use anyhow::Result;

use crate::Chain;

/// The directory the chooser enumerates and the prefix `GetMacroIconInfo` splices back on
/// (`0x84c988`, at `0x4f1a8d`); the reference strips its 16 characters from each stored entry.
const ICON_DIR: &str = "Interface\\Icons\\";

fn macro_icon_stem(path: &str) -> Option<&str> {
    let stem = icon_stem(path)?;
    has_prefix(stem, &["Spell_", "Ability_"]).then_some(stem)
}

/// A file directly under `Interface\Icons\` as the chooser stores it: the name after the prefix
/// without a last-dot `.blp` or `.tga`.
fn icon_stem(path: &str) -> Option<&str> {
    let prefix = path.get(..ICON_DIR.len())?;
    let is_icon_dir = prefix
        .bytes()
        .zip(ICON_DIR.bytes())
        .all(|(actual, wanted)| {
            (actual == b'/' || actual == b'\\') && wanted == b'\\'
                || actual.eq_ignore_ascii_case(&wanted)
        });
    let path = is_icon_dir.then_some(&path[ICON_DIR.len()..])?;
    // The reference skips directories (`flags & 0x10`): here, a name in a subdirectory.
    if path.contains('\\') || path.contains('/') {
        return None;
    }
    // The last dot starts the extension, so `Foo.tga.blp` stores as `Foo.tga`: the reference's
    // picker shows `Ability_Druid_Mangle.tga` beside `Ability_Druid_Mangle`.
    let (stem, ext) = path.rsplit_once('.')?;
    if !ext.eq_ignore_ascii_case("blp") && !ext.eq_ignore_ascii_case("tga") {
        return None;
    }
    (!stem.is_empty()).then_some(stem)
}

fn has_prefix(stem: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| {
        stem.len() >= prefix.len()
            && stem.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    })
}

/// The chooser's list: every `Interface\Icons\` file whose name begins `Spell_` or `Ability_`,
/// sorted and deduplicated without case, each as the full path `GetMacroIconInfo` returns.
///
/// The reference unions the `(listfile)` of every open `patch*.MPQ` and `interface.MPQ` (name table
/// index 6, `0x82e12c`, read via `0x648fb0`) with two loose-file scans of `Interface\Icons\`; only
/// the archive half is built, as a stock install has no loose icons. Per entry it keeps the name
/// after the prefix without a last-dot `.blp` or `.tga`, skipping directories, then `qsort`s with
/// `SStrCmpI` (`0x4f0128`, comparator `0x4f05e0`) and drops adjacent duplicates
/// (`0x4f0140`-`0x4f01a4`): the order is alphabetical, and a stock 5875 install yields 517 icons.
pub fn load_macro_icons(chain: &mut Chain) -> Result<Vec<String>> {
    let mut names: Vec<String> = Vec::new();
    for entry in chain.list_filtered(|name| macro_icon_stem(name).is_some())? {
        let path = entry.name.replace('/', "\\");
        // The listing predicate already rejected every other name before probing archive indexes.
        names.push(
            macro_icon_stem(&path)
                .expect("filtered macro icon name")
                .to_string(),
        );
    }
    // Sort without case, then drop adjacent duplicates: the reference's two steps, in its order.
    names.sort_by_key(|n| n.to_ascii_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    Ok(names
        .into_iter()
        .map(|n| format!("{ICON_DIR}{n}"))
        .collect())
}

/// The first entry of both build-12340 lists, and the comparator's head (`0x564a20`).
const QUESTION_MARK: &str = "INV_Misc_QuestionMark";

/// Build 12340's two chooser lists (`BuildMacroIconLists`, `0x565840`), as names without the
/// `Interface\Icons\` prefix `GetMacroIconInfo` and `GetMacroItemIconInfo` splice back on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathMacroIcons {
    /// `GetNumMacroIcons`: `Ability_` and `Spell_` files.
    pub spell: Vec<String>,
    /// `GetNumMacroItemIcons`: `INV_` files (`0xa0f944`).
    pub item: Vec<String>,
}

/// Both lists start with `INV_Misc_QuestionMark`, take every `Interface\Icons\` archive file by
/// prefix (`0x565630`), sort with the question mark first then by name (`0x564a20`), and drop
/// adjacent duplicates. Labelled: the two loose-file scans (`0x5656b0`, which strips only `.bz`,
/// and `0x565810`) are not built, as a stock install has no loose icons; the archive half's
/// extension strip is the 1.12 builder's (its add `0x565550`/`0x5655c0` is unread); and the
/// compares are taken as case-insensitive, as 1.12's `SStrCmpI`.
pub fn load_wrath_macro_icons(chain: &mut Chain) -> Result<WrathMacroIcons> {
    let mut icons = WrathMacroIcons {
        spell: vec![QUESTION_MARK.to_string()],
        item: vec![QUESTION_MARK.to_string()],
    };
    for entry in chain.list_filtered(|name| icon_stem(name).is_some())? {
        let path = entry.name.replace('/', "\\");
        let Some(stem) = icon_stem(&path) else {
            continue;
        };
        if has_prefix(stem, &["Ability_", "Spell_"]) {
            icons.spell.push(stem.to_string());
        }
        if has_prefix(stem, &["INV_"]) {
            icons.item.push(stem.to_string());
        }
    }
    for list in [&mut icons.spell, &mut icons.item] {
        sort_wrath(list);
    }
    Ok(icons)
}

/// `0x564a20`'s order, then the adjacent-duplicate drop.
fn sort_wrath(list: &mut Vec<String>) {
    let head = |n: &str| !n.eq_ignore_ascii_case(QUESTION_MARK);
    list.sort_by_key(|n| (head(n), n.to_ascii_lowercase()));
    list.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_filter_matches_the_chooser_name_rules() {
        let cases = [
            (
                "Interface\\Icons\\Spell_Holy_Test.blp",
                Some("Spell_Holy_Test"),
            ),
            ("interface\\icons\\ability_Test.TGA", Some("ability_Test")),
            (
                "Interface\\Icons\\Ability_Test.tga.blp",
                Some("Ability_Test.tga"),
            ),
            ("Interface\\Icons\\Item_Test.blp", None),
            ("Interface\\Icons\\Spell_Test.png", None),
            ("Interface\\Icons\\Nested\\Spell_Test.blp", None),
            ("Interface\\Icons\\Spell_Test", None),
            ("Interface\\Icons\\.blp", None),
            ("Other\\Icons\\Spell_Test.blp", None),
        ];

        for (name, expected) in cases {
            assert_eq!(macro_icon_stem(name), expected, "candidate {name}");
        }
        assert_eq!(
            macro_icon_stem("Interface/Icons/Spell_Test.blp"),
            Some("Spell_Test")
        );
    }

    #[test]
    fn wrath_lists_put_the_question_mark_first_then_sort_and_dedup_without_case() {
        let mut list: Vec<String> = [
            "INV_Misc_QuestionMark",
            "inv_sword_04",
            "INV_Axe_01",
            "INV_Sword_04",
            "inv_misc_questionmark",
        ]
        .map(String::from)
        .to_vec();
        sort_wrath(&mut list);
        assert_eq!(
            list,
            ["INV_Misc_QuestionMark", "INV_Axe_01", "inv_sword_04"]
        );
        assert!(has_prefix("inv_Bag", &["INV_"]) && !has_prefix("Spell_x", &["INV_"]));
        assert_eq!(
            icon_stem("Interface\\Icons\\INV_Axe_01.blp"),
            Some("INV_Axe_01")
        );
    }
}
