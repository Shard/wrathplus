//! Actual authored observed-unit addon coverage for the bounded Wrath host.

use benilla_ui::script::{ScriptDialect, ScriptValue, UiScript, UnitState};

use super::load_bootstrap;

fn unit(name: Option<&str>, guid: u64, health: u32, max_health: u32, level: u32) -> UnitState {
    UnitState {
        exists: true,
        name: name.map(str::to_owned),
        guid,
        health,
        max_health,
        level,
        ..Default::default()
    }
}

fn load_units() -> UiScript {
    let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    script.set_screen_size(1024.0, 768.0);
    assert!(load_bootstrap(&script).is_empty());
    script
        .run_addon_chunk_named(
            "AzerothBenchUnits",
            include_bytes!("../../../../addons/AzerothBenchUnits/Units.lua"),
            "@Interface\\AddOns\\AzerothBenchUnits\\Units.lua",
        )
        .unwrap();
    let document = benilla_ui::framexml::parse(include_str!(
        "../../../../addons/AzerothBenchUnits/Units.xml"
    ))
    .unwrap();
    let report = benilla_ui::loader::load_in_addon(
        &script,
        &document,
        r"Interface\AddOns\AzerothBenchUnits\Units.xml",
        &|_| None,
        "AzerothBenchUnits",
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(
        report.missing_files.is_empty(),
        "{:?}",
        report.missing_files
    );
    script
}

#[test]
fn actual_units_lua_xml_tracks_observed_player_and_local_target() {
    let mut script = load_units();
    script.set_unit("player", Some(unit(Some("Benchone"), 0x1234, 80, 100, 7)));
    script.set_unit("target", Some(unit(None, 0x5678, 45, 60, 9)));
    script.fire_event("PLAYER_ENTERING_WORLD", vec![]);

    let texts = script.eval::<(String, String, String, String)>(
        "return AzerothBenchUnitsHeading:GetText(), AzerothBenchUnitsPlayer:GetText(), AzerothBenchUnitsTarget:GetText(), AzerothBenchUnitsQualification:GetText()"
    ).unwrap();
    assert_eq!(
        texts,
        (
            "Observed units".into(),
            "Player: Benchone  HP 80/100  Level 7".into(),
            "Selected target: Unknown Being  HP 45/60  Level 9".into(),
            "Local selection; not target server confirmation".into()
        )
    );

    script.set_unit("player", Some(unit(Some("Benchone"), 0x1234, 73, 100, 7)));
    script.fire_event("UNIT_HEALTH", vec![ScriptValue::Str("player".to_owned())]);
    assert_eq!(
        script
            .eval::<String>("return AzerothBenchUnitsPlayer:GetText()")
            .unwrap(),
        "Player: Benchone  HP 73/100  Level 7"
    );

    script.set_unit("target", Some(unit(None, 0x5678, 30, 70, 9)));
    script.fire_event(
        "UNIT_MAXHEALTH",
        vec![ScriptValue::Str("target".to_owned())],
    );
    assert_eq!(
        script
            .eval::<String>("return AzerothBenchUnitsTarget:GetText()")
            .unwrap(),
        "Selected target: Unknown Being  HP 30/70  Level 9"
    );

    script.set_unit("target", None);
    script.fire_event("PLAYER_TARGET_CHANGED", vec![]);
    assert_eq!(
        script
            .eval::<String>("return AzerothBenchUnitsTarget:GetText()")
            .unwrap(),
        "Selected target: none"
    );
}

#[test]
fn logout_invalidates_display_while_retaining_host_snapshot_for_reentry() {
    let mut script = load_units();
    script.set_unit("player", Some(unit(Some("Benchone"), 0x1234, 80, 100, 7)));
    script.fire_event("PLAYER_ENTERING_WORLD", vec![]);
    script.fire_event("PLAYER_LOGOUT", vec![]);

    assert_eq!(
        script
            .eval::<String>("return AzerothBenchUnitsPlayer:GetText()")
            .unwrap(),
        "Player: unavailable"
    );
    assert!(script
        .eval::<bool>("return UnitExists('player') and UnitHealth('player') == 80")
        .unwrap());

    script.set_unit("player", Some(unit(Some("Benchone"), 0x1234, 65, 100, 7)));
    script.tick(0.30);
    assert_eq!(
        script
            .eval::<String>("return AzerothBenchUnitsPlayer:GetText()")
            .unwrap(),
        "Player: unavailable"
    );

    script.fire_event("PLAYER_ENTERING_WORLD", vec![]);
    assert_eq!(
        script
            .eval::<String>("return AzerothBenchUnitsPlayer:GetText()")
            .unwrap(),
        "Player: Benchone  HP 65/100  Level 7"
    );
    script.fire_event("PLAYER_LEAVING_WORLD", vec![]);
    assert_eq!(
        script
            .eval::<String>("return AzerothBenchUnitsPlayer:GetText()")
            .unwrap(),
        "Player: unavailable"
    );
}

#[test]
fn bounded_timer_recovers_missed_observations_without_exporting_control_state() {
    let mut script = load_units();
    script.set_unit("player", Some(unit(Some("Benchone"), 0x1234, 80, 100, 7)));
    script.fire_event("PLAYER_ENTERING_WORLD", vec![]);
    script.set_unit("player", Some(unit(Some("Benchone"), 0x1234, 61, 100, 7)));

    script.tick(0.24);
    assert!(script
        .eval::<bool>(
            "return string.find(AzerothBenchUnitsPlayer:GetText(), 'HP 80/100', 1, true) ~= nil"
        )
        .unwrap());
    script.tick(0.02);
    assert!(script
        .eval::<bool>(
            "return string.find(AzerothBenchUnitsPlayer:GetText(), 'HP 61/100', 1, true) ~= nil"
        )
        .unwrap());

    assert!(script
        .eval::<bool>(
            "return AzerothBenchUnits == nil and SetTarget == nil and TargetUnit == nil \
             and AttackTarget == nil and CastSpell == nil and SaveBindings == nil",
        )
        .unwrap());
    assert!(script
        .eval::<bool>(
            "return AzerothBenchUnitsFrame:GetLeft() == 16 \
             and AzerothBenchUnitsFrame:GetTop() == 678 \
             and AzerothBenchUnitsFrame:GetWidth() == 300 \
             and AzerothBenchUnitsFrame:GetHeight() == 100",
        )
        .unwrap());
}
