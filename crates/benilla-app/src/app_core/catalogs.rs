//! The client's DBC tables the entity spawn and the UI both read, loaded once at boot; their
//! owners keep the loaders and re-export the types from their old paths.

use bevy::prelude::*;

/// `PetPersonality.dbc` and `PetLoyalty.dbc`, loaded once; without them happiness and loyalty
/// answer the bindings' failure forms.
#[derive(Resource)]
pub(crate) struct PetStatTables {
    pub(crate) personalities: benilla_formats::PetPersonalities,
    pub(crate) loyalty: benilla_formats::PetLoyaltyNames,
}

/// `CreatureFamily.dbc` (the word and icon) and `ItemPetFood.dbc` (the diet a family's food mask
/// names), apart from [`PetStatTables`] so each half degrades alone.
#[derive(Resource)]
pub(crate) struct PetFamilyTables {
    pub(crate) families: benilla_formats::CreatureFamilies,
    pub(crate) foods: benilla_formats::PetFoodNames,
}

/// `DurabilityCosts.dbc` and `DurabilityQuality.dbc`, the client's repair prices; absent, every
/// cost shows 0.
#[derive(Resource)]
pub(crate) struct RepairTables(pub(crate) benilla_formats::DurabilityTables);

/// `BankBagSlotPrices.dbc`, the client's own slot prices; absent, the purchase row shows 0 and
/// the server still charges its own price.
#[derive(Resource)]
pub(crate) struct BankPrices(pub(crate) benilla_formats::BankBagSlotPrices);

/// `StableSlotPrices.dbc`, the purchase row's prices; absent, the row quotes 0.
#[derive(Resource)]
pub(crate) struct StableSlotPrices(pub(crate) benilla_formats::StableSlotPrices);
