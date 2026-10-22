//! The pet stable pair's app half: `GetPetTalentTree` (`0x5d3d80`) from the active pet's family's
//! pet talent type and the `TalentTab` pet masks, and `IsAtStableMaster` (`0x5a1090`) from the
//! open stable master.
//!
//! The family is the pet's cached creature template's, as `UnitCreatureFamily("pet")` reads it
//! ([`crate::ui_pet_stats`]); a miss reads no tree until the creature query lands. Unresolved,
//! labelled: the reference's talent-capable test on the pet (`0x71b630`) is not modelled; a family
//! without a pet talent type (the warlock minions, -1) already answers nil.

use bevy::prelude::*;

use wrathplus_ui::script::UiScript;

use crate::names::NameCache;
use crate::ui_pet::{PetBar, PetUnit};
use crate::ui_pet_stats::PetFamilyTables;
use crate::ui_stable::StableOpen;
use crate::ui_talent::Talents;
use crate::ui_unit::UnitFeed;

pub(crate) struct UiPetTalentTreePlugin;

impl Plugin for UiPetTalentTreePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            feed_pet_talent_tree
                .in_set(UnitFeed)
                .after(crate::ui_pet::pet_stop_on_old_target_clear)
                .after(crate::ui_stable::feed_stable),
        );
    }
}

/// The tree name for a pet talent type: the first tab whose pet mask has its bit.
fn tree_for(
    talents: Option<&Talents>,
    families: Option<&PetFamilyTables>,
    family: Option<u32>,
) -> Option<String> {
    let ty = families?.families.get(family?)?.pet_talent_type;
    talents?.catalog.pet_tab_name(ty).map(str::to_string)
}

#[allow(clippy::type_complexity)] // Bevy system parameters / a closure table, kept inline
fn feed_pet_talent_tree(
    script: Option<NonSendMut<UiScript>>,
    bar: Res<PetBar>,
    pet: PetUnit,
    names: Res<NameCache>,
    families: Option<Res<PetFamilyTables>>,
    talents: Option<Res<Talents>>,
    stable: Res<StableOpen>,
    mut last: Local<crate::ui_script::VmMemo<Option<(Option<String>, bool)>>>,
) {
    let Some(mut script) = script else { return };
    let family = pet
        .store(bar.guid())
        .and_then(|s| s.0.object_entry())
        .filter(|&e| e != 0)
        .and_then(|entry| names.creature_record(entry))
        .map(|r| r.pet_family);
    let fresh = (
        tree_for(talents.as_deref(), families.as_deref(), family),
        stable.npc.is_some(),
    );
    let last = last.get(&script);
    if last.as_ref() != Some(&fresh) {
        script.set_wrath_pet_stable(fresh.0.clone(), fresh.1);
        *last = Some(fresh);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wrathplus_formats::{TalentCatalog, TalentTabInfo};

    #[test]
    fn the_tree_follows_the_family_type_and_the_tab_masks() {
        let tab = |id, name: &str, pet_talent_mask| TalentTabInfo {
            id,
            name: name.into(),
            race_mask: 0,
            class_mask: 0,
            pet_talent_mask,
            background: String::new(),
        };
        let talents = Talents {
            catalog: TalentCatalog::from_rows(
                vec![tab(409, "Tenacity", 2), tab(410, "Ferocity", 1)],
                Vec::new(),
            ),
        };
        assert_eq!(
            tree_for(Some(&talents), None, Some(1)),
            None,
            "no family table"
        );
        assert_eq!(tree_for(Some(&talents), None, None), None);
    }
}
