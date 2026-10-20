//! Wrath token roster: known-currency bits select CurrencyTypes rows; the hidden 32-slot
//! token bag supplies instance counts. Honor/arena are separate native descriptor balances.
use std::collections::BTreeMap;

use benilla_protocol::{types::Field, ObjectFields};
use benilla_ui::script::{UiScript, WrathCurrencyEntry, WrathCurrencySnapshot};
use bevy::prelude::*;

use crate::entities::ItemDisplays;
use crate::items::Items;
use crate::net::{NetCommands, ObjectStore, Objects, SelfPlayer};
use crate::ui_script::VmMemo;

#[derive(Resource)]
pub(crate) struct CurrencyData {
    types: benilla_formats::currencies::CurrencyCatalog,
    categories: benilla_formats::currencies::CurrencyCategoryCatalog,
}

pub(super) fn load(mut commands: Commands, assets: Option<Res<benilla_assets::WorldAssets>>) {
    use benilla_assets::LockRecover;
    let Some(assets) = assets else { return };
    let mut chain = assets.chain.lock_recover();
    if !matches!(chain.profile(), benilla_formats::DataProfile::Wrath { .. }) {
        return;
    }
    match (
        benilla_formats::currencies::load_currency_types(&mut chain),
        benilla_formats::currencies::load_currency_categories(&mut chain),
    ) {
        (Ok(types), Ok(categories)) => commands.insert_resource(CurrencyData { types, categories }),
        (types, categories) => warn!(
            "currency DBCs: types={:?}, categories={:?}",
            types.err(),
            categories.err()
        ),
    }
}

/// Require both words: a partially streamed mask is not an authoritative empty roster.
fn known_currencies(player: &ObjectFields) -> Option<u64> {
    Some(
        u64::from(player.get_at(Field::PlayerKnownCurrencies, 0)?)
            | (u64::from(player.get_at(Field::PlayerKnownCurrencies, 1)?) << 32),
    )
}

fn token_counts<'a>(
    player: &ObjectFields,
    mut object: impl FnMut(u64) -> Option<&'a ObjectFields>,
) -> BTreeMap<u32, u32> {
    let mut counts = BTreeMap::<u32, u32>::new();
    // AzerothCore Player.h: CURRENCYTOKEN_SLOT_START..END has 32 GUID positions.
    for slot in 0..32u16 {
        let Some(item) = player
            .get_field_guid(Field::PlayerCurrencytokenSlot1, slot * 2)
            .filter(|&guid| guid != 0)
            .and_then(&mut object)
        else {
            continue;
        };
        if let Some((entry, count)) = item.object_entry().zip(item.item_stack_count()) {
            // Installed GetCurrencyListInfo (0x5b0680): first matching hidden stack wins.
            counts.entry(entry).or_insert(count);
        }
    }
    counts
}

fn roster<'a>(
    known: u64,
    types: impl Iterator<Item = &'a benilla_formats::currencies::CurrencyType>,
    counts: &BTreeMap<u32, u32>,
    mut category_name: impl FnMut(u32) -> Option<String>,
    mut metadata: impl FnMut(u32) -> (Option<String>, Option<String>),
) -> Vec<WrathCurrencyEntry> {
    let mut groups = BTreeMap::<u32, Vec<WrathCurrencyEntry>>::new();
    for currency in types {
        if currency.item_id == 0 || !currency.known_mask().is_some_and(|bit| known & bit != 0) {
            continue;
        }
        let (name, icon) = metadata(currency.item_id);
        groups
            .entry(currency.category_id)
            .or_default()
            .push(WrathCurrencyEntry {
                name,
                icon,
                category_id: currency.category_id,
                item_id: currency.item_id,
                count: counts.get(&currency.item_id).copied().unwrap_or(0),
                extra_currency_type: match currency.item_id {
                    43307 => 1,
                    43308 => 2,
                    _ => 0,
                },
                ..Default::default()
            });
    }
    let mut entries = Vec::new();
    for (category_id, children) in groups {
        entries.push(WrathCurrencyEntry {
            category_id,
            name: category_name(category_id),
            is_header: true,
            is_expanded: true,
            ..Default::default()
        });
        entries.extend(children);
    }
    entries
}

pub(crate) fn feed(
    script: Option<NonSendMut<UiScript>>,
    me: Query<&ObjectStore, With<SelfPlayer>>,
    objects: Objects,
    data: Option<Res<CurrencyData>>,
    items: Res<Items>,
    icons: Option<Res<ItemDisplays>>,
    commands: Res<NetCommands>,
    mut memo: Local<VmMemo<Option<WrathCurrencySnapshot>>>,
) {
    let Some(mut script) = script else { return };
    let last = memo.get(&script);
    let mut fresh = WrathCurrencySnapshot::default();
    if let Ok(player) = me.single() {
        fresh.honor_current = player.0.get(Field::PlayerHonorCurrency);
        fresh.arena_current = player.0.get(Field::PlayerArenaCurrency);
        if let (Some(known), Some(data)) = (known_currencies(&player.0), data.as_deref()) {
            let mut counts = token_counts(&player.0, |guid| objects.object(guid));
            // Native special rows bypass the hidden bag; never create rows without known bits.
            counts.insert(43307, fresh.arena_current.unwrap_or(0));
            counts.insert(43308, fresh.honor_current.unwrap_or(0));
            fresh.entries = roster(
                known,
                data.types.iter(),
                &counts,
                |id| data.categories.get(id).map(|c| c.name.clone()),
                |id| {
                    let Some(template) = items.template(id, 0, &commands) else {
                        return (None, None);
                    };
                    (
                        Some(template.name.clone()),
                        icons
                            .as_deref()
                            .and_then(|icons| icons.catalog.get(template.display_info_id))
                            .and_then(|display| display.icon.clone()),
                    )
                },
            );
        }
    }
    // Rebuild pending metadata every frame; only authoritative snapshot changes cross the seam.
    // VmMemo re-seeds /reload; missing self pushes an empty snapshot on logout.
    if last.as_ref() != Some(&fresh) {
        script.set_wrath_currency_snapshot(fresh.clone());
        *last = Some(fresh);
    }
}

#[cfg(test)]
pub(crate) mod tests;
