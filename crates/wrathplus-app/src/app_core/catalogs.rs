//! The client's DBC tables the entity spawn and the UI both read, loaded once at boot; their
//! owners keep the loaders and re-export the types from their old paths.

use bevy::prelude::*;

/// `PetPersonality.dbc` and `PetLoyalty.dbc`, loaded once; without them happiness and loyalty
/// answer the bindings' failure forms.
#[derive(Resource)]
pub(crate) struct PetStatTables {
    pub(crate) personalities: wrathplus_formats::PetPersonalities,
    pub(crate) loyalty: wrathplus_formats::PetLoyaltyNames,
}

/// `CreatureFamily.dbc` (the word and icon) and `ItemPetFood.dbc` (the diet a family's food mask
/// names), apart from [`PetStatTables`] so each half degrades alone.
#[derive(Resource)]
pub(crate) struct PetFamilyTables {
    pub(crate) families: wrathplus_formats::CreatureFamilies,
    pub(crate) foods: wrathplus_formats::PetFoodNames,
}

/// `DurabilityCosts.dbc` and `DurabilityQuality.dbc`, the client's repair prices; absent, every
/// cost shows 0.
#[derive(Resource)]
pub(crate) struct RepairTables(pub(crate) wrathplus_formats::DurabilityTables);

/// `BankBagSlotPrices.dbc`, the client's own slot prices; absent, the purchase row shows 0 and
/// the server still charges its own price.
#[derive(Resource)]
pub(crate) struct BankPrices(pub(crate) wrathplus_formats::BankBagSlotPrices);

/// `StableSlotPrices.dbc`, the purchase row's prices; absent, the row quotes 0.
#[derive(Resource)]
pub(crate) struct StableSlotPrices(pub(crate) wrathplus_formats::StableSlotPrices);

/// `Item.dbc`, the client's record of every item entry: a creature's virtual items name only the
/// entry (`UNIT_VIRTUAL_ITEM_SLOT_ID`), so their display, sheath and class come from here. Absent,
/// creatures hold nothing.
#[derive(Resource)]
pub(crate) struct ItemRecords(pub(crate) wrathplus_formats::ItemRecordCatalog);

/// The race id to `UnitRace`'s display name and `raceFile` token (`"Scourge"`, `"NightElf"`); the
/// display name is also what `$R`/`$r` expand to. Build 12340's enUS `ChrRaces.dbc` rows 1-11,
/// its name and `clientFileString` columns; its gendered name columns are empty in enUS.
pub(crate) fn race_names(race: u8) -> Option<(&'static str, &'static str)> {
    Some(match race {
        1 => ("Human", "Human"),
        2 => ("Orc", "Orc"),
        3 => ("Dwarf", "Dwarf"),
        4 => ("Night Elf", "NightElf"),
        5 => ("Undead", "Scourge"),
        6 => ("Tauren", "Tauren"),
        7 => ("Gnome", "Gnome"),
        8 => ("Troll", "Troll"),
        9 => ("Goblin", "Goblin"),
        10 => ("Blood Elf", "BloodElf"),
        11 => ("Draenei", "Draenei"),
        _ => return None,
    })
}

/// The class id to `UnitClass`'s display name and uppercase `classFileName`.
pub(crate) fn class_names(class: u8) -> Option<(&'static str, &'static str)> {
    Some(match class {
        1 => ("Warrior", "WARRIOR"),
        2 => ("Paladin", "PALADIN"),
        3 => ("Hunter", "HUNTER"),
        4 => ("Rogue", "ROGUE"),
        5 => ("Priest", "PRIEST"),
        6 => ("Death Knight", "DEATHKNIGHT"),
        7 => ("Shaman", "SHAMAN"),
        8 => ("Mage", "MAGE"),
        9 => ("Warlock", "WARLOCK"),
        11 => ("Druid", "DRUID"),
        _ => return None,
    })
}
