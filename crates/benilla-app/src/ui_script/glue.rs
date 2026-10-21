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
    let failures = core.load_files_into(script, &core.toc.files, &mut toc);
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
    realm_list_up: Res<crate::realm_select::Realms>,
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
    if realm_list_up.shown && realm_list_up.is_changed() {
        script.fire_event("CLOSE_STATUS_DIALOG", vec![]);
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
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<crate::login::LoginIntent>()
            .insert_resource(LoginSubmit(tx))
            .insert_resource(LoginAbandon(Default::default()))
            .insert_resource(crate::realmlist::Realmlist::unpinned("127.0.0.1"))
            .init_resource::<crate::realm_select::Realms>()
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
}
