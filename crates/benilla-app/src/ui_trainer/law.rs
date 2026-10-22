//! Build 12340's trainer window laws: the icon (`GetTrainerServiceIcon 00595150`) reads the
//! service spell itself, where the tooltip (`SetTrainerService 0062e900`) hops through a learn
//! effect to the taught spell, so the two can name different spells on one row. Do not unify them.

use benilla_formats::{
    SkillLineCatalog, SpellCatalog, SpellDisplay, SPELL_ATTR_IS_TRADESKILL,
    SPELL_EFFECT_LEARN_PET_SPELL, SPELL_EFFECT_LEARN_SPELL, SPELL_EFFECT_SKILL_STEP,
};
use benilla_protocol::types::trainer_spell_state;
use benilla_ui::script::{TrainerServiceCategory, TrainerTooltip, TRAINER_GROUP_KNOWN};

use crate::entities::ItemDisplays;
use crate::items::Items;
use crate::net::NetCommands;
use crate::ui_items::item_icon;

use super::{TRAINER_TYPE_MOUNT, TRAINER_TYPE_TRADESKILL};

/// `Effect[0]` creating an item: `CREATE_ITEM` (24), `CREATE_ITEM_2` (157) or
/// `CREATE_RANDOM_ITEM` (59), the item arm of the icon, tooltip and description.
fn creates_item(d: &SpellDisplay) -> bool {
    matches!(d.effects[0], 24 | 157 | 59)
}

/// `GetTrainerServiceIcon 00595150`: at a tradeskill trainer (`[0xc0e498] == 2`), a service
/// spell whose `Effect[0]` creates an item shows that item's icon; otherwise the spell's own
/// `SpellIconID`. Labelled: the reference reads the item's display id off the client's `Item.dbc`
/// (`0065c290`); the template cache stands in, `None` until its answer re-fires `TRAINER_UPDATE`.
/// Unlike 1.12, no learn effect is followed: 3.3.5 lists the taught spell itself.
pub(super) fn service_icon(
    wire_spell: u32,
    trainer_type: u32,
    spells: &SpellCatalog,
    icons: Option<&ItemDisplays>,
    items: &Items,
    commands: &NetCommands,
) -> Option<String> {
    let wire = spells.get(wire_spell)?;
    if trainer_type == TRAINER_TYPE_TRADESKILL && creates_item(wire) {
        return items
            .template(wire.effect_item_type[0], 0, commands)
            .map(|t| t.display_info_id)
            .and_then(|d| item_icon(icons, d));
    }
    wire.icon.clone()
}

/// `SetTrainerService 0062e900` picks the subject the tooltip builders describe: the first learn
/// effect (36, or 57 for the pet caster) whose taught spell resolves; an unresolvable one moves to
/// the next slot. The taught spell shows as its item when it is a trade spell whose `Effect[0]`
/// creates a nonzero `EffectItemType[0]`, else as itself; with no learn effect, the service spell.
pub(super) fn service_tooltip(wire_spell: u32, spells: &SpellCatalog) -> TrainerTooltip {
    let wire_only = TrainerTooltip::Spell {
        spell_id: wire_spell,
        alt_caster: false,
    };
    let Some(wire) = spells.get(wire_spell) else {
        return wire_only;
    };
    for i in 0..3 {
        let effect = wire.effects[i];
        if effect != SPELL_EFFECT_LEARN_SPELL && effect != SPELL_EFFECT_LEARN_PET_SPELL {
            continue;
        }
        let trigger = wire.effect_trigger_spell[i];
        let Some(taught) = spells.get(trigger) else {
            continue;
        };
        let item = taught.effect_item_type[0];
        if taught.attributes & SPELL_ATTR_IS_TRADESKILL != 0 && creates_item(taught) && item != 0 {
            return TrainerTooltip::Item(item);
        }
        return TrainerTooltip::Spell {
            spell_id: trigger,
            alt_caster: effect == SPELL_EFFECT_LEARN_PET_SPELL,
        };
    }
    wire_only
}

/// The list builder's group key. Labelled: transcribed from 1.12's `0x4d7560` (`0x4d7786`);
/// 12340's builder (the writer of `[0xc0e4bc]`) is not traced. Its readers agree: a header row's
/// name is the `[0xa13e08]` key table at a tradeskill trainer, else the `SkillLine.dbc` name
/// (`00594e80`). Type 2 resolves no skill line, so no row
/// drops: 1 when the wire spell steps a skill (`0x4d77b6`), else 2, labelled from the table at
/// `0x807520`. Type 1 puts a known service in the signed `-1` group (`0x4d77e8`). The rest key on
/// the taught spell's skill line (`0x4d7c60` → `0x60c920`), and 0 drops the service (`0x4d7807`).
pub(super) fn service_group(
    wire_spell: u32,
    taught: u32,
    trainer_type: u32,
    category: TrainerServiceCategory,
    spells: &SpellCatalog,
    skill_lines: Option<&SkillLineCatalog>,
    // The VM's own `GlobalStrings.lua`, for the header labels.
    get: &dyn Fn(&str) -> Option<String>,
) -> (u32, String) {
    if trainer_type == TRAINER_TYPE_TRADESKILL {
        let step = spells
            .get(wire_spell)
            .is_some_and(|d| d.effects.contains(&SPELL_EFFECT_SKILL_STEP));
        return if step {
            (1, get("TRADESKILL_SERVICE_STEP").unwrap_or_default())
        } else {
            (2, get("TRADESKILL_SERVICE_LEARN").unwrap_or_default())
        };
    }
    if trainer_type == TRAINER_TYPE_MOUNT && category == TrainerServiceCategory::Used {
        return (
            TRAINER_GROUP_KNOWN,
            get("KNOWN_TALENTS_HEADER").unwrap_or_default(),
        );
    }
    let line = skill_lines
        .and_then(|c| c.spell_to_line(taught))
        .unwrap_or(0);
    if line == 0 {
        return (0, String::new());
    }
    let name = skill_lines
        .and_then(|c| c.line(line))
        .map(|l| l.name.clone())
        .unwrap_or_else(|| format!("Skill {line}"));
    (line, name)
}

/// A wire `state` byte's category (`00594260`): 0 "available", 2 "used", anything else
/// "unavailable".
pub(super) fn category(state: u8) -> TrainerServiceCategory {
    match state {
        trainer_spell_state::GRAY => TrainerServiceCategory::Used,
        trainer_spell_state::GREEN => TrainerServiceCategory::Available,
        _ => TrainerServiceCategory::Unavailable,
    }
}
