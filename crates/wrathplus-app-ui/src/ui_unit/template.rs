//! The creature template's fields a unit snapshot carries for the `Unit*` verbs that read the
//! template: `UnitLevel`'s boss bit and `UnitCreatureFamily`'s word.

use wrathplus_formats::CreatureFamilies;
use wrathplus_ui::script::UnitState;

use crate::names::NameCache;

/// Fill `s` from the cached template of the creature `guid`: its type flags (`[rec+0xc]`, whose
/// bit `0x4` `0x715d70` reads as the boss flag) and its family (`[rec+0x14]`, `0x7153e0`) as the
/// `CreatureFamily.dbc` word. A player, or a creature whose query has not answered, has neither.
pub(crate) fn fill(
    s: &mut UnitState,
    guid: u64,
    names: &NameCache,
    families: Option<&CreatureFamilies>,
) {
    let rec = wrathplus_protocol::guid::entry(guid).and_then(|e| names.creature_record(e));
    let Some(rec) = rec.filter(|_| !s.is_player) else {
        return;
    };
    s.creature_type_flags = rec.type_flags;
    s.creature_family = families
        .and_then(|f| f.name(rec.pet_family))
        .map(Into::into);
}

impl super::SnapshotTables<'_> {
    /// `CreatureFamily.dbc`, which [`fill`] names a template's family with.
    pub(crate) fn families(&self) -> Option<&CreatureFamilies> {
        self.families.as_deref().map(|t| &t.families)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::CreatureRecord;

    /// A boss-flagged template fills the type flags; a player's guid takes nothing.
    #[test]
    fn a_creature_carries_its_template_flags_and_a_player_none() {
        const BOSS: u64 = 0xF130_0000_7B00_0001;
        let mut names = NameCache::default();
        let record = CreatureRecord {
            name: "Hogger".into(),
            subname: None,
            creature_type: 7,
            pet_family: 0,
            rank: 1,
            type_flags: 0x4,
            civilian: false,
            racial_leader: false,
            display_id: 0,
            modifiers: None,
        };
        names.insert_creature(wrathplus_protocol::guid::entry(BOSS).unwrap(), Some(record));
        let mut s = UnitState::default();
        fill(&mut s, BOSS, &names, None);
        assert_eq!((s.creature_type_flags, s.creature_family), (0x4, None));
        let mut player = UnitState {
            is_player: true,
            ..Default::default()
        };
        fill(&mut player, BOSS, &names, None);
        assert_eq!(player.creature_type_flags, 0);
        assert_eq!(
            UnitState::sex_of_gender(Some(2)),
            1,
            "the neutral byte (`0x9fe7ec`)"
        );
    }
}
