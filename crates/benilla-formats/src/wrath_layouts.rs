//! Build-12340 layouts for the DBCs whose shape changed beyond wider locale blocks, as maps from
//! the 1.12 schema each loader declares to the 3.3.5 file's columns. A loader keeps its 1.12
//! column numbers; [`crate::dbc::parse`] applies the map when the file is the 3.3.5 shape.
//!
//! Source of the 3.3.5 layouts: AzerothCore's `DBCStructure.h`/`DBCfmt.h`. A 1.12 column the
//! 3.3.5 file no longer carries maps to `None` and reads as 0.

/// The map for `table` (its name, `.dbc` optional) from a `schema_len`-slot 1.12 schema to a
/// `file_fields`-column 3.3.5 file, when one is known.
pub(crate) fn columns(table: &str, schema_len: usize, file_fields: usize) -> Option<Vec<Option<usize>>> {
    let table = table.strip_suffix(".dbc").unwrap_or(table);
    let (vanilla, wrath, build): (usize, usize, fn(&mut Map)) = match table {
        "Spell" => (173, 234, spell),
        "SpellDispelType" => (12, 21, spell_dispel_type),
        "SkillLine" => (22, 56, skill_line),
        "TalentTab" => (15, 24, talent_tab),
        "SpellShapeshiftForm" => (14, 35, spell_shapeshift_form),
        "Faction" => (37, 57, faction),
        "ChrRaces" => (29, 69, chr_races),
        "ChrClasses" => (17, 60, chr_classes),
        "CreatureDisplayInfo" => (12, 16, creature_display_info),
        "CreatureFamily" => (18, 28, creature_family),
        "SpellItemEnchantment" => (24, 38, spell_item_enchantment),
        "AreaTable" => (25, 36, area_table),
        "WorldMapArea" => (8, 11, |m| { m.same(0, 8); }),
        "WorldMapContinent" => (13, 14, |m| { m.same(0, 13); }),
        "TaxiPathNode" => (9, 11, |m| { m.same(0, 9); }),
        // Two pet-talent masks appended.
        "Talent" => (21, 23, |m| { m.same(0, 21); }),
        // 1.12's trailing skill-up count is gone.
        "SkillLineAbility" => (15, 14, |m| { m.same(0, 14); }),
        "SpellRange" => (22, 40, spell_range),
        // Columns appended: behavior tier; the advanced-sound id; sheathe sounds; missile and
        // kit extras.
        "AnimationData" => (7, 8, |m| { m.same(0, 7); }),
        "SoundEntries" => (29, 30, |m| { m.same(0, 29); }),
        "Material" => (3, 5, |m| { m.same(0, 3); }),
        "SpellVisual" => (16, 32, |m| { m.same(0, 16); }),
        // 1.12's kit type (1) is 3.3.5's start animation, a different column; flags appended.
        "SpellVisualKit" => (35, 38, |m| { m.run(0, 0, 1).run(2, 2, 33); }),
        // Min and max allowed scale appended.
        "SpellVisualEffectName" => (5, 7, |m| { m.same(0, 5); }),
        // Four doodad weights between the doodads and the density.
        "GroundEffectTexture" => (7, 11, |m| { m.same(0, 5).run(5, 9, 2); }),
        _ => return None,
    };
    if schema_len != vanilla || file_fields != wrath {
        return None;
    }
    let mut map = Map(vec![None; vanilla]);
    build(&mut map);
    Some(map.0)
}

/// 1.12 slot → 3.3.5 column.
struct Map(Vec<Option<usize>>);

impl Map {
    /// `len` consecutive slots from `from` read the file from `to`.
    fn run(&mut self, from: usize, to: usize, len: usize) -> &mut Self {
        for k in 0..len {
            self.0[from + k] = Some(to + k);
        }
        self
    }

    /// Slots `from..from+len` at the same columns.
    fn same(&mut self, from: usize, len: usize) -> &mut Self {
        self.run(from, from, len)
    }

    /// A 1.12 localized string (8 locale slots + flags, at `from`) reads the 3.3.5 block at `to`
    /// (16 slots + flags): its first eight slots and its flags.
    fn loc(&mut self, from: usize, to: usize) -> &mut Self {
        self.run(from, to, 8);
        self.0[from + 8] = Some(to + 16);
        self
    }
}

/// `Spell.dbc` 173 → 234. 1.12's `School` (1) is an index; 3.3.5 keeps a `SchoolMask` (225),
/// which the spell loader converts. `CastUI` (3) and the per-effect dice (67-72) are gone.
fn spell(m: &mut Map) {
    m.run(0, 0, 1)
        .run(1, 225, 1)
        .run(2, 1, 1)
        .run(4, 2, 1)
        .run(5, 3, 1)
        .run(6, 4, 5) // Attributes..AttributesEx4
        .run(11, 12, 1) // Stances
        .run(12, 14, 1) // StancesNot
        .run(13, 16, 3) // Targets, TargetCreatureType, RequiresSpellFocus
        .run(16, 20, 2) // CasterAuraState, TargetAuraState
        .run(18, 28, 43) // CastingTimeIndex..EquippedItemInventoryTypeMask
        .run(61, 71, 6) // Effect[3], EffectDieSides[3]
        .run(73, 77, 36) // EffectRealPointsPerLevel..EffectMiscValue
        .run(109, 116, 6) // EffectTriggerSpell[3], EffectPointsPerComboPoint[3]
        .run(115, 131, 5) // SpellVisual[2], SpellIconID, ActiveIconID, SpellPriority
        .loc(120, 136) // SpellName
        .loc(129, 153) // Rank
        .loc(138, 170) // Description
        .loc(147, 187) // ToolTip
        .run(156, 204, 5) // ManaCostPercentage..SpellFamilyName
        .run(161, 209, 2) // SpellFamilyFlags (low, high)
        .run(163, 212, 10); // MaxAffectedTargets..RequiredAuraVision
}

/// `SpellDispelType.dbc` 12 → 21: id, name, then the mask (the tooltip's name gate) and the
/// internal name.
fn spell_dispel_type(m: &mut Map) {
    m.run(0, 0, 1).loc(1, 1).run(10, 18, 1).run(11, 20, 1);
}

/// `SkillLine.dbc` 22 → 56: id, category, costs, name, description, icon.
fn skill_line(m: &mut Map) {
    m.same(0, 3).loc(3, 3).loc(12, 20).run(21, 37, 1);
}

/// `TalentTab.dbc` 15 → 24: id, name, icon, race mask, class mask, order, background.
fn talent_tab(m: &mut Map) {
    m.run(0, 0, 1)
        .loc(1, 1)
        .run(10, 18, 3)
        .run(13, 22, 2);
}

/// `SpellShapeshiftForm.dbc` 14 → 35: id, bonus action bar, name, flags, creature type,
/// attack icon.
fn spell_shapeshift_form(m: &mut Map) {
    m.same(0, 2).loc(2, 2).run(11, 19, 3);
}

/// `Faction.dbc` 37 → 57: the reputation columns are unchanged; the name and description blocks
/// move past the 3.3.5 spillover columns.
fn faction(m: &mut Map) {
    m.same(0, 19).loc(19, 23).loc(28, 40);
}

/// `ChrRaces.dbc` 29 → 69. 1.12's mount scale (7), login effect (10), combat stun (11) and
/// starting taxi nodes (14) are gone.
fn chr_races(m: &mut Map) {
    m.same(0, 7)
        .run(8, 7, 2) // BaseLanguage, CreatureType
        .run(12, 9, 2) // ResSicknessSpell, SplashSound
        .run(15, 11, 2) // ClientFileString, CinematicSequence
        .loc(17, 14) // Name
        .run(26, 65, 3); // FacialHairCustomization[2], HairCustomization
}

/// `ChrClasses.dbc` 17 → 60: damage bonus stat, display power, pet name token, name, file name,
/// spell family, flags (whose relic bit 1.12 reads).
fn chr_classes(m: &mut Map) {
    m.run(0, 0, 1)
        .run(2, 1, 3)
        .loc(5, 4)
        .run(14, 55, 3);
}

/// `CreatureDisplayInfo.dbc` 12 → 16: a portrait texture column precedes size class.
fn creature_display_info(m: &mut Map) {
    m.same(0, 9).run(9, 10, 3);
}

/// `CreatureFamily.dbc` 18 → 28: pet talent type and category precede the name.
fn creature_family(m: &mut Map) {
    m.same(0, 8).loc(8, 10).run(17, 27, 1);
}

/// `SpellItemEnchantment.dbc` 24 → 38: a charges column precedes the effects.
fn spell_item_enchantment(m: &mut Map) {
    m.run(0, 0, 1).run(1, 2, 12).loc(13, 14).run(22, 31, 2);
}

/// `SpellRange.dbc` 22 → 40: 3.3.5 splits min and max range into hostile and friendly columns;
/// 1.12's single values read the hostile ones.
fn spell_range(m: &mut Map) {
    m.run(0, 0, 2).run(2, 3, 1).run(3, 5, 1).loc(4, 6).loc(13, 23);
}

/// `AreaTable.dbc` 25 → 36: liquid type grew to four columns.
fn area_table(m: &mut Map) {
    m.same(0, 11)
        .loc(11, 11)
        .run(20, 28, 2)
        .run(22, 33, 3);
}

#[cfg(test)]
mod tests {
    use super::columns;

    #[test]
    fn every_map_is_in_range_and_injective() {
        for (table, v, w) in [
            ("Spell", 173, 234),
            ("SpellDispelType", 12, 21),
            ("SkillLine", 22, 56),
            ("TalentTab", 15, 24),
            ("SpellShapeshiftForm", 14, 35),
            ("Faction", 37, 57),
            ("ChrRaces", 29, 69),
            ("ChrClasses", 17, 60),
            ("CreatureDisplayInfo", 12, 16),
            ("CreatureFamily", 18, 28),
            ("SpellItemEnchantment", 24, 38),
            ("AreaTable", 25, 36),
            ("WorldMapArea", 8, 11),
            ("WorldMapContinent", 13, 14),
            ("TaxiPathNode", 9, 11),
            ("Talent", 21, 23),
            ("SkillLineAbility", 15, 14),
            ("SpellRange", 22, 40),
            ("AnimationData", 7, 8),
            ("SoundEntries", 29, 30),
            ("Material", 3, 5),
            ("SpellVisual", 16, 32),
            ("GroundEffectTexture", 7, 11),
            ("SpellVisualKit", 35, 38),
            ("SpellVisualEffectName", 5, 7),
        ] {
            let map = columns(table, v, w).unwrap();
            let mut seen = std::collections::HashSet::new();
            for c in map.iter().flatten() {
                assert!(*c < w, "{table}: column {c} out of range");
                assert!(seen.insert(*c), "{table}: column {c} mapped twice");
            }
        }
        assert!(columns("Spell.dbc", 173, 234).is_some());
        assert!(columns("Spell", 173, 173).is_none());
    }

    #[test]
    fn spell_columns_land_on_the_build_12340_fields() {
        let m = columns("Spell", 173, 234).unwrap();
        for (vanilla, wrath) in [
            (18, 28),   // CastingTimeIndex
            (36, 46),   // rangeIndex
            (42, 52),   // Reagent[0]
            (82, 86),   // EffectImplicitTargetA[0]
            (109, 116), // EffectTriggerSpell[0]
            (117, 133), // SpellIconID
            (120, 136), // SpellName enUS
            (156, 204), // ManaCostPercentage
            (165, 214), // PreventionType
        ] {
            assert_eq!(m[vanilla], Some(wrath), "slot {vanilla}");
        }
        assert_eq!(m[67], None);
    }
}
