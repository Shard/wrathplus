//! The trainer's state re-evaluator, 12340's `0x596b00`: on each of its triggers (a spell learned
//! or removed, the pet's book, a skill or money edge) it re-derives every service's state from the
//! player's book, skills and level, so the wire state lasts only until the first, then the tree is
//! recounted and `TRAINER_UPDATE` fires. Nothing else moves a row: `SMSG_TRAINER_BUY_SUCCEEDED` has
//! no handler (the NPC dispatcher `0x6d8410` takes `0x1b1` and `0x1b4` only, registered at
//! `0x6e8679`), and `BuyTrainerService` (`0x594da0`) only sends the buy.

use std::collections::BTreeSet;

use wrathplus_formats::{
    LearnEffect, SkillLineCatalog, SpellCatalog, SPELL_EFFECT_LEARN_SPELL, SPELL_EFFECT_SKILL_STEP,
};
use wrathplus_protocol::types::trainer_spell_state::{GRAY, GREEN, RED};
use wrathplus_protocol::types::TrainerSpell;

/// One `PLAYER_SKILL_INFO` slot: the permanent bonus adds only to a non-zero value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SkillSlot {
    pub skill_id: u32,
    pub step: u32,
    pub value_plus_perm: i32,
}

/// Everything `0x596b00` reads off the player, gathered once per re-evaluation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PlayerView {
    /// The player's known-spell set (`[0xbe8dc4]`'s bitmap, `0x7260e0`).
    pub known: BTreeSet<u32>,
    pub level: u32,
    /// The occupied `PLAYER_SKILL_INFO` slots.
    pub skills: Vec<SkillSlot>,
}

impl PlayerView {
    fn skill(&self, skill_id: u32) -> Option<&SkillSlot> {
        self.skills.iter().find(|s| s.skill_id == skill_id)
    }
}

/// `0x7260e0 || 0x726160`: known, or known at a higher rank along `SkillLineAbility`'s chain.
fn known_or_higher(id: u32, known: &BTreeSet<u32>, skill_lines: &SkillLineCatalog) -> bool {
    known.contains(&id) || skill_lines.higher_rank_known(id, known)
}

/// One row of `0x596b00`: a wire spell with no `Spell.dbc` record keeps its wire state; every
/// other restarts at 0 (available).
/// 1. Each effect counts. A `SKILL_STEP` (44) is known when the player's slot for its line holds
///    the step; a `LEARN_SPELL` (36) when its taught spell is known. Any other effect, with no
///    learn effect before it, stands for the service spell itself, which is checked, and the walk
///    ends. So a plain spell a trainer lists goes used once it is in the book.
/// 2. Every counted effect known: used (2).
/// 3. Still available: a skill slot below the requirement is unavailable; no slot writes nothing.
/// 4. Still available: a missing required ability, else a level requirement above 1 the player
///    lacks, is unavailable.
///
/// Labelled, not built: "before it" read as "anywhere in the record" (a later learn effect still
/// suppresses the self-check); the step leg's second test (`[+0x20]`'s `SKILL_STEP` against the
/// slot, `0x596de8`); the `0x53be30` list membership that reads used; and a learn effect aimed at
/// the pet (target 5), which the reference walks as "any other" effect.
pub(super) fn re_derive(
    wire: &TrainerSpell,
    spells: &SpellCatalog,
    skill_lines: &SkillLineCatalog,
    player: &PlayerView,
) -> u8 {
    let Some(record) = spells.get(wire.spell) else {
        return wire.state;
    };
    let mut state = GREEN;
    let (mut counted, mut known) = (0u32, 0u32);
    let mut learn_seen = false;
    for effect in spells.learn_effects(wire.spell) {
        match *effect {
            LearnEffect::Spell(t) => {
                learn_seen = true;
                counted += 1;
                known += u32::from(known_or_higher(t, &player.known, skill_lines));
            }
            LearnEffect::SkillStep { skill, step } => {
                counted += 1;
                known += u32::from(player.skill(skill).is_some_and(|s| s.step >= step));
            }
            // 1.12's pet learn is "any other" effect here, folded into the arm below.
            LearnEffect::PetSpell(_) => {}
        }
    }
    let other = record
        .effects
        .iter()
        .any(|&e| e != 0 && e != SPELL_EFFECT_LEARN_SPELL && e != SPELL_EFFECT_SKILL_STEP);
    if other && !learn_seen {
        counted += 1;
        known += u32::from(known_or_higher(wire.spell, &player.known, skill_lines));
    }
    if counted > 0 && known == counted {
        state = GRAY;
    }
    if state == GREEN && wire.req_skill != 0 {
        if let Some(slot) = player.skill(wire.req_skill) {
            if slot.value_plus_perm < wire.req_skill_value as i32 {
                state = RED;
            }
        }
    }
    if state == GREEN {
        let missing = wire
            .req_spells
            .iter()
            .any(|&id| id != 0 && !known_or_higher(id, &player.known, skill_lines));
        if missing || (wire.req_level > 1 && player.level < u32::from(wire.req_level)) {
            state = RED;
        }
    }
    state
}

/// Every row in place; the group recount is the engine's.
pub(super) fn re_derive_all(
    services: &mut [TrainerSpell],
    spells: &SpellCatalog,
    skill_lines: &SkillLineCatalog,
    player: &PlayerView,
) {
    for wire in services.iter_mut() {
        wire.state = re_derive(wire, spells, skill_lines, player);
    }
}
