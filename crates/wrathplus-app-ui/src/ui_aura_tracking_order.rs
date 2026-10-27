//! The tracking menu's spell list, 12340's `[0xbe8de4]` (its array `[0xbe8dec]`), in learn order:
//! a learned spell joins at the end (`0x542030`, the append at `0x54214b`) when any aura slot is
//! 44, 45 or 151 (`0x7fdf60`, [`wrathplus_formats::SpellDisplay::tracking_aura`]), and an
//! unlearned one leaves in place (`0x53fad0`), so a rank-up, an unlearn then a learn, moves to the
//! end. The login book arrives in ascending id order (the server sends it from a `std::map`). The
//! session reset `0x53ec40` empties the list for a new character; a `/reload` keeps it.

use std::collections::BTreeSet;

use bevy::prelude::*;

use wrathplus_ui::script::UiScript;

use crate::app_core::session::SelfGuid;
use crate::app_core::state::{PlayerActions, Spells};

/// The character the list belongs to, and the list: engine state, so it outlives a VM.
#[derive(Resource, Default)]
pub(super) struct TrackingOrder {
    guid: Option<u64>,
    order: Vec<u32>,
}

/// Keep `order` in learn order over the `known` tracking spells: survivors stay in place, a gone
/// spell leaves its slot, newcomers append in ascending id.
pub(crate) fn merge_learn_order(order: &mut Vec<u32>, known: &BTreeSet<u32>) {
    order.retain(|id| known.contains(id));
    let had: BTreeSet<u32> = order.iter().copied().collect();
    order.extend(known.iter().filter(|id| !had.contains(id)));
}

/// Follow the book into the list and push it to the VM when it moves, or to a fresh VM.
pub(super) fn feed_tracking_spells(
    script: Option<NonSendMut<UiScript>>,
    actions: Option<Res<PlayerActions>>,
    spells: Option<Res<Spells>>,
    self_guid: Option<Res<SelfGuid>>,
    mut state: ResMut<TrackingOrder>,
    mut pushed: Local<crate::app_core::vm_memo::VmMemo<Option<Vec<u32>>>>,
) {
    let (Some(mut script), Some(actions), Some(spells), Some(self_guid)) =
        (script, actions, spells, self_guid)
    else {
        return;
    };
    let (last, reset) = pushed.get_reset(&script);
    let new_character = state.guid != self_guid.0;
    if !reset && !new_character && !actions.is_changed() && !spells.is_changed() {
        return;
    }
    if new_character {
        *state = TrackingOrder {
            guid: self_guid.0,
            order: Vec::new(),
        };
    }
    let known: BTreeSet<u32> = actions
        .spells
        .iter()
        .copied()
        .filter(|&id| spells.catalog.get(id).is_some_and(|d| d.tracking_aura()))
        .collect();
    merge_learn_order(&mut state.order, &known);
    if reset || last.as_ref() != Some(&state.order) {
        *last = Some(state.order.clone());
        script.set_tracking_spells(state.order.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The login book lands ascending; a learn appends; an unlearn leaves its slot; a rank-up
    /// (old out, new in) moves to the end, as `0x53fad0` then `0x542030` do.
    #[test]
    fn the_list_keeps_learn_order_with_removal_in_place() {
        let set = |ids: &[u32]| ids.iter().copied().collect::<BTreeSet<u32>>();
        let mut order = Vec::new();
        merge_learn_order(&mut order, &set(&[2580, 1494, 2383]));
        assert_eq!(order, [1494, 2383, 2580], "the login book, ascending");
        merge_learn_order(&mut order, &set(&[2580, 1494, 2383, 19883]));
        merge_learn_order(&mut order, &set(&[2580, 1494, 2383, 19883, 5225]));
        assert_eq!(
            order,
            [1494, 2383, 2580, 19883, 5225],
            "learns append, not sort"
        );
        merge_learn_order(&mut order, &set(&[2580, 1494, 19883, 5225]));
        assert_eq!(
            order,
            [1494, 2580, 19883, 5225],
            "an unlearn leaves in place"
        );
        merge_learn_order(&mut order, &set(&[2580, 1494, 19883, 5225, 2383]));
        assert_eq!(
            order,
            [1494, 2580, 19883, 5225, 2383],
            "a relearn joins at the end"
        );
    }

    /// Each landing of the tracking aura is a setter call (`0x57ea30`) and fires
    /// `MINIMAP_UPDATE_TRACKING`: a re-landed same aura (restamped) fires again, a frame with no
    /// update does not.
    #[test]
    fn every_landing_of_the_tracking_aura_fires_the_minimap_update() {
        use crate::app_core::session::{ObjectStore, SelfPlayer};
        use crate::app_core::state::{ClientState, Selection};
        use crate::net::wrath_auras::PacketAuras;
        use crate::net::{Guid, GuidIndex, NetCommands};
        use wrathplus_formats::{SpellCatalog, SpellDisplay};
        const ME: u64 = 10;
        const FIND_HERBS: u32 = 2383;
        let (tx, _) = crossbeam_channel::unbounded();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(ClientState::InWorld)
            .init_resource::<Selection>()
            .init_resource::<crate::app_core::state::PetBar>()
            .init_resource::<GuidIndex>()
            .insert_resource(NetCommands(tx))
            .insert_resource(Spells {
                catalog: SpellCatalog::from_displays(
                    [(
                        FIND_HERBS,
                        SpellDisplay {
                            name: "Find Herbs".into(),
                            effect_apply_aura: [45, 0, 0],
                            ..Default::default()
                        },
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..Spells::empty_for_tests()
            })
            .add_plugins(crate::ui_aura::UiAuraPlugin);
        app.insert_non_send_resource(UiScript::new().unwrap());
        app.world_mut()
            .non_send_resource::<UiScript>()
            .run(
                "N = 0 local f = CreateFrame('Frame') \
                 f:RegisterEvent('MINIMAP_UPDATE_TRACKING') \
                 f:SetScript('OnEvent', function() N = N + 1 end)",
            )
            .unwrap();
        let me = app
            .world_mut()
            .spawn((ObjectStore::default(), Guid(ME), SelfPlayer))
            .id();
        app.world_mut().resource_mut::<GuidIndex>().0.insert(ME, me);
        crate::net::wrath_auras::set_for_test(app.world_mut(), ME, &[(0, FIND_HERBS)]);
        let fired = |app: &mut App| {
            app.update();
            app.world_mut()
                .non_send_resource::<UiScript>()
                .eval::<i64>("local n = N N = 0 return n")
                .unwrap()
        };
        assert_eq!(fired(&mut app), 1, "the first landing");
        assert_eq!(fired(&mut app), 0, "no update, no event");
        app.world_mut()
            .resource_mut::<PacketAuras>()
            .by_guid
            .get_mut(&ME)
            .unwrap()[0]
            .received_at = 5.0;
        assert_eq!(fired(&mut app), 1, "the same aura re-landed");
    }
}
