//! The spell-targeting push: the cursor state `crate::spell::targeting` owns becomes the VM's
//! `SpellIsTargeting`, `SpellCanTargetUnit` and pickup-reroute answers each frame, and its spell
//! edges fire `CURRENT_SPELL_CAST_CHANGED`.

use bevy::prelude::*;

use wrathplus_ui::script::UiScript;

use crate::app_core::schedule::UnitFeed;
use crate::app_core::vm_memo::VmMemo;
use crate::spell::targeting::{BindChecks, SpellTargeting, TargetingWants};
use crate::target::unit_tokens::UnitTokens;

/// Push the targeting state into the VM each frame, before the input pass, so a word armed last
/// frame stands for this frame's clicks: the ESC chain's word, the bag and doll pickup reroute's
/// item half, and `CURRENT_SPELL_CAST_CHANGED` on each edge. That event hides the enchant confirm
/// popups (`UIParent.lua:449`); its one emitter `0x4b3250` is called from the cast arm, abort and
/// bind sites, `StopTargeting 0x6e4900` among them. It fires on the spell changing, so a cancel
/// and re-arm in one frame still counts.
pub(crate) fn feed_targeting_to_vm(
    targeting: Res<SpellTargeting>,
    checks: BindChecks,
    tokens: UnitTokens,
    mut last: Local<VmMemo<Option<u32>>>,
    script: Option<NonSendMut<UiScript>>,
) {
    if let Some(mut script) = script {
        let last = last.get(&script);
        script.set_spell_targeting(targeting.active());
        script.set_item_pick_armed(targeting.wants(TargetingWants::Item));
        // `SpellCanTargetUnit(unit)`: the VM resolves the token to a guid (`0x6e6d3a`), and
        // `0x6e6460`'s unit leg answers for that guid. The units to ask are the held ones the
        // resolver's inputs list, the `target` chains' included.
        let targetable: Vec<u64> = script
            .held_unit_guids()
            .into_iter()
            .filter(|&guid| {
                tokens
                    .held(guid)
                    .is_some_and(|(entity, _)| targeting.can_target_unit(entity, &checks))
            })
            .collect();
        script.set_spell_targetable_units(targetable);
        if *last != targeting.spell() {
            *last = targeting.spell();
            script.fire_event("CURRENT_SPELL_CAST_CHANGED", vec![]);
        }
    }
}

/// The targeting push, in the unit feed ahead of the input pass.
pub struct UiSpellTargetingPlugin;

impl Plugin for UiSpellTargetingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            // The state push runs before the input pass's `ToggleGameMenu`, whose
            // `SpellStopTargeting` lands with the script calls after it. After the old-target
            // clear, the pet bar's writer in the feed: `"pet"` resolves off the bar. After the aura
            // feed, whose resolver inputs list the units the hover verdict is asked for.
            feed_targeting_to_vm
                .in_set(UnitFeed)
                .after(crate::ui_pet::pet_stop_on_old_target_clear)
                .after(crate::app_core::schedule::AuraEvents),
        );
    }
}

#[cfg(test)]
#[path = "ui_spell_targeting_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ui_spell_targeting_group_tests.rs"]
mod group_tests;
