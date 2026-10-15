//! Wrath static achievement metadata. This is not a player-progress store.
use benilla_assets::{AssetSet, LockRecover, WorldAssets};
use benilla_ui::script::{
    UiScript, WrathAchievement, WrathAchievementCriterion, WrathAchievementMetadata,
};
use bevy::prelude::*;

#[derive(Resource)]
struct AchievementMetadata(WrathAchievementMetadata);

pub(crate) struct UiAchievementsPlugin;
impl Plugin for UiAchievementsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load.after(AssetSet::Open));
    }
}

fn load(mut commands: Commands, assets: Option<Res<WorldAssets>>) {
    let Some(assets) = assets else { return };
    let mut chain = assets.chain.lock_recover();
    if !matches!(chain.profile(), benilla_formats::DataProfile::Wrath { .. }) {
        return;
    }
    use benilla_formats::achievements::{load_achievement_criteria, load_achievements};
    match (
        load_achievements(&mut chain),
        load_achievement_criteria(&mut chain),
    ) {
        (Ok(achievements), Ok(criteria)) => {
            let mut metadata = WrathAchievementMetadata::default();
            for row in achievements.iter() {
                metadata.achievements.insert(
                    row.id,
                    WrathAchievement {
                        id: row.id,
                        name: row.name.clone(),
                        category_id: row.category_id,
                        flags: row.flags,
                        required_criteria_count: row.required_criteria_count,
                        reference_achievement_id: row.reference_achievement_id,
                    },
                );
                metadata.linked_criteria.insert(
                    row.id,
                    criteria.for_achievement(row.id).map(|row| row.id).collect(),
                );
            }
            for row in criteria.iter() {
                metadata.criteria.insert(
                    row.id,
                    WrathAchievementCriterion {
                        id: row.id,
                        achievement_id: row.achievement_id,
                        criteria_type: row.criteria_type,
                        asset_id: row.asset_id,
                        required_quantity: row.required_quantity,
                        description: row.description.clone(),
                        flags: row.flags,
                    },
                );
            }
            commands.insert_resource(AchievementMetadata(metadata));
        }
        (achievements, criteria) => warn!(
            "achievement metadata unavailable: achievements={:?}, criteria={:?}",
            achievements.err(),
            criteria.err()
        ),
    }
}

/// One startup handoff for each fresh in-game VM, before any FrameXML executes.
pub(crate) fn seed(world: &World, script: &mut UiScript) {
    if let Some(metadata) = world.get_resource::<AchievementMetadata>() {
        script.set_wrath_achievement_metadata(metadata.0.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_ui::script::ScriptDialect;

    #[test]
    fn static_snapshot_is_available_to_each_fresh_vm_before_ui_execution() {
        let mut world = World::new();
        let mut metadata = WrathAchievementMetadata::default();
        metadata.achievements.insert(
            1,
            WrathAchievement {
                id: 1,
                name: "Metadata only".into(),
                category_id: 42,
                flags: 0,
                required_criteria_count: 0,
                reference_achievement_id: 0,
            },
        );
        for id in [12, 10] {
            metadata.criteria.insert(
                id,
                WrathAchievementCriterion {
                    id,
                    achievement_id: 1,
                    criteria_type: 0,
                    asset_id: 7,
                    required_quantity: 8,
                    description: format!("criterion {id}"),
                    flags: 0,
                },
            );
        }
        // Preserve raw linked order, not the criterion-ID map's sorted order.
        metadata.linked_criteria.insert(1, vec![12, 10]);
        world.insert_resource(AchievementMetadata(metadata));
        for _ in 0..2 {
            let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
            seed(&world, &mut script);
            script.run("assert(GetAchievementCategory(1)==42); assert(GetAchievementNumCriteria(1)==2); local d,t,c,q,r,n,f,a,s,id=GetAchievementCriteriaInfo(1,1); assert(id==12 and c==false and q==0 and r==8 and n==nil)").unwrap();
        }
        let mut vanilla = UiScript::new().unwrap();
        seed(&world, &mut vanilla);
        vanilla.run("assert(GetAchievementCategory==nil)").unwrap();
    }

    #[test]
    fn absent_catalog_is_not_replaced_with_fabricated_metadata() {
        let world = World::new();
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        seed(&world, &mut script);
        script.run("assert(select('#',GetAchievementCategory(1))==0); assert(select('#',GetAchievementCriteriaInfo(1,1))==0)").unwrap();
    }
}
