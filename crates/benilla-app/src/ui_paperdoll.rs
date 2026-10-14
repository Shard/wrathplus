//! Wrath character-sheet values from semantic player fields, not reconstructed stats.
use crate::net::{ObjectStore, SelfPlayer};
use benilla_protocol::messages::{Field, ObjectFields};
use benilla_ui::script::{UiScript, WrathPaperDollStats};
use bevy::prelude::*;

pub(crate) struct UiPaperDollPlugin;
impl Plugin for UiPaperDollPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, feed.in_set(crate::ui_unit::UnitFeed));
    }
}

fn snapshot(f: &ObjectFields) -> WrathPaperDollStats {
    let float = |field| f.get_float(field, 0).unwrap_or(0.0);
    let word = |field| f.get(field).unwrap_or(0);
    WrathPaperDollStats {
        ratings: std::array::from_fn(|i| {
            f.get_at(Field::PlayerCombatRating1, i as u16).unwrap_or(0)
        }),
        dodge: float(Field::PlayerDodgePercentage),
        parry: float(Field::PlayerParryPercentage),
        block: float(Field::PlayerBlockPercentage),
        melee_crit: float(Field::PlayerCritPercentage),
        ranged_crit: float(Field::PlayerRangedCritPercentage),
        spell_crit: std::array::from_fn(|i| {
            f.get_float(Field::PlayerSpellCritPercentage1, i as u16)
                .unwrap_or(0.0)
        }),
        spell_damage: std::array::from_fn(|i| {
            // Positive and negative school bonuses are signed descriptor values.
            let pos = f
                .get_at(Field::PlayerModDamageDonePos, i as u16)
                .unwrap_or(0) as i32;
            let neg = f
                .get_at(Field::PlayerModDamageDoneNeg, i as u16)
                .unwrap_or(0) as i32;
            pos.saturating_add(neg).max(0)
        }),
        spell_healing: word(Field::PlayerModHealingDonePos),
        spell_penetration: (word(Field::PlayerModTargetResistance) as i32).saturating_neg(),
        shield_block: word(Field::PlayerShieldBlock),
        expertise: [
            word(Field::PlayerExpertise),
            word(Field::PlayerOffhandExpertise),
        ],
        // Mana is power slot zero; descriptor rates are mana/second, not mana/5.
        mana_regen: [
            float(Field::UnitPowerRegenFlatModifier),
            float(Field::UnitPowerRegenInterruptedFlatModifier),
        ],
        // No fabricated rating/primary-stat ratios while GameTables are unavailable.
        conversions: Default::default(),
    }
}

fn feed(script: Option<NonSendMut<UiScript>>, player: Query<&ObjectStore, With<SelfPlayer>>) {
    let Some(mut script) = script else { return };
    if script.dialect() != benilla_ui::script::ScriptDialect::Wrath12340 {
        return;
    }
    script.set_wrath_paperdoll_stats(
        player
            .iter()
            .next()
            .map(|p| snapshot(&p.0))
            .unwrap_or_default(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::messages::FieldLayout;
    #[test]
    fn named_wrath_fields_feed_values_and_signed_school_bonuses() {
        let layout = FieldLayout::Wrath12340;
        let data = [
            (Field::PlayerCombatRating1, 24, 177),
            (Field::PlayerSpellCritPercentage1, 6, 12.5f32.to_bits()),
            (Field::PlayerCritPercentage, 0, 8.25f32.to_bits()),
            (Field::PlayerDodgePercentage, 0, 4.25f32.to_bits()),
            (Field::PlayerModDamageDonePos, 2, 100),
            (Field::PlayerModDamageDoneNeg, 2, (-20i32) as u32),
            (Field::PlayerModTargetResistance, 0, (-35i32) as u32),
            (Field::PlayerExpertise, 0, 12),
            (Field::PlayerOffhandExpertise, 0, 9),
            (Field::PlayerShieldBlock, 0, 234),
            (Field::PlayerModHealingDonePos, 0, 345),
            (Field::UnitPowerRegenFlatModifier, 0, 2.5f32.to_bits()),
            (
                Field::UnitPowerRegenInterruptedFlatModifier,
                0,
                1.25f32.to_bits(),
            ),
        ];
        let pairs: Vec<_> = data
            .iter()
            .map(|&(field, offset, value)| (layout.resolve(field).unwrap().index + offset, value))
            .collect();
        let stats = snapshot(&ObjectFields::from_native_pairs(layout, &pairs));
        assert_eq!(stats.ratings[24], 177);
        assert_eq!(stats.spell_crit[6], 12.5);
        assert_eq!(stats.melee_crit, 8.25);
        assert_eq!(stats.dodge, 4.25);
        assert_eq!(stats.spell_damage[2], 80);
        assert_eq!(stats.spell_penetration, 35);
        assert_eq!(stats.expertise, [12, 9]);
        assert_eq!(stats.shield_block, 234);
        assert_eq!(stats.spell_healing, 345);
        assert_eq!(stats.mana_regen, [2.5, 1.25]);
        assert_eq!(stats.conversions, Default::default());
    }
    #[test]
    fn feed_clears_logout_and_reseeds_a_new_vm() {
        use benilla_ui::script::ScriptDialect;
        let layout = FieldLayout::Wrath12340;
        let index = layout.resolve(Field::PlayerCritPercentage).unwrap().index;
        let mut app = App::new();
        app.add_plugins(UiPaperDollPlugin);
        app.insert_non_send_resource(
            UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap(),
        );
        let player = app
            .world_mut()
            .spawn((
                SelfPlayer,
                ObjectStore(ObjectFields::from_native_pairs(
                    layout,
                    &[(index, 8.25f32.to_bits())],
                )),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world()
                .non_send_resource::<UiScript>()
                .eval::<f64>("return GetCritChance()")
                .unwrap(),
            8.25
        );
        app.insert_non_send_resource(
            UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap(),
        );
        app.update();
        assert_eq!(
            app.world()
                .non_send_resource::<UiScript>()
                .eval::<f64>("return GetCritChance()")
                .unwrap(),
            8.25
        );
        app.world_mut().despawn(player);
        app.update();
        assert_eq!(
            app.world()
                .non_send_resource::<UiScript>()
                .eval::<f64>("return GetCritChance()")
                .unwrap(),
            0.0
        );
    }
}
