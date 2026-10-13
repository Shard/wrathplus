use std::collections::HashMap;

use crate::framexml;
use crate::loader;
use crate::script::{ScriptDialect, ScriptValue, SeededCvar, UiScript, UnitGuids, UnitState};

#[test]
fn vanilla_remains_the_default_and_keeps_legacy_semantics() {
    let mut s = UiScript::new().unwrap();
    assert_eq!(s.dialect(), ScriptDialect::Vanilla);
    assert!(s.eval::<bool>("return select == nil").unwrap());

    s.run(
        "f=CreateFrame('Frame'); f:RegisterEvent('E'); \
         f:SetScript('OnEvent', function(a) LEGACY={a,this,event,arg1} end)",
    )
    .unwrap();
    s.fire_event("E", vec![ScriptValue::Str("payload".into())]);
    assert!(s
        .eval::<bool>("return LEGACY[1] == nil and LEGACY[2] == f")
        .unwrap());
    assert_eq!(
        s.eval::<String>("return LEGACY[3]..':'..LEGACY[4]")
            .unwrap(),
        "E:payload"
    );
}

#[test]
fn dialect_parser_and_generic_for_are_isolated_between_simultaneous_states() {
    let vanilla = UiScript::new().unwrap();
    let wrath = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    for _ in 0..2 {
        assert!(vanilla.run("return #{} ").is_err());
        assert!(vanilla.run("return 5 % 2").is_err());
        assert!(vanilla.run("return function(...) return ... end").is_err());
        assert_eq!(
            vanilla
                .eval::<i64>("local n=0; for k,v in {a=2} do n=n+v end; return n")
                .unwrap(),
            2
        );
        assert_eq!(wrath.eval::<i64>("return #{1,2}, 5 % 2").unwrap(), 2);
        assert_eq!(wrath.eval::<i64>("return 5 % 2").unwrap(), 1);
        assert!(wrath.run("for k,v in {a=2} do end").is_err());
    }
}

#[test]
fn wrath_keeps_native_lua51_select_length_and_table_semantics() {
    let s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    assert_eq!(s.dialect(), ScriptDialect::Wrath12340);
    assert_eq!(s.eval::<i64>("return select('#', 1, nil, 3)").unwrap(), 3);
    assert_eq!(s.eval::<i64>("return #{10,20,30}").unwrap(), 3);
    assert!(s
        .eval::<bool>("return table.maxn ~= nil and not pcall(table.setn, {}, 10)")
        .unwrap());
}

fn seeded_cvar(name: &str, value: &str) -> SeededCvar {
    SeededCvar {
        name: name.into(),
        value: value.into(),
        default: "host-default-not-a-wrath-claim".into(),
        latched: false,
    }
}

#[test]
fn vanilla_cvar_surface_and_unbounded_registry_are_unchanged() {
    let mut s = UiScript::new().unwrap();
    s.seed_cvars([seeded_cvar("realmName", "Vanilla Realm")]);

    assert_eq!(
        s.eval::<(String, String)>("return GetCVar('REALMNAME'), GetCVarDefault('realmName')")
            .unwrap(),
        (
            "Vanilla Realm".into(),
            "host-default-not-a-wrath-claim".into()
        )
    );
    assert!(s
        .eval::<bool>("return SetCVar ~= nil and RegisterCVar ~= nil and ConsoleExec ~= nil")
        .unwrap());
    s.set_cvar_host("realmName", "Later Realm");
    assert_eq!(
        s.eval::<String>("return GetCVar('realmName')").unwrap(),
        "Later Realm"
    );
    assert!(s.take_cvar_changes().is_empty());
}

fn observed_unit(exists: bool, guid: u64) -> UnitState {
    UnitState {
        exists,
        guid,
        ..Default::default()
    }
}

#[test]
fn wrath_unit_is_unit_compares_only_existing_observed_identities() {
    let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    s.set_unit("player", Some(observed_unit(true, 0x10)));
    s.set_unit("target", Some(observed_unit(true, 0x10)));
    s.set_unit("party1", Some(observed_unit(true, 0x20)));

    assert_eq!(
        s.eval::<String>(r#"return type(UnitIsUnit("player", "target"))"#)
            .unwrap(),
        "number",
        "the admitted existing binding returns numeric 1; exact build-12340 return type is not yet independently qualified"
    );
    assert_eq!(
        s.eval::<i64>(r#"return UnitIsUnit("player", "target")"#)
            .unwrap(),
        1
    );
    assert!(s
        .eval::<bool>(r#"return UnitIsUnit("player", "party1") == nil"#)
        .unwrap());

    s.set_unit("target", None);
    assert!(s
        .eval::<bool>(r#"return UnitIsUnit("player", "target") == nil"#)
        .unwrap());
    s.set_unit("target", Some(observed_unit(false, 0x10)));
    assert!(s
        .eval::<bool>(r#"return UnitIsUnit("player", "target") == nil"#)
        .unwrap());
    s.set_unit("player", Some(observed_unit(true, 0)));
    assert_eq!(
        s.eval::<i64>(r#"return UnitIsUnit("player", "player")"#)
            .unwrap(),
        1,
        "the existing resolver recognizes the same existing token even before its GUID arrives"
    );
    assert_eq!(
        s.eval::<i64>(r#"return UnitIsUnit("player", "PLAYER")"#)
            .unwrap(),
        1,
        "same-token identity follows the resolver's case-insensitive semantics"
    );
    s.set_unit("target", Some(observed_unit(true, 0)));
    assert!(s
        .eval::<bool>(r#"return UnitIsUnit("player", "target") == nil"#)
        .unwrap());
}

#[test]
fn wrath_unit_is_unit_uses_the_existing_target_chain_resolver() {
    const TARGET: u64 = 0xF130_0000_0000_0001;
    const TARGET_TARGET: u64 = 0x21;

    let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    s.set_unit_guids(&UnitGuids {
        target: TARGET,
        held: HashMap::from([(TARGET, TARGET_TARGET)]),
        ..Default::default()
    });
    s.set_unit("party1", Some(observed_unit(true, TARGET_TARGET)));
    s.set_unit_by_guid(TARGET_TARGET, Some(observed_unit(true, TARGET_TARGET)));

    assert_eq!(
        s.eval::<i64>(r#"return UnitIsUnit("targettarget", "party1")"#)
            .unwrap(),
        1
    );
}

#[test]
fn wrath_setscript_and_xml_handlers_receive_positional_arguments_only() {
    let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    s.run(
        "this,event,arg1='sentinel-this','sentinel-event','sentinel-arg'; \
         f=CreateFrame('Button','WrathButton'); f:RegisterEvent('E'); \
         f:SetScript('OnEvent', function(self, event, ...) EVENT={self,event,...} end); \
         f:SetScript('OnUpdate', function(self, elapsed) UPDATE={self,elapsed} end); \
         f:SetScript('OnClick', function(self, button, down) CLICK={self,button,down} end)",
    )
    .unwrap();
    s.fire_event("E", vec![ScriptValue::Str("payload".into())]);
    s.tick(0.25);
    s.run("WrathButton:Click('RightButton')").unwrap();
    assert!(s
        .eval::<bool>("return EVENT[1]==f and EVENT[2]=='E' and EVENT[3]=='payload'")
        .unwrap());
    assert!(s
        .eval::<bool>("return UPDATE[1]==f and UPDATE[2]==0.25")
        .unwrap());
    assert!(s
        .eval::<bool>("return CLICK[1]==f and CLICK[2]=='RightButton'")
        .unwrap());
    assert_eq!(
        s.eval::<(String, String, String)>("return this,event,arg1")
            .unwrap(),
        (
            "sentinel-this".into(),
            "sentinel-event".into(),
            "sentinel-arg".into()
        )
    );

    let doc = framexml::parse(
        r#"<Ui><Frame name="XmlWrath"><Scripts><OnLoad>
        XML_SELF=self; XML_THIS=this
        </OnLoad></Scripts></Frame></Ui>"#,
    )
    .unwrap();
    let report = loader::load(&s, &doc, &|_| None);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(s
        .eval::<bool>("return XML_SELF==XmlWrath and XML_THIS=='sentinel-this'")
        .unwrap());
}

#[test]
fn wrath_nested_handlers_do_not_mutate_or_invent_legacy_globals() {
    let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    s.run(
        "outer=CreateFrame('Frame'); inner=CreateFrame('Frame'); \
         inner:SetScript('OnShow',function(self) INNER=self end); \
         outer:RegisterEvent('E'); outer:SetScript('OnEvent',function(self,event,...) \
           inner:Hide(); inner:Show(); OUTER={self,event,...,this,arg1} end)",
    )
    .unwrap();
    s.fire_event("E", vec![ScriptValue::Int(7)]);
    assert!(s
        .eval::<bool>("return INNER==inner and OUTER[1]==outer and OUTER[2]=='E' and OUTER[3]==7")
        .unwrap());
    assert!(s
        .eval::<bool>("return this==nil and event==nil and arg1==nil")
        .unwrap());
}

#[test]
fn wrath_native_pattern_backtracking_and_recursion_are_bounded() {
    let s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    for source in [
        "return string.find(string.rep('a', 32), 'a*a*a*a*a*a*a*a*a*a*b')",
        "return string.match(string.rep('a', 32), 'a*a*a*a*a*a*a*a*a*a*b')",
        "return string.gsub(string.rep('a', 32), 'a*a*a*a*a*a*a*a*a*a*b', '')",
        "local it=string.gmatch(string.rep('a', 32), 'a*a*a*a*a*a*a*a*a*a*b'); return it()",
    ] {
        let error = s
            .run(source)
            .expect_err("pathological pattern must stop inside the native matcher");
        assert!(error.to_string().contains("pattern matching"), "{error}");
    }
    assert_eq!(
        s.eval::<String>("return string.match('player-123', '%d+')")
            .unwrap(),
        "123"
    );
}

#[test]
fn wrath_addon_chunks_share_private_table_per_addon() {
    let s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    s.run_addon_chunk_named(
        "Probe",
        b"local name,p=...; p.value=name",
        "@Probe\\one.lua",
    )
    .unwrap();
    s.run_addon_chunk_named(
        "Probe",
        b"local name,p=...; ADDON={name,p.value}",
        "@Probe\\two.lua",
    )
    .unwrap();
    assert_eq!(
        s.eval::<(String, String)>("return ADDON[1],ADDON[2]")
            .unwrap(),
        ("Probe".into(), "Probe".into())
    );
}

#[test]
fn wrath_pointer_focus_tracks_real_hit_testing_visibility_and_button_state() {
    let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    s.set_screen_size(800.0, 600.0);
    s.run(
        r#"
        log = {}
        local function record(self, suffix)
            table.insert(log, self:GetName()..suffix)
        end
        local function frame(name, x, y, level)
            local f = CreateFrame("Button", name)
            f:SetPoint("BOTTOMLEFT", x, y)
            f:SetWidth(200); f:SetHeight(200)
            f:SetFrameLevel(level); f:EnableMouse(true)
            f:SetScript("OnEnter", function(self, motion)
                assert(motion == true); record(self, ":enter")
            end)
            f:SetScript("OnLeave", function(self, motion)
                assert(motion == true); record(self, ":leave")
            end)
            return f
        end
        Back = frame("WrathBack", 100, 100, 1)
        Front = frame("WrathFront", 150, 150, 2)
        Disabled = frame("WrathDisabled", 400, 100, 3)
        Disabled:SetWidth(100); Disabled:SetHeight(100); Disabled:Disable()
        "#,
    )
    .unwrap();
    s.resolve();

    s.mouse_move(110.0, 110.0);
    assert!(s.eval::<bool>("return GetMouseFocus() == Back").unwrap());

    s.mouse_move(175.0, 175.0);
    assert!(s.eval::<bool>("return GetMouseFocus() == Front").unwrap());
    s.run("Front:Hide()").unwrap();
    assert!(
        s.eval::<bool>("return GetMouseFocus() == nil").unwrap(),
        "hiding the hovered frame clears focus before the deferred re-pick"
    );

    s.mouse_move(175.0, 175.0);
    assert!(s.eval::<bool>("return GetMouseFocus() == Back").unwrap());
    s.mouse_move(425.0, 125.0);
    assert!(
        s.eval::<bool>("return GetMouseFocus() == Disabled")
            .unwrap(),
        "a disabled button remains the actual hover frame"
    );
    s.mouse_move(700.0, 500.0);
    assert!(s.eval::<bool>("return GetMouseFocus() == nil").unwrap());
    assert_eq!(
        s.eval::<String>("return table.concat(log, ',')").unwrap(),
        "WrathBack:enter,WrathBack:leave,WrathFront:enter,WrathFront:leave,WrathBack:enter,WrathBack:leave",
        "overlap follows frame order; disabled buttons suppress enter and leave handlers"
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn wrath_cursor_position_is_screen_space_and_callers_apply_effective_scale() {
    let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    s.set_screen_size(800.0, 600.0);
    s.run(
        "Scaled = CreateFrame('Frame'); Scaled:SetScale(0.5); \
         function ReadCursorForScaledFrame() \
           local x,y = GetCursorPosition(); local scale = Scaled:GetEffectiveScale(); \
           return x,y,x/scale,y/scale \
         end",
    )
    .unwrap();

    s.mouse_move(240.0, 135.0);
    assert_eq!(
        s.eval::<(f32, f32, f32, f32)>("return ReadCursorForScaledFrame()")
            .unwrap(),
        (240.0, 135.0, 480.0, 270.0),
        "GetCursorPosition stays in screen UI units; frame-local math divides by effective scale"
    );
}
