//! Native success-only presentation. Unknown template/ownership data never timeout-shows.
use super::{lifecycle::Lifecycle, NativeRollTemplates, NativeRolls};
use crate::{
    items::Items,
    net::{NetCommands, Objects, SelfGuid, SessionDialect},
};
use benilla_ui::script::{
    wrath_loot_roll_reasons as native_reasons, ScriptDialect, ScriptValue, UiScript,
    WrathLootRollEntry as Entry, WrathLootRollsState as State,
};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct Presentation {
    lifecycle: Lifecycle,
    published: std::collections::HashMap<u32, Entry>,
    lines: Vec<(benilla_protocol::wrath_roll::Roll, u16)>,
}

#[derive(Resource)]
pub(super) struct CategoryLimits(benilla_formats::item_limit_category::ItemLimitCategoryCatalog);

pub(super) fn load_limits(
    mut commands: Commands,
    assets: Option<Res<benilla_assets::WorldAssets>>,
) {
    use benilla_assets::LockRecover;
    let Some(assets) = assets else {
        return;
    };
    let mut chain = assets.chain.lock_recover();
    if !matches!(chain.profile(), benilla_formats::DataProfile::Wrath { .. }) {
        return;
    }
    match benilla_formats::item_limit_category::load_item_limit_categories(&mut chain) {
        Ok(catalog) => {
            commands.insert_resource(CategoryLimits(catalog));
        }
        Err(error) => warn!("ItemLimitCategory.dbc: {error:#}"),
    }
}

/// `0x754dd0`: entry limit precedes category limit; only a proven violation is false.
fn capacity(
    max_count: u32,
    category: u32,
    count: u32,
    mut owned_entry: impl FnMut() -> Option<u64>,
    mut owned_category: impl FnMut(u32) -> Option<(u64, u32)>,
) -> Option<bool> {
    if max_count != 0 && owned_entry()?.checked_add(u64::from(count))? > u64::from(max_count) {
        return Some(false);
    }
    if category != 0 {
        let (owned, limit) = owned_category(category)?;
        if owned.checked_add(u64::from(count))? > u64::from(limit) {
            return Some(false);
        }
    }
    Some(true)
}

pub(super) fn feed(
    script: Option<NonSendMut<UiScript>>,
    dialect: Res<SessionDialect>,
    mut presentation: ResMut<Presentation>,
    mut inbox: ResMut<NativeRolls>,
    mut templates: ResMut<NativeRollTemplates>,
    items: Res<Items>,
    objects: Objects,
    own: Res<SelfGuid>,
    commands: Res<NetCommands>,
    icons: Option<Res<crate::entities::ItemDisplays>>,
    limits: Option<Res<CategoryLimits>>,
    time: Res<Time>,
    names: Res<crate::names::NameCache>,
    mut chat: ResMut<crate::ui_chat::ChatLog>,
    loot: Res<crate::ui_loot::LootConfig>,
    mut memo: Local<crate::ui_script::VmMemo<State>>,
) {
    if *dialect != SessionDialect::Wrath12340 {
        return;
    }
    let Presentation {
        lifecycle,
        published,
        lines,
    } = &mut *presentation;
    templates.receipts.clear();
    lifecycle.tick(time.delta().as_millis().min(u128::from(u32::MAX)) as u32);
    for roll in inbox.updates.drain(..) {
        if !matches!(roll, benilla_protocol::wrath_roll::Roll::Start { .. }) {
            lines.push((roll.clone(), 0));
        }
        lifecycle.receive(roll);
    }
    super::outcomes::drain(
        lines,
        &items,
        &names,
        &commands,
        own.0,
        &mut chat,
        loot.show_loot_spam,
    );
    // Queries also progress while no VM exists; negative answers drop rather than resurrect.
    let mut ready = std::collections::HashMap::new();
    for roll in &lifecycle.entries {
        if roll.announced {
            continue;
        }
        let template = items.template(roll.item.entry, 0, &commands);
        if items.template_answered_unknown(roll.item.entry)
            || matches!(templates.metadata.get(&roll.item.entry), Some(None))
        {
            ready.insert(roll.id, Err(()));
            continue;
        }
        let (Some(t), Some(Some(metadata))) = (template, templates.metadata.get(&roll.item.entry))
        else {
            continue;
        };
        let can_receive = capacity(
            t.max_count,
            metadata.item_limit_category,
            roll.count,
            || {
                crate::ui_items::owned_entry_count(
                    objects.object(own.0?)?,
                    &objects,
                    roll.item.entry,
                )
            },
            |category| {
                let limit = limits.as_ref()?.0.get(category)?.quantity();
                let owned = crate::ui_items::owned_category_count(
                    objects.object(own.0?)?,
                    &objects,
                    category,
                    |entry| {
                        // The shared Items owner deduplicates queries across all consumers.
                        let _ = items.template(entry, 0, &commands);
                        templates
                            .metadata
                            .get(&entry)?
                            .as_ref()
                            .map(|m| m.item_limit_category)
                    },
                )?;
                Some((owned, limit))
            },
        );
        let reasons = native_reasons(
            roll.vote_mask,
            metadata.flags2,
            metadata.required_disenchant_skill,
            can_receive,
        );
        if reasons.iter().any(Option::is_none) {
            continue;
        }
        ready.insert(
            roll.id,
            Ok(Entry {
                roll_id: roll.id,
                guid: roll.item.guid,
                slot: roll.item.slot,
                map: roll.map,
                entry: roll.item.entry,
                count: roll.count,
                suffix_factor: roll.item.suffix_factor,
                random_property: roll.item.random_property,
                vote_mask: roll.vote_mask,
                texture: crate::ui_items::item_icon(icons.as_deref(), t.display_info_id),
                name: Some(t.name.clone()),
                quality: Some(t.quality),
                bind_on_pickup: t.bonding == 1,
                time_left_ms: roll.remaining_ms,
                // Native item links have enchant, three gems, signed random property, suffix and level.
                link: Some(format!(
                    "|c{}|Hitem:{}:0:0:0:0:{}:{}:0|h[{}]|h|r",
                    crate::ui_items::quality_color(t.quality),
                    roll.item.entry,
                    roll.item.random_property,
                    roll.item.suffix_factor,
                    t.name
                )),
                reasons,
                disenchant_skill_required: Some(metadata.required_disenchant_skill),
            }),
        );
    }
    let Some(mut script) = script else {
        lifecycle.prepare(|e| matches!(ready.get(&e.id), Some(Err(()))).then_some(false));
        return;
    };
    if script.dialect() != ScriptDialect::Wrath12340 {
        return;
    }
    let starts = lifecycle.prepare(|e| ready.get(&e.id).map(Result::is_ok));
    for (id, _) in &starts {
        if let Some(Ok(entry)) = ready.remove(id) {
            published.insert(*id, entry);
        }
    }
    published.retain(|id, _| lifecycle.entries.iter().any(|e| e.id == *id));
    let state = State {
        rolls: lifecycle
            .entries
            .iter()
            .filter_map(|e| {
                let mut entry = published.get(&e.id)?.clone();
                entry.time_left_ms = e.remaining_ms;
                Some(entry)
            })
            .collect(),
    };
    let (last, reset) = memo.get_reset(&script);
    if reset || *last != state {
        script.set_wrath_loot_rolls(state.clone());
        *last = state;
    }
    let announcements = if reset {
        lifecycle
            .entries
            .iter()
            .filter(|e| e.announced)
            .map(|e| (e.id, e.countdown_ms))
            .collect()
    } else {
        starts
    };
    for (id, countdown) in announcements {
        script.fire_event(
            "START_LOOT_ROLL",
            vec![
                ScriptValue::Int(id.into()),
                ScriptValue::Int(countdown.into()),
            ],
        );
    }
    for id in std::mem::take(&mut lifecycle.cancelled) {
        script.fire_event("CANCEL_LOOT_ROLL", vec![ScriptValue::Int(id.into())]);
    }
}

#[cfg(test)]
mod tests;

/// Resolve client-only IDs against the current native lifecycle, never against the 1.12 model.
pub(crate) fn drain(
    script: &mut UiScript,
    presentation: &mut Presentation,
    commands: &NetCommands,
) {
    for (id, vote) in script.take_loot_roll_confirms() {
        let live = presentation
            .lifecycle
            .entries
            .iter()
            .any(|e| e.id == id && e.announced && e.remaining_ms != 0);
        if live && vote <= 3 {
            let Some(entry) = presentation.published.get(&id) else {
                continue;
            };
            let event = if vote == 3 && entry.quality.is_some_and(|q| (q as i32) > 2) {
                "CONFIRM_DISENCHANT_ROLL"
            } else if vote != 0 && vote != 3 && entry.bind_on_pickup {
                "CONFIRM_LOOT_ROLL"
            } else {
                continue;
            };
            script.fire_event(
                event,
                vec![ScriptValue::Int(id.into()), ScriptValue::Int(vote.into())],
            );
        }
    }
    for (id, vote) in script.take_loot_roll_votes() {
        let Some(row) = presentation
            .lifecycle
            .entries
            .iter()
            .find(|e| e.id == id && e.announced && e.remaining_ms != 0)
        else {
            continue;
        };
        if !presentation
            .published
            .get(&id)
            .is_some_and(|e| e.permits(vote))
        {
            continue;
        }
        if commands
            .0
            .send(crate::net::ClientCommand::LootRoll {
                looted_target: row.item.guid,
                item_slot: row.item.slot,
                roll_type: vote,
            })
            .is_ok()
        {
            presentation.lifecycle.entries.retain(|e| e.id != id);
            presentation.lifecycle.cancelled.push(id);
            presentation.published.remove(&id);
        }
    }
}
