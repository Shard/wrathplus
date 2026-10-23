//! The stock glue on the installed data.

use super::*;
use wrathplus_ui::script::glue::GlueRequest;
use wrathplus_ui::script::ScriptValue;

/// The stock glue on the installed data: GlueXML loads clean, the engine's first screen shows
/// AccountLogin with its boxes, and the stock Login button's handler queues the login with the
/// typed account and password. Skips without an install.
#[test]
fn the_stock_account_login_opens_and_submits_on_the_installed_data() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut script = UiScript::new().unwrap();
    script.install_glue_api().unwrap();
    script.set_screen_size(1600.0, 1000.0);
    let failures = load_glue_ui(&script);
    assert!(failures.is_empty(), "{failures:#?}");
    // GlueParent's own start: FRAMES_LOADED, then the engine's first screen.
    script.fire_event("FRAMES_LOADED", vec![]);
    script.fire_event("SET_GLUE_SCREEN", vec![ScriptValue::Str("login".into())]);
    script.tick(0.1);
    let shown = script
        .eval::<String>(
            "local function v(f) return f and (f:IsVisible() and 'v' or (f:IsShown() and 's' or 'h')) or 'nil' end \
             return table.concat({v(GlueParent), v(AccountLogin), v(AccountLoginUI), \
             v(AccountLoginAccountEdit), v(AccountLoginPasswordEdit), \
             tostring(GetCurrentGlueScreenName())}, ' ')",
        )
        .unwrap();
    assert_eq!(
        shown, "v v v v v login",
        "GlueParent AccountLogin AccountLoginUI account password screen"
    );
    // AzerothBench's Server field (`assets/ui/glue/ServerField.xml`): it shows the address,
    // and leaving it after an edit asks the host to keep the new one.
    script.set_glue_realmlist("127.0.0.1", false);
    script.fire_event("BENILLA_REALMLIST_CHANGED", vec![]);
    let field = |script: &UiScript| {
        script
            .eval::<String>(
                "return tostring(AccountLoginServerEdit:IsVisible())..' '..\
                 tostring(not AccountLoginServerEdit.pinned)..' '..\
                 AccountLoginServerEdit:GetText()..' '..AccountLoginServerEditLabel:GetText()",
            )
            .unwrap()
    };
    assert_eq!(field(&script), "1 true 127.0.0.1 Server");
    script
        .run(
            "AccountLoginServerEdit:SetFocus() AccountLoginServerEdit:SetText('10.0.0.5:3724') \
             AccountLoginServerEdit:ClearFocus()",
        )
        .unwrap();
    assert_eq!(
        script.take_glue_requests(),
        vec![GlueRequest::SetRealmlist("10.0.0.5:3724".into())]
    );
    // The host keeps it and says so; a launch pin shows, disabled.
    script.set_glue_realmlist("10.0.0.5:3724", false);
    script.fire_event("BENILLA_REALMLIST_CHANGED", vec![]);
    assert_eq!(field(&script), "1 true 10.0.0.5:3724 Server");
    script.set_glue_realmlist("vps.example:3724", true);
    script.fire_event("BENILLA_REALMLIST_CHANGED", vec![]);
    assert_eq!(
        field(&script),
        "1 false vps.example:3724 Server (set by the launch)"
    );
    script
        .run(
            "AccountLoginServerEdit:SetFocus() AccountLoginServerEdit:SetText('x') \
             AccountLoginServerEdit:ClearFocus()",
        )
        .unwrap();
    assert!(
        script.take_glue_requests().is_empty(),
        "a pinned field sends nothing"
    );
    assert!(
        script.errors().is_empty(),
        "script errors: {:?}",
        script.errors()
    );
    script.set_glue_realmlist("127.0.0.1", false);
    script.fire_event("BENILLA_REALMLIST_CHANGED", vec![]);
    // `--fill-only`: the launch's credentials wait in the boxes; nothing submits until Login.
    script.fill_login("Bob", "hunter2").unwrap();
    assert_eq!(
        script
            .eval::<(String, String)>(
                "return AccountLoginAccountEdit:GetText(), AccountLoginPasswordEdit:GetText()"
            )
            .unwrap(),
        ("Bob".to_string(), "hunter2".to_string())
    );
    assert!(
        script.take_glue_requests().is_empty(),
        "a fill does not log in"
    );
    script.run("AccountLogin_Login()").unwrap();
    assert_eq!(
        script.take_glue_requests(),
        vec![
            GlueRequest::Login {
                account: "Bob".into(),
                password: "hunter2".into()
            },
            // Save Account Name unticked: the saved name clears.
            GlueRequest::SaveAccountName(String::new()),
        ]
    );
    assert_eq!(
        script
            .eval::<String>("return AccountLoginPasswordEdit:GetText()")
            .unwrap(),
        "",
        "the stock handler clears the password box"
    );
    // The realm list the engine opens after the logon: the stock RealmList over a seeded
    // list, and a double-click on a row is `ChangeRealm(tab, row)`.
    let realm = |name: &str| wrathplus_ui::script::glue::GlueRealm {
        name: name.into(),
        num_characters: 1,
        ..Default::default()
    };
    script.set_glue_realms(
        vec![wrathplus_ui::script::glue::GlueRealmTab {
            name: String::new(),
            realms: vec![realm("AzerothCore"), realm("Second")],
        }],
        1,
        5.0,
    );
    script.fire_event("OPEN_REALM_LIST", vec![]);
    script.tick(0.1);
    assert_eq!(
        script
            .eval::<String>(
                "return tostring(RealmList:IsVisible())..' '..RealmListRealmButton1:GetText()..' '..\
                 tostring(RealmListRealmButton3:IsShown())"
            )
            .unwrap(),
        // 12340's visibility queries answer 1 or nil.
        "1 AzerothCore nil"
    );
    script
        .run("RealmSelectButton_OnDoubleClick(RealmListRealmButton2, 2)")
        .unwrap();
    assert_eq!(
        script.take_glue_requests(),
        vec![GlueRequest::ChangeRealm {
            category: 1,
            index: 2
        }]
    );
    // Character select over seeded rows: the stock list shows them, a click selects the row
    // and asks the stage for it, a double-click enters the world.
    script.set_glue_characters(
        vec![
            wrathplus_ui::script::glue::GlueCharacter {
                name: "Benchone".into(),
                race: "Human".into(),
                class: "Warrior".into(),
                level: 7,
                zone: "Elwynn Forest".into(),
                stage: "Human".into(),
                ..Default::default()
            },
            wrathplus_ui::script::glue::GlueCharacter {
                name: "Grunt".into(),
                race: "Orc".into(),
                class: "Death Knight".into(),
                level: 55,
                zone: "Acherus".into(),
                stage: "DeathKnight".into(),
                ..Default::default()
            },
        ],
        "Orc",
    );
    // Leaving login fades AccountLoginUI out first (`GlueScreenExit` → `GlueFrameFadeOut(...,
    // GoToPendingGlueScreen)`), so the switch lands once the fade has run.
    script.fire_event(
        "SET_GLUE_SCREEN",
        vec![ScriptValue::Str("charselect".into())],
    );
    for _ in 0..30 {
        script.tick(0.1);
    }
    script.fire_event("CHARACTER_LIST_UPDATE", vec![]);
    script.tick(0.1);
    assert_eq!(
        script
            .eval::<String>(
                "return tostring(CharacterSelect:IsVisible())..' '..GetCurrentGlueScreenName()..' '..\
                 CharSelectCharacterButton1ButtonTextName:GetText()..' '..\
                 tostring(CharSelectCharacterButton2:IsShown())..' '..tostring(CharSelectCharacterButton3:IsShown())"
            )
            .unwrap(),
        "1 charselect Benchone 1 nil"
    );
    script.take_glue_requests();
    script
        .run("CharacterSelectButton_OnClick(CharSelectCharacterButton2)")
        .unwrap();
    let requests = script.take_glue_requests();
    assert!(
        requests.contains(&GlueRequest::SelectCharacter(2)),
        "{requests:?}"
    );
    assert!(
        requests.contains(&GlueRequest::SetBackground(
            r"Interface\Glues\Models\UI_DeathKnight\UI_DeathKnight.m2".into()
        )),
        "{requests:?}"
    );
    script
        .run("CharacterSelectButton_OnDoubleClick(CharSelectCharacterButton2)")
        .unwrap();
    assert!(script
        .take_glue_requests()
        .contains(&GlueRequest::EnterWorld));
    let errors = script.take_errors();
    assert!(errors.is_empty(), "{errors:#?}");
}
