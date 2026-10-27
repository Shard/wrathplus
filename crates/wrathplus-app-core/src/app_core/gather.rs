//! A creature corpse's gather trade, 12340's `0x715e50`: the index into
//! [`super::state::LearnedAbilities::gather`], the cursor's icon and the tooltip's line.

/// The trade from the template's `type_flags`: `HERB_LOOT` (`0x100`) 1, `MINING_LOOT` (`0x200`) 2,
/// `ENGINEERING_LOOT` (`0x8000`) 3, and anything else, a missing template included, 0 Skinning.
pub fn gather_kind(type_flags: u32) -> usize {
    if type_flags & 0x100 != 0 {
        1
    } else if type_flags & 0x200 != 0 {
        2
    } else if type_flags & 0x8000 != 0 {
        3
    } else {
        0
    }
}

/// The tooltip's line by trade (`0x620fc3`), red as shown; its skill-difficulty colour for a
/// learned trade (`0x61a1f0`) is not built.
pub const LINE_KEYS: [&str; 4] = [
    "UNIT_SKINNABLE_LEATHER",
    "UNIT_SKINNABLE_HERB",
    "UNIT_SKINNABLE_ROCK",
    "UNIT_SKINNABLE_BOLTS",
];

#[cfg(test)]
mod tests {
    #[test]
    fn the_first_gather_flag_names_the_trade() {
        assert_eq!(super::gather_kind(0), 0);
        assert_eq!(super::gather_kind(0x100), 1);
        assert_eq!(super::gather_kind(0x200), 2);
        assert_eq!(super::gather_kind(0x8000), 3);
        assert_eq!(super::gather_kind(0x300), 1, "the herb test comes first");
    }
}
