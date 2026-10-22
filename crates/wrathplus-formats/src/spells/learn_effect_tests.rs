use super::*;

/// A trainer's wrapper carries the learn effect and the step `0x4d7d40` compares.
#[test]
fn the_openers_skill_steps_read_off_the_real_file() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_spell_catalog(&mut chain).expect("Spell.dbc");
    let blacksmithing = 164;
    for (wrapper, taught, step) in [
        (2020, 2018, 1),
        (2021, 3100, 2),
        (3539, 3538, 3),
        (9786, 9785, 4),
    ] {
        assert_eq!(
            cat.learn_effects(wrapper),
            &[
                LearnEffect::Spell(taught),
                LearnEffect::SkillStep {
                    skill: blacksmithing,
                    step
                }
            ],
            "wrapper {wrapper}"
        );
    }
    // Mining's opener: base −1 + die 1 on the LEARN slot is not a step; the SKILL_STEP slot is.
    assert_eq!(
        cat.learn_effects(2581),
        &[
            LearnEffect::Spell(2575),
            LearnEffect::SkillStep {
                skill: 186,
                step: 1
            }
        ]
    );
    // A plain ability teaches nothing.
    assert!(cat.learn_effects(2383).is_empty(), "Find Herbs");
}
