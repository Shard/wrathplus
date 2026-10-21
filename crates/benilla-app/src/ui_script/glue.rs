//! The stock glue interface: build 12340's `Interface\GlueXML\GlueXML.toc` over the chain, the
//! screens `GlueParent.lua` switches between (`AccountLogin`, `CharacterSelect`, …), on its own VM
//! outside the world. The loader is FrameXML's ([`reference_ui::core_from`]) on the glue toc.

use benilla_ui::script::{GlueRequest, ScriptValue, UiScript};
use bevy::prelude::*;

use crate::glue_strings::GlueStrings;
use crate::net::{LoginFailedMessage, LoginStageMessage};

use super::reference_ui;

/// The glue load list, which `CGlueMgr` walks as `UI_Init` walks FrameXML's.
pub(super) const GLUE_TOC: &str = r"Interface\GlueXML\GlueXML.toc";

/// Load the chain's GlueXML onto `script`; returns every failure, as [`super::manifest::load_core`]
/// does for FrameXML.
pub(crate) fn load_glue_ui(script: &UiScript) -> Vec<String> {
    let Some(bytes) = reference_ui::read(GLUE_TOC) else {
        let e = format!("{GLUE_TOC}: not found — the stock glue interface is not built");
        error!("ui_script: {e}");
        return vec![e];
    };
    let core = reference_ui::core_at(GLUE_TOC, &benilla_ui::source::decode(&bytes));
    let mut toc = benilla_ui::status::Status::default();
    let mut failures = core.load_files_into(script, &core.toc.files, &mut toc);
    // Then AzerothBench's own additions (the Realmlist button), after the whole stock glue.
    let ours = super::addons::Addon::glue_additions();
    failures.extend(ours.load_files(script, &ours.toc.files));
    let mut log = benilla_ui::status::Status::default();
    toc.close_into(
        &mut log,
        script.framexml_debug(),
        benilla_ui::status::toc_banner(GLUE_TOC),
    );
    script.report_load_status(log);
    failures
}

/// Whether the stock GlueXML screens host the login screen this run (`WOW_STOCK_GLUE=1`), in
/// place of the native Rust screen. Off by default until the stock screen is proven live; the
/// native screen is deleted then.
pub(crate) fn stock_glue_active() -> bool {
    static ON: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var("WOW_STOCK_GLUE").as_deref() == Ok("1"));
    *ON
}

/// Run condition: the stock glue hosts the login screen.
pub(crate) fn stock_login_up(state: Option<Res<State<crate::char_select::ClientState>>>) -> bool {
    stock_glue_active() && state.is_some_and(|s| *s.get() == crate::char_select::ClientState::Login)
}

/// Entering the login screen with the stock glue: a fresh VM with the glue API, GlueXML loaded
/// silenced (as the in-game load is), then GlueParent's start, `FRAMES_LOADED` and the engine's
/// first screen, `SET_GLUE_SCREEN "login"`. Replaces the boot VM until the login screen is left.
pub(crate) fn enter_stock_login(world: &mut World) {
    if !stock_glue_active() {
        return;
    }
    let Some(mut script) = super::lifecycle::new_vm(world) else {
        return;
    };
    if let Err(e) = script.install_glue_api() {
        error!("ui_script: glue API install failed: {e}");
        return;
    }
    super::lifecycle::seat_raster_seam_for_load(world, &mut script);
    script.set_glue_saved_account(&crate::login::load_saved_account());
    let server = world
        .get_resource::<crate::realmlist::Realmlist>()
        .map(|r| r.address().to_string())
        .unwrap_or_default();
    script.set_glue_connection(false, &server);
    let pinned = world
        .get_resource::<crate::realmlist::Realmlist>()
        .is_some_and(|r| r.pinned_by_env());
    script.set_glue_realmlist(&server, pinned);
    super::manifest::silenced_ui_load(&mut script, |script| {
        for failure in load_glue_ui(script) {
            warn!("ui_script: GlueXML load: {failure}");
        }
    });
    script.fire_event("FRAMES_LOADED", vec![]);
    script.fire_event(
        "SET_GLUE_SCREEN",
        vec![benilla_ui::script::ScriptValue::Str("login".into())],
    );
    // The backdrop: AccountLogin's ModelFFX pane samples the glue booth, which stands the scene
    // this names (`UI_MainMenu_Northrend`, `portrait::glue_booth`).
    if let Some(mut preview) = world.get_resource_mut::<crate::portrait::GluePreview>() {
        preview.scene = Some(crate::portrait::GlueScene::MainMenu);
        preview.look = None;
        preview.yaw = 0.0;
    }
    info!("ui_script: stock glue — AccountLogin up");
    world.insert_non_send_resource(script);
}

/// Leaving the login screen: the boot VM again, as at every UI session's end; the native screens
/// after login draw over no glue frames.
pub(crate) fn leave_stock_login(world: &mut World) {
    if stock_glue_active() {
        super::lifecycle::install_boot_vm(world);
    }
}

// ── The login screen on the stock glue ──────────────────────────────────────────────────────────

/// `AccountLogin`'s calls drive the same attempt the native screen does ([`crate::login::Attempt`]),
/// and the attempt's stages and failures reach the stock `GlueDialog` as the engine's status events
/// (`OPEN_STATUS_DIALOG`, `UPDATE_STATUS_DIALOG`, `CLOSE_STATUS_DIALOG`, `GlueDialog.lua`). The
/// login module's `drive_policy` keeps the attempt's bookkeeping in every state.
fn s(text: &str) -> ScriptValue {
    ScriptValue::Str(text.into())
}

/// Open the stock status dialog of `which` (`GlueDialogTypes`) with `text`.
fn open_status(script: &mut UiScript, which: &str, text: &str) {
    script.fire_event("OPEN_STATUS_DIALOG", vec![s(which), s(text)]);
}

/// The stock login's calls and the attempt's progress, while the stock glue hosts the screen.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stock_login(
    script: Option<NonSendMut<UiScript>>,
    mut attempt: crate::login::Attempt,
    strings: Option<Res<GlueStrings>>,
    mut realms: ResMut<crate::realm_select::Realms>,
    choice: Res<crate::net::RealmChoice>,
    mut stages: MessageReader<LoginStageMessage>,
    mut failures: MessageReader<LoginFailedMessage>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut script) = script else {
        return;
    };
    let empty = GlueStrings::default();
    let strings = strings.as_deref().unwrap_or(&empty);
    for request in script.take_glue_requests() {
        match request {
            GlueRequest::Login { account, password } => {
                if account.is_empty() {
                    let text = strings.text("LOGIN_ENTER_NAME", "Please enter your account name.");
                    open_status(&mut script, "OKAY", text);
                } else if password.is_empty() {
                    let text = strings.text("LOGIN_ENTER_PASSWORD", "Please enter your password.");
                    open_status(&mut script, "OKAY", text);
                } else {
                    info!("login: stock glue submit as {account}");
                    attempt.submit_typed(&account, &password);
                    let text = strings.text("LOGIN_STATE_CONNECTING", "Connecting");
                    open_status(&mut script, "CANCEL", text);
                }
            }
            GlueRequest::CancelLogin | GlueRequest::StatusDialogClick => {
                attempt.abandon_in_flight();
                script.fire_event("CLOSE_STATUS_DIALOG", vec![]);
            }
            GlueRequest::Quit => {
                exit.write(AppExit::Success);
            }
            GlueRequest::SaveAccountName(name) => crate::login::save_account(&name),
            GlueRequest::ChangeRealm { category, index } => {
                crate::realm_select::glue_change_realm(&mut realms, &choice, category, index)
            }
            GlueRequest::RequestRealmList => crate::realm_select::glue_refresh(&choice),
            GlueRequest::RealmListCancelled => {
                crate::realm_select::glue_cancel(&mut realms, &choice)
            }
            // AzerothBench's Realmlist button: the native editor's Okay (normalize, point the
            // session at it, keep it in realmlist.wtf and the CVar).
            GlueRequest::SetRealmlist(text) => {
                let realmlist = attempt.realmlist_mut();
                if crate::login::accept_realmlist(&text, realmlist, Some(&mut *script)) {
                    let address = realmlist.address().to_string();
                    script.set_glue_realmlist(&address, realmlist.pinned_by_env());
                    script.set_glue_connection(false, &address);
                } else {
                    open_status(
                        &mut script,
                        "OKAY",
                        &format!("\"{text}\" is not a realmlist address (host or host:port)."),
                    );
                }
            }
        }
    }
    for msg in stages.read() {
        let text = crate::login::stage_text(strings, msg.stage).to_string();
        script.fire_event("UPDATE_STATUS_DIALOG", vec![s(&text)]);
    }
    for msg in failures.read() {
        // As the native screen's: the server's words for a terminal failure, else the refusal's
        // or the dial's text.
        let text = if msg.terminal {
            msg.reason.clone()
        } else {
            crate::login::fail_text(strings, msg.refusal, msg.dial.as_ref())
        };
        open_status(&mut script, "OKAY", &text);
    }
    // Reaching the realm list ends the connecting dialog, as `CGlueMgr::UpdateGlueDialog`
    // (`0x46b140`) does once the login state passes `LOGIN_STATE_CONNECTING`.
    // The realm list to show, or a refresh of the one shown: the snapshot, then the engine's
    // `OPEN_REALM_LIST` (shows the frame, or redraws it).
    if realms.shown && realms.is_changed() {
        use crate::realm_select::RealmsCategory;
        script.fire_event("CLOSE_STATUS_DIALOG", vec![]);
        let tabs = crate::realm_select::glue_tabs(realms.bypass_change_detection());
        let selected = realms.selected_category();
        script.set_glue_realms(tabs, selected, crate::realm_select::GLUE_UPDATE_RATE);
        script.fire_event("OPEN_REALM_LIST", vec![]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_ui::script::{GlueRequest, ScriptValue};

    /// The stock glue on the installed data: GlueXML loads clean, the engine's first screen shows
    /// AccountLogin with its boxes, and the stock Login button's handler queues the login with the
    /// typed account and password. Skips without an install.
    #[test]
    fn the_stock_account_login_opens_and_submits_on_the_installed_data() {
        let _data = benilla_formats::wow_data_or_skip!();
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
        // AzerothBench's Realmlist button (`assets/ui/glue`): the stock dialog edits the address.
        script.set_glue_realmlist("127.0.0.1", false);
        script.run("AccountLoginRealmlistButton:Click()").unwrap();
        assert_eq!(
            script
                .eval::<String>(
                    "return tostring(AccountLoginRealmlistButton:IsVisible())..' '..GlueDialog.which..' '..\
                     GlueDialogEditBox:GetText()"
                )
                .unwrap(),
            "1 BENILLA_REALMLIST 127.0.0.1"
        );
        script
            .run("GlueDialogEditBox:SetText('10.0.0.5:3724') GlueDialog_OnClick(1)")
            .unwrap();
        assert_eq!(
            script.take_glue_requests(),
            vec![GlueRequest::SetRealmlist("10.0.0.5:3724".into())]
        );
        script
            .run(
                "AccountLoginAccountEdit:SetText('Bob') AccountLoginPasswordEdit:SetText('hunter2') \
                 AccountLogin_Login()",
            )
            .unwrap();
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
        let realm = |name: &str| benilla_ui::script::GlueRealm {
            name: name.into(),
            num_characters: 1,
            ..Default::default()
        };
        script.set_glue_realms(
            vec![benilla_ui::script::GlueRealmTab {
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
        let errors = script.take_errors();
        assert!(errors.is_empty(), "{errors:#?}");
    }
}

#[cfg(test)]
mod stock_login_tests {
    use super::*;
    use crate::net::{LoginAbandon, LoginFailedMessage, LoginStageMessage, LoginSubmit};

    /// A glue VM whose event frame records each status event as `event:which:text;`.
    fn vm() -> UiScript {
        let s = UiScript::new().unwrap();
        s.install_glue_api().unwrap();
        s.run(
            "local f = CreateFrame('Frame') \
             for _, e in ipairs({'OPEN_STATUS_DIALOG', 'UPDATE_STATUS_DIALOG', 'CLOSE_STATUS_DIALOG'}) do f:RegisterEvent(e) end \
             f:SetScript('OnEvent', function(self, e, a, b) LOG = (LOG or '') .. e .. ':' .. tostring(a) .. ':' .. tostring(b) .. ';' end)",
        )
        .unwrap();
        s
    }

    fn app() -> (App, crossbeam_channel::Receiver<crate::net::LoginRequest>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (realm_tx, _realm_rx) = crossbeam_channel::unbounded();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<crate::login::LoginIntent>()
            .insert_resource(LoginSubmit(tx))
            .insert_resource(LoginAbandon(Default::default()))
            .insert_resource(crate::realmlist::Realmlist::unpinned("127.0.0.1"))
            .init_resource::<crate::realm_select::Realms>()
            .insert_resource(crate::net::RealmChoice(realm_tx))
            .add_message::<LoginStageMessage>()
            .add_message::<LoginFailedMessage>()
            .add_message::<AppExit>()
            .add_systems(Update, stock_login);
        app.insert_non_send_resource(vm());
        (app, rx)
    }

    fn log(app: &mut App) -> String {
        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .eval::<String>("local l = LOG or '' LOG = nil return l")
            .unwrap()
    }

    /// `DefaultServerLogin` submits the attempt to the realmlist and opens the stock connecting
    /// dialog; a stage updates its text, a refusal opens the OKAY dialog, Cancel closes it.
    #[test]
    fn the_stock_login_drives_the_attempt_and_the_stock_status_dialog() {
        let (mut app, rx) = app();
        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .run("DefaultServerLogin('Bob', 'hunter2')")
            .unwrap();
        app.update();
        let sent: Vec<_> = rx.try_iter().collect();
        assert_eq!(sent.len(), 1);
        assert_eq!(
            (sent[0].user.as_str(), sent[0].host.as_str()),
            ("Bob", "127.0.0.1")
        );
        assert!(
            app.world()
                .resource::<crate::login::LoginIntent>()
                .in_flight
        );
        assert_eq!(log(&mut app), "OPEN_STATUS_DIALOG:CANCEL:Connecting;");

        app.world_mut().write_message(LoginStageMessage {
            stage: benilla_protocol::LoginStage::Authenticating,
        });
        app.update();
        assert_eq!(log(&mut app), "UPDATE_STATUS_DIALOG:Authenticating:nil;");

        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .run("StatusDialogClick()")
            .unwrap();
        app.update();
        assert_eq!(log(&mut app), "CLOSE_STATUS_DIALOG:nil:nil;");
        assert!(
            !app.world()
                .resource::<crate::login::LoginIntent>()
                .in_flight
        );

        // An empty box asks for it instead of dialing.
        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .run("DefaultServerLogin('Bob', '')")
            .unwrap();
        app.update();
        assert_eq!(rx.try_iter().count(), 0);
        assert!(log(&mut app).starts_with("OPEN_STATUS_DIALOG:OKAY:"));
    }

    /// A realm list to show reaches the stock frames as the snapshot and `OPEN_REALM_LIST`, and the
    /// stock `ChangeRealm` answers the park with that row's realm.
    #[test]
    fn the_stock_realm_list_is_fed_and_answers_the_park() {
        let (realm_tx, realm_rx) = crossbeam_channel::unbounded();
        let (mut app, _rx) = app();
        app.insert_resource(crate::net::RealmChoice(realm_tx));
        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .run(
                "local f = CreateFrame('Frame') f:RegisterEvent('OPEN_REALM_LIST') \
                 f:SetScript('OnEvent', function() OPENED = (OPENED or 0) + 1 end)",
            )
            .unwrap();
        {
            let mut realms = app
                .world_mut()
                .resource_mut::<crate::realm_select::Realms>();
            realms.realms = ["AzerothCore", "Second"]
                .into_iter()
                .map(|name| benilla_protocol::RealmInfo {
                    name: name.into(),
                    address: "127.0.0.1:8085".into(),
                    population: 1.0,
                    characters: 1,
                    realm_type: 0,
                    flags: 0,
                    category: 1,
                    id: 1,
                })
                .collect();
            realms.shown = true;
        }
        app.update();
        let (opened, second) = {
            let s = app.world_mut().non_send_resource_mut::<UiScript>();
            (
                s.eval::<i64>("return OPENED or 0").unwrap(),
                s.eval::<String>("return (GetRealmInfo(1, 2))").unwrap(),
            )
        };
        assert_eq!(opened, 1);
        assert_eq!(second, "Second");
        app.update();
        assert_eq!(
            app.world_mut()
                .non_send_resource_mut::<UiScript>()
                .eval::<i64>("return OPENED")
                .unwrap(),
            1,
            "an unchanged list does not reopen"
        );
        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .run("ChangeRealm(1, 2)")
            .unwrap();
        app.update();
        let sent: Vec<_> = realm_rx.try_iter().collect();
        assert!(
            matches!(sent.last(), Some(crate::net::RealmRequest::Enter(name)) if name == "Second"),
            "{sent:?}"
        );
        assert!(!app.world().resource::<crate::realm_select::Realms>().shown);
    }

    /// The Realmlist button's address becomes the session's; one that is not an address opens the
    /// OKAY dialog and changes nothing.
    #[test]
    fn the_realmlist_button_sets_the_session_address() {
        let (mut app, _rx) = app();
        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .run("BenillaSetRealmlist('logon.example.org:3725')")
            .unwrap();
        app.update();
        assert_eq!(
            app.world()
                .resource::<crate::realmlist::Realmlist>()
                .address(),
            "logon.example.org:3725"
        );
        assert_eq!(
            app.world_mut()
                .non_send_resource_mut::<UiScript>()
                .eval::<String>("return (BenillaGetRealmlist())")
                .unwrap(),
            "logon.example.org:3725"
        );
        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .run("BenillaSetRealmlist('two words')")
            .unwrap();
        app.update();
        assert!(log(&mut app).starts_with("OPEN_STATUS_DIALOG:OKAY:"));
        assert_eq!(
            app.world()
                .resource::<crate::realmlist::Realmlist>()
                .address(),
            "logon.example.org:3725"
        );
    }
}
