//! Bounded Wrath 12340 addon host layered beside the native safety overlay.

use benilla_ui::script::UiScript;
use bevy::prelude::*;

use super::{UiFeed, UiInput, UiPaint};

/// Per-frame allowance for addon OnUpdate and input handlers.
pub(super) const RUNTIME_INSTRUCTION_BUDGET: u64 = 20_000_000;

pub(super) fn build(app: &mut App) {
    use crate::console::ConsoleCommandApp;

    app.add_observer(super::on_cvar);
    app.console_command(
        "reloadUI",
        "Reload the interface.",
        super::console_reload_ui,
    );
    crate::shutdown::on_app_exit(
        app,
        shutdown_on_exit
            .before(crate::cvars::save_config_on_exit)
            .into_configs(),
    );
    app.init_resource::<super::AddOnIdentity>()
        .init_resource::<super::LeavingWorldArmed>()
        .add_systems(
            Startup,
            super::setup_script.after(benilla_assets::AssetSet::Open),
        )
        .add_systems(
            OnEnter(crate::char_select::ClientState::InWorld),
            super::lifecycle::arm_entry_ui_load,
        )
        .add_systems(
            OnExit(crate::char_select::ClientState::InWorld),
            super::end_ui_session,
        )
        .add_systems(Update, super::lifecycle::arm_leaving_world_on_self_create)
        .add_systems(
            PreUpdate,
            (
                super::run_pending_reload,
                super::lifecycle::run_pending_entry_load,
                arm_runtime_budget,
            )
                .chain(),
        )
        .add_systems(Update, super::feed_framerate.in_set(UiFeed))
        .add_systems(
            Update,
            (
                super::extract::tick_script,
                super::input::feed_ui_input.in_set(crate::char_select::InWorldGated),
            )
                .chain()
                .in_set(UiInput)
                .before(crate::bindings::BindingSet),
        )
        .add_systems(
            Update,
            super::extract::paint_script
                .in_set(UiPaint)
                .after(crate::vplates::VPlateSet)
                .before(crate::ui_pass::UiQuadAppend),
        )
        .add_systems(
            Update,
            super::arbitrate_pointer_over_ui
                .after(UiInput)
                .before(benilla_world::schedule::WorldStage::Input),
        );
}

fn arm_runtime_budget(script: Option<NonSend<UiScript>>) {
    if let Some(script) = script {
        script.set_instruction_budget(RUNTIME_INSTRUCTION_BUDGET);
    }
}

fn shutdown_on_exit(
    script: Option<NonSendMut<UiScript>>,
    identity: Res<super::AddOnIdentity>,
    pending_entry: Option<Res<super::PendingEntryUiLoad>>,
    mut armed: ResMut<super::LeavingWorldArmed>,
    mut exits: MessageReader<AppExit>,
    mut cvars: Option<ResMut<crate::cvars::Cvars>>,
) {
    if exits.read().next().is_none() || pending_entry.is_some() {
        return;
    }
    let leaving_world = armed.spend();
    if let Some(mut script) = script {
        shutdown(&mut script, identity.0.as_ref(), leaving_world);
        if let (Some(value), Some(cvars)) =
            (script.persist_wrath_tutorial_history(), cvars.as_mut())
        {
            cvars.set("flaggedTutorials", &value);
        }
    }
}

pub(super) fn load_bootstrap(script: &UiScript) -> Vec<String> {
    let root = super::addons::Addon::wrath_bootstrap();
    root.load_files(script, &root.toc.files)
}

pub(super) fn shutdown(
    script: &mut UiScript,
    identity: Option<&(String, String)>,
    leaving_world: bool,
) {
    script.set_instruction_budget(super::addons::LOAD_INSTRUCTION_BUDGET);
    if leaving_world {
        script.fire_event("PLAYER_LEAVING_WORLD", vec![]);
    }
    script.fire_event("PLAYER_LOGOUT", vec![]);
    crate::ui_layout::save_now(script, identity);
    super::addons::save_addon_variables(script, identity);
    super::addons::save_enable_state(script, identity);
}

#[cfg(test)]
#[path = "wrath_units_tests.rs"]
mod wrath_units_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_observer_lua_xml_lifecycle_observations_and_pointer_click() {
        use benilla_ui::script::{ScriptDialect, ScriptValue, UnitState};
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script.set_instruction_budget(RUNTIME_INSTRUCTION_BUDGET);
        script.set_screen_size(1024.0, 768.0);
        // The stock CVars the Wrath surface registers at birth.
        script.take_cvar_registrations();
        let mut cvars = crate::cvars::Cvars::with_rows(crate::shipped_cvars::FEATURE_ROWS);
        cvars.set("gamma", "1.2");
        script.seed_cvars(cvars.vm_seed());
        assert!(load_bootstrap(&script).is_empty());
        script
            .run_addon_chunk_named(
                "AzerothBenchObserver",
                include_bytes!("../../../../addons/AzerothBenchObserver/Observer.lua"),
                "@Interface\\AddOns\\AzerothBenchObserver\\Observer.lua",
            )
            .unwrap();
        let doc = benilla_ui::framexml::parse(include_str!(
            "../../../../addons/AzerothBenchObserver/Observer.xml"
        ))
        .unwrap();
        let report = benilla_ui::loader::load(&script, &doc, &|_| None);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        script.fire_event(
            "ADDON_LOADED",
            vec![ScriptValue::Str("AzerothBenchObserver".into())],
        );
        script.fire_event("PLAYER_LOGIN", vec![]);
        script.set_unit(
            "player",
            Some(UnitState {
                exists: true,
                name: Some("Benchone".into()),
                guid: 0x1234,
                health: 80,
                max_health: 100,
                level: 7,
                ..Default::default()
            }),
        );
        script.fire_event("PLAYER_ENTERING_WORLD", vec![]);
        assert!(script.eval::<bool>("return AzerothBenchObserverDB.last.name == 'Benchone' and AzerothBenchObserverDB.last.health == 80 and AzerothBenchObserverDB.last.healthMax == 100 and AzerothBenchObserverDB.last.level == 7 and AzerothBenchObserverDB.last.clientBuild == '12340' and AzerothBenchObserverDB.last.playerIsSelf == true").unwrap());
        assert!(script.eval::<bool>("return AzerothBenchObserver.presentationAtLoad.gamma == '1.2' and AzerothBenchObserverDB.last.presentation.gamma == '1.2' and type(AzerothBenchObserverDB.last.presentation.uiScale) == 'string'").unwrap());
        script.set_cvar_host("gamma", "1.3");
        script.tick(5.1);
        assert!(script.eval::<bool>("return AzerothBenchObserverDB.last.presentation.gamma == '1.3' and AzerothBenchObserver.presentationAtLoad.gamma == '1.2'").unwrap());
        assert!(script.take_cvar_changes().is_empty());
        assert!(script.take_cvar_registrations().is_empty());
        script.tick(0.01);
        let (x, y) = script.eval::<(f32, f32)>("local f=AzerothBenchObserverSample; return (f:GetLeft()+f:GetRight())/2, (f:GetTop()+f:GetBottom())/2").unwrap();
        assert!(script.mouse_button(x, y, "LeftButton", true));
        assert!(script.mouse_button(x, y, "LeftButton", false));
        assert!(script.eval::<bool>("return AzerothBenchObserverDB.clicks == 1 and AzerothBenchObserverDB.last.source == 'click'").unwrap());
        script.fire_event("PLAYER_LEAVING_WORLD", vec![]);
        script.fire_event("PLAYER_LOGOUT", vec![]);
        assert!(script.eval::<bool>("local e=AzerothBenchObserverDB.events; return e[#e-1].event == 'PLAYER_LEAVING_WORLD' and e[#e].event == 'PLAYER_LOGOUT'").unwrap());
    }

    #[test]
    fn observer_copies_are_bounded_and_unavailable_outside_live_player_state() {
        use benilla_ui::script::{ScriptDialect, ScriptValue, UnitState};
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script.seed_cvars(
            crate::cvars::Cvars::with_rows(crate::shipped_cvars::FEATURE_ROWS).vm_seed(),
        );
        script
            .run_addon_chunk_named(
                "AzerothBenchObserver",
                include_bytes!("../../../../addons/AzerothBenchObserver/Observer.lua"),
                "@Observer.lua",
            )
            .unwrap();
        script.run("local s,r=AzerothBenchObserver.Snapshot(); assert(s==nil and r=='NOT_READY'); AzerothBenchObserver_OnEvent(nil,'ADDON_LOADED','AzerothBenchObserver'); AzerothBenchObserver_OnEvent(nil,'PLAYER_LOGIN')").unwrap();
        script.set_unit(
            "player",
            Some(UnitState {
                exists: true,
                guid: 123,
                health: 60,
                max_health: 60,
                ..Default::default()
            }),
        );
        // Fixture-seeded unit state verifies addon copy/lifecycle plumbing, not live wire/ECS acceptance.
        script.run("AzerothBenchObserver_OnEvent(nil,'PLAYER_ENTERING_WORLD'); for i=1,40 do AzerothBenchObserver_OnUpdate(nil,5) end; local s=AzerothBenchObserver.Snapshot(); local h=AzerothBenchObserver.History(); assert(#h==32); s.health=999; s.presentation.gamma='mutated'; h[1].presentation.gamma='mutated'; h[#h].health=998; table.remove(h,1); assert(AzerothBenchObserver.Snapshot().health==60); assert(AzerothBenchObserver.Snapshot().presentation.gamma~='mutated'); assert(#AzerothBenchObserver.History()==32); assert(AzerothBenchObserverDB.last.health==60)").unwrap();
        script.run("AzerothBenchObserverDB.last.health=997; AzerothBenchObserverDB.last.presentation.gamma='forged'; assert(AzerothBenchObserver.Snapshot().health==60 and AzerothBenchObserver.Snapshot().presentation.gamma~='forged'); for i=1,100 do AzerothBenchObserver_OnEvent(nil,'UNIT_HEALTH','player') end; assert(#AzerothBenchObserver.events==64)").unwrap();
        script.set_unit("player", None);
        script.run("local s,r=AzerothBenchObserver.Snapshot(); local h,q=AzerothBenchObserver.History(); assert(s==nil and r=='NO_PLAYER' and h==nil and q=='NO_PLAYER')").unwrap();
        script.set_unit(
            "player",
            Some(UnitState {
                exists: true,
                guid: 124,
                health: 40,
                max_health: 60,
                ..Default::default()
            }),
        );
        script.run("local s,r=AzerothBenchObserver.Snapshot(); assert(s==nil and r=='NO_SAMPLE'); assert(#AzerothBenchObserver.History()==0); AzerothBenchObserver_OnUpdate(nil,5); assert(AzerothBenchObserver.Snapshot().health==40 and #AzerothBenchObserver.History()==1)").unwrap();
        script.fire_event(
            "PLAYER_LEAVING_WORLD",
            vec![ScriptValue::Str("unused".into())],
        );
        script.run("AzerothBenchObserver_OnEvent(nil,'PLAYER_LEAVING_WORLD'); local s,r=AzerothBenchObserver.Snapshot(); local h,q=AzerothBenchObserver.History(); assert(s==nil and r=='NOT_READY' and h==nil and q=='NOT_READY')").unwrap();
    }

    #[test]
    fn authored_root_is_wrath_only_and_materializes_uiparent() {
        let script = UiScript::new_with_dialect(benilla_ui::script::ScriptDialect::Wrath12340)
            .expect("Wrath VM");
        let failures = load_bootstrap(&script);
        assert!(failures.is_empty(), "{failures:#?}");
        assert!(script.eval::<bool>("return UIParent ~= nil").unwrap());
    }

    #[test]
    fn shutdown_events_are_ordered_and_bounded() {
        let mut script =
            UiScript::new_with_dialect(benilla_ui::script::ScriptDialect::Wrath12340).unwrap();
        script
            .run(
                "Seen = {}; local f = CreateFrame('Frame'); \
                 f:RegisterEvent('PLAYER_LEAVING_WORLD'); f:RegisterEvent('PLAYER_LOGOUT'); \
                 f:SetScript('OnEvent', function(self, event) table.insert(Seen, event) end)",
            )
            .unwrap();
        shutdown(&mut script, None, true);
        assert_eq!(
            script
                .eval::<(String, String)>("return Seen[1], Seen[2]")
                .unwrap(),
            ("PLAYER_LEAVING_WORLD".into(), "PLAYER_LOGOUT".into())
        );
    }
}
