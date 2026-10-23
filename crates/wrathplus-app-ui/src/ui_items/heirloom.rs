//! Heirloom scaling for the item views: the `ScalingStat*` tables and each template's
//! `[ScalingStatDistribution, ScalingStatValue]` from `SMSG_ITEM_QUERY_SINGLE_RESPONSE`, turned
//! into the per-level table the tooltip and `GetItemStats` read (build 12340's `0x61f550`).

use std::collections::HashMap;

use wrathplus_formats::heirlooms::HeirloomCatalog;
use wrathplus_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

use crate::net::NetHandlerApp;

/// The scaling tables; absent when they fail to load, and every item then keeps its template.
#[derive(Resource)]
pub(crate) struct Heirlooms(pub(crate) HeirloomCatalog);

/// Each scaling template's `[distribution, mask]`, and the entries learned since the item feed
/// last pushed, which it re-pushes so a view built before the scaling landed is replaced.
#[derive(Resource, Default)]
pub(crate) struct ItemScalings {
    pub(crate) by_entry: HashMap<u32, [u32; 2]>,
    fresh: Vec<u32>,
}

impl ItemScalings {
    pub(crate) fn take_fresh(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.fresh)
    }
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<ItemScalings>()
        .add_systems(Startup, load.after(wrathplus_assets::AssetSet::Open))
        .net_handler(SessionEventKind::WrathItemScaling, on_scaling);
}

fn load(mut commands: Commands, world_assets: Option<Res<wrathplus_assets::WorldAssets>>) {
    use wrathplus_assets::LockRecover;
    let Some(world_assets) = world_assets else {
        return;
    };
    let mut chain = world_assets.chain.lock_recover();
    match wrathplus_formats::heirlooms::load_heirloom_catalog(&mut chain) {
        Ok(cat) => commands.insert_resource(Heirlooms(cat)),
        Err(e) => warn!("ui_items: ScalingStat tables failed to load: {e:#}"),
    }
}

/// A template scales when its mask is set (the builder's gate, `0x6277f0` reads `+0xbc`).
fn on_scaling(In(event): In<SessionEvent>, mut scalings: ResMut<ItemScalings>) {
    if let SessionEvent::WrathItemScaling(entry, Some(scaling)) = event {
        if scaling[1] != 0 {
            scalings.by_entry.insert(entry, scaling);
            scalings.fresh.push(entry);
        }
    }
}

/// The item's numbers at every `ScalingStatValues` level; `None` without its distribution row.
pub(crate) fn heirloom_view(
    cat: &HeirloomCatalog,
    [distribution, mask]: [u32; 2],
) -> Option<wrathplus_ui::heirloom::HeirloomView> {
    let dist = cat.distribution(distribution)?;
    let levels = (1..)
        .map_while(|level| cat.scale(distribution, mask, level))
        .map(|s| wrathplus_ui::heirloom::HeirloomLevel {
            stats: s.stats,
            armor: s.armor,
            dps: s.dps,
            spell_power: s.spell_power,
        })
        .collect();
    Some(wrathplus_ui::heirloom::HeirloomView {
        max_level: dist.max_level,
        levels,
    })
}

#[cfg(test)]
#[path = "heirloom_tests.rs"]
mod tests;
