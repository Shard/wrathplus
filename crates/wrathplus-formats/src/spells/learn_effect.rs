//! The learn effects a trainer's state re-evaluator reads.

/// `SPELL_EFFECT_LEARN_SPELL`: teaches its `EffectTriggerSpell`, as a trainer's wrapper does.
pub const SPELL_EFFECT_LEARN_SPELL: u32 = 36;

/// One learn effect, in the slot order the trainer's state re-evaluator reads (`0x4d7d40`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearnEffect {
    /// `SPELL_EFFECT_LEARN_SPELL`: teaches `EffectTriggerSpell`.
    Spell(u32),
    /// `SPELL_EFFECT_SKILL_STEP`: raises skill line `skill` (`EffectMiscValue`) to `step`,
    /// `EffectBasePoints + EffectDieSides`.
    SkillStep { skill: u32, step: u32 },
    /// `SPELL_EFFECT_LEARN_PET_SPELL`: teaches the pet `EffectTriggerSpell`.
    PetSpell(u32),
}
/// `SPELL_EFFECT_LEARN_PET_SPELL`: the trainer's icon accepts it or 36 in any slot (`0x4d8ff5`),
/// but [`super::SpellCatalog::learned_spell`] follows 36 only.
pub const SPELL_EFFECT_LEARN_PET_SPELL: u32 = 57;
/// `SPELL_EFFECT_SKILL_STEP`: raises a profession's cap; a tradeskill trainer's tree partitions
/// on it alone, in the wire spell's three slots (`0x4d77b6`).
pub const SPELL_EFFECT_SKILL_STEP: u32 = 44;
