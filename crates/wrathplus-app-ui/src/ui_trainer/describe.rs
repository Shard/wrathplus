//! What a trainer row says about its spell beyond the name: the description and, at a
//! tradeskill trainer, the created item's link.

use crate::app_core::session::ObjectStore;
use crate::app_core::state::Spells;
use crate::items::Items;
use crate::net::NetCommands;
use wrathplus_formats::{SkillLineCatalog, SPELL_ATTR_IS_TRADESKILL};

use super::TRAINER_TYPE_TRADESKILL;

/// The feed's token inputs: the spell tables, the skill lines, the player and the VM.
pub(super) type DescribeInputs<'a> = (
    &'a Spells,
    &'a SkillLineCatalog,
    Option<&'a ObjectStore>,
    &'a wrathplus_ui::script::UiScript,
);

/// `GetTrainerServiceDescription` (`005959d0`) on the service spell's own record: its
/// `Description`, when not empty, with its tokens filled (`0057abc0` into a 1,024-byte buffer);
/// else, for a trade spell (`Attributes & 0x20`) whose `Effect[0]` creates an item (24, 59 or 157),
/// the item template's description once cached; else nothing, which the binding answers as nil.
/// Labelled: no caster spell modifiers apply, as in the tradeskill window's description.
pub(super) fn service_description(
    spell_id: u32,
    (spells, skill_lines, me, script): DescribeInputs,
    items: &Items,
    commands: &NetCommands,
) -> String {
    let Some(d) = spells.catalog.get(spell_id) else {
        return String::new();
    };
    if let Some(text) = d.description.as_deref().filter(|t| !t.is_empty()) {
        let global = |key: &str| wrathplus_ui::strings::global(script.lua(), key);
        let ctx = wrathplus_formats::TokenContext {
            durations: &spells.durations,
            radii: &spells.radii,
            ranges: Some(&spells.ranges),
            skill: &|id| crate::spell::spell_skill_value(me, Some(skill_lines), id),
            lookup: &|id| spells.catalog.get(id),
            mods: None,
            unmodified_points: false,
            home_area: &|| None,
            global: &global,
            printf: &crate::ui_script::token_printf,
        };
        let mut out = wrathplus_formats::substitute(text, d, &ctx);
        let mut end = out.len().min(1_023);
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
        return out;
    }
    let item = d.effect_item_type[0];
    if d.attributes & SPELL_ATTR_IS_TRADESKILL == 0
        || !matches!(d.effects[0], 24 | 59 | 157)
        || item == 0
    {
        return String::new();
    }
    items
        .template(item, 0, commands)
        .map(|t| t.description.clone())
        .unwrap_or_default()
}

/// `GetTrainerServiceItemLink` (`0x596290`): at a tradeskill trainer, a spell whose first effect
/// is `CREATE_ITEM` (24), `CREATE_RANDOM_ITEM` (59) or `CREATE_ITEM_2` (157) links its item,
/// once the template is cached (the link's level field is 0, as `0x61e360` writes it).
pub(super) fn service_item_link(
    trainer_type: u32,
    display: Option<&wrathplus_formats::SpellDisplay>,
    items: &Items,
    commands: &NetCommands,
) -> Option<String> {
    let d = display.filter(|_| trainer_type == TRAINER_TYPE_TRADESKILL)?;
    let item = d.effect_item_type[0];
    if !matches!(d.effects[0], 24 | 59 | 157) || item == 0 {
        return None;
    }
    let t = items.template(item, 0, commands)?;
    Some(crate::items::wrath_item_link(
        item, &t.name, t.quality, [0; 4], 0, 0, 0,
    ))
}
