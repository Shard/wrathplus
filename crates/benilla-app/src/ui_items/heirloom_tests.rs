use super::*;
use benilla_ui::script::{ItemTemplateView, PlayerReqState, UiScript};

/// A scaling template's event is kept and queued for a re-push; an unscaled one is not.
#[test]
fn a_masked_scaling_is_kept_and_queued() {
    use bevy::ecs::system::RunSystemOnce;
    let mut world = World::new();
    world.init_resource::<ItemScalings>();
    for ev in [
        SessionEvent::WrathItemScaling(7, Some([1, 0x21])),
        SessionEvent::WrathItemScaling(8, Some([0, 0])),
        SessionEvent::WrathItemScaling(9, None),
    ] {
        world.run_system_once_with(on_scaling, ev).unwrap();
    }
    let mut s = world.resource_mut::<ItemScalings>();
    assert_eq!(s.by_entry.get(&7), Some(&[1, 0x21]));
    assert!(!s.by_entry.contains_key(&8) && !s.by_entry.contains_key(&9));
    assert_eq!(s.take_fresh(), vec![7]);
    assert!(s.take_fresh().is_empty());
}

/// An heirloom shoulder over the installed tables (`ScalingStatDistribution` 1: strength,
/// stamina, crit; the shoulder budget, cloth-shoulder armor and spell power), through the tooltip
/// and `GetItemStats` at levels 60 and 80.
#[test]
fn installed_heirloom_tooltip_and_stats_at_two_levels() {
    let data = benilla_formats::wow_data_or_skip!();
    let mut chain = benilla_formats::Chain::open_wrath(&data, "enUS").expect("read-only chain");
    let cat = benilla_formats::heirlooms::load_heirloom_catalog(&mut chain).unwrap();
    let view = heirloom_view(&cat, [1, 0x1 | 0x20 | 0x8000]).unwrap();
    assert_eq!((view.max_level, view.levels.len()), (80, 80));

    let mut s = UiScript::new().unwrap();
    s.run(
        r#"
        ITEM_MOD_STRENGTH = "%c%d Strength"; ITEM_MOD_STAMINA = "%c%d Stamina"
        ARMOR_TEMPLATE = "%d Armor"; ITEM_SPELL_TRIGGER_ONEQUIP = "Equip:"
        ITEM_MOD_CRIT_RATING = "Improves critical strike rating by %d."
        ITEM_MOD_SPELL_POWER = "Increases spell power by %d."
        local a = CreateFrame("Button", "Slot"); a:SetPoint("CENTER", 0, 0)
        TT = CreateFrame("GameTooltip", "TT")
        TT:SetOwner(a, "ANCHOR_RIGHT")
    "#,
    )
    .unwrap();
    s.set_item_template(
        42985,
        ItemTemplateView {
            name: "Tattered Dreadmist Mantle".into(),
            inventory_type: 3,
            armor: 1,
            required_level: 1,
            heirloom: Some(view),
            ..Default::default()
        },
    );
    for (level, want) in [
        (
            60,
            [
                "88 Armor",
                "+19 Strength",
                "+29 Stamina",
                "Equip: Improves critical strike rating by 19.",
                "Equip: Increases spell power by 57.",
            ],
        ),
        (
            80,
            [
                "210 Armor",
                "+51 Strength",
                "+76 Stamina",
                "Equip: Improves critical strike rating by 51.",
                "Equip: Increases spell power by 343.",
            ],
        ),
    ] {
        s.set_player_req_state(PlayerReqState {
            level,
            ..Default::default()
        });
        s.run("TT:SetHyperlink('item:42985')").unwrap();
        let n: i64 = s.eval("return TT:NumLines()").unwrap();
        let lines: Vec<String> = (1..=n)
            .map(|i| {
                s.eval(&format!(
                    "return getglobal('TTTextLeft{i}'):GetText() or ''"
                ))
                .unwrap()
            })
            .collect();
        for line in want {
            assert!(
                lines.iter().any(|l| l == line),
                "{line} at {level}: {lines:?}"
            );
        }
        let (sta, crit, armor, sp): (i64, i64, i64, i64) = s
            .eval(&format!(
                "local t = GetItemStats('item:42985:0:0:0:0:0:0:0:{level}') return \
                 t.ITEM_MOD_STAMINA_SHORT, t.ITEM_MOD_CRIT_RATING_SHORT, t.RESISTANCE0_NAME, \
                 t.ITEM_MOD_SPELL_POWER_SHORT"
            ))
            .unwrap();
        let want = if level == 60 {
            (29, 19, 88, 57)
        } else {
            (76, 51, 210, 343)
        };
        assert_eq!((sta, crit, armor, sp), want, "GetItemStats at {level}");
    }
}
