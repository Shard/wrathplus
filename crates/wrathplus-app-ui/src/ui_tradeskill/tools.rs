//! A recipe's "Requires:" tools, as build 12340's `GetTradeSkillTools` (`0x5de620`) lists them: the
//! spell focus, then `Totem[0..2]` by item, then `RequiredTotemCategoryID[0..2]` by
//! `TotemCategory.dbc` name. A focus always reads met (its `hasTool` is the literal 1.0); a totem is
//! met by carrying the item (`0x754a20`), a category by carrying any item whose own category serves
//! it (`0x7548f0`). Not built: the `RequiredAreasID` entry between the focus and the totems, which
//! names the area through `AreaGroup.dbc`.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use wrathplus_assets::{LockRecover, WorldAssets};
use wrathplus_formats::{SpellDisplay, SpellFocusCatalog, TotemCategoryCatalog};

use super::SpellFocus;
use crate::app_core::session::ObjectStore;
use crate::inventory::{carried_counts, count_of, InventoryScope};
use crate::items::Items;
use crate::net::{NetCommands, Objects};

/// `TotemCategory.dbc`: a category tool's name and which items serve it.
#[derive(Resource)]
pub(crate) struct TotemCategories(pub(crate) TotemCategoryCatalog);

/// The two name tables a "Requires:" line reads, bundled for the book's 16-param feed.
#[derive(SystemParam)]
pub(crate) struct ToolNames<'w> {
    pub(crate) focus: Option<Res<'w, SpellFocus>>,
    pub(crate) categories: Option<Res<'w, TotemCategories>>,
}

pub(super) fn load_totem_categories(mut commands: Commands, assets: Option<Res<WorldAssets>>) {
    let Some(assets) = assets else { return };
    let loaded = {
        let mut chain = assets.chain.lock_recover();
        wrathplus_formats::load_totem_categories(&mut chain)
    };
    match loaded {
        Ok(catalog) => {
            debug!("ui_tradeskill: {} totem categories", catalog.len());
            commands.insert_resource(TotemCategories(catalog));
        }
        Err(e) => warn!(
            "ui_tradeskill: TotemCategory.dbc failed to load — the Requires line drops category \
             tools: {e:#}"
        ),
    }
}

/// Whether any carried item serves the tool category `need`.
fn carries_category(
    categories: &TotemCategoryCatalog,
    need: u32,
    store: &ObjectStore,
    objects: &Objects,
    items: &Items,
    commands: &NetCommands,
) -> bool {
    carried_counts(&store.0, objects).into_keys().any(|entry| {
        items
            .template(entry, 0, commands)
            .is_some_and(|t| t.totem_category != 0 && categories.serves(t.totem_category, need))
    })
}

/// The `(name, has)` pairs, in `0x5de620`'s order. A tool whose template has not landed is left
/// out while it is asked for, as the reference does.
pub(super) fn recipe_tools(
    d: &SpellDisplay,
    focus: Option<&SpellFocusCatalog>,
    categories: Option<&TotemCategoryCatalog>,
    store: &ObjectStore,
    objects: &Objects,
    items: &Items,
    commands: &NetCommands,
) -> Vec<(String, bool)> {
    let mut tools = Vec::new();
    if d.requires_spell_focus != 0 {
        if let Some(name) = focus.and_then(|f| f.name(d.requires_spell_focus)) {
            tools.push((name.to_string(), true));
        }
    }
    for &t in d.totems.iter().filter(|&&t| t != 0) {
        let have = count_of(&store.0, objects, t, InventoryScope::CARRIED) > 0;
        if let Some(info) = items.template(t, 0, commands) {
            tools.push((info.name.clone(), have));
        }
    }
    if let Some(categories) = categories {
        for &need in d.totem_categories.iter().filter(|&&c| c != 0) {
            if let Some(row) = categories.get(need) {
                let have = carries_category(categories, need, store, objects, items, commands);
                tools.push((row.name.clone(), have));
            }
        }
    }
    tools
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
