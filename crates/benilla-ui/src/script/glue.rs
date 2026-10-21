//! Build 12340's glue API for the stock GlueXML screens, installed only on the glue VM
//! ([`super::UiScript::install_glue_api`]): the in-game VM never answers these, as the reference's
//! glue Lua state is its own (`CGlueMgr`).
//!
//! This slice is AccountLogin's and GlueParent's. The calls that act go out as [`GlueRequest`]s for
//! App; the reads answer from [`GlueState`], which App seeds. Labelled as inert, because a 3.3.5a
//! AzerothCore realm never raises them: the TOS/EULA/scan/contest notices (shown as accepted), the
//! account messages (none, every agreement already accepted), game-account selection, tokens and PINs, `LaunchURL` (no browser), and
//! the glue music and ambience calls (App owns glue sound).

use mlua::{Lua, MultiValue, Value};

use super::Model;

/// A glue call App acts on, in call order.
#[derive(Clone, PartialEq, Eq)]
pub enum GlueRequest {
    /// `DefaultServerLogin(account, password)`: log in to the realmlist's logon server.
    Login { account: String, password: String },
    /// `CancelLogin()`: abandon the attempt in flight.
    CancelLogin,
    /// `QuitGame()`.
    Quit,
    /// `SetSavedAccountName(name)`; empty clears.
    SaveAccountName(String),
    /// `StatusDialogClick()`: the status dialog's button (Cancel while connecting).
    StatusDialogClick,
}

impl std::fmt::Debug for GlueRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Never the password.
            GlueRequest::Login { account, .. } => {
                write!(f, "Login {{ account: {account:?}, password: <set> }}")
            }
            GlueRequest::CancelLogin => f.write_str("CancelLogin"),
            GlueRequest::Quit => f.write_str("Quit"),
            GlueRequest::SaveAccountName(n) => write!(f, "SaveAccountName({n:?})"),
            GlueRequest::StatusDialogClick => f.write_str("StatusDialogClick"),
        }
    }
}

/// What the glue reads answer from, seeded by App.
#[derive(Default)]
pub(crate) struct GlueState {
    pub(crate) requests: Vec<GlueRequest>,
    pub(crate) saved_account: String,
    pub(crate) connected: bool,
    pub(crate) server_name: String,
    pub(crate) current_screen: String,
    pub(crate) num_characters: u32,
}

impl super::UiScript {
    /// Install the glue API on this VM: the glue host's VM only.
    pub fn install_glue_api(&self) -> mlua::Result<()> {
        install(self.lua())
    }

    /// Drain the glue calls App acts on, in call order.
    pub fn take_glue_requests(&mut self) -> Vec<GlueRequest> {
        std::mem::take(&mut self.model_mut().glue.requests)
    }

    /// `GetSavedAccountName`'s answer.
    pub fn set_glue_saved_account(&mut self, name: &str) {
        self.model_mut().glue.saved_account = name.to_string();
    }

    /// `IsConnectedToServer` and `GetServerName`'s answers.
    pub fn set_glue_connection(&mut self, connected: bool, server_name: &str) {
        let mut m = self.model_mut();
        m.glue.connected = connected;
        m.glue.server_name = server_name.to_string();
    }

    /// `GetNumCharacters`'s answer.
    pub fn set_glue_num_characters(&mut self, n: u32) {
        self.model_mut().glue.num_characters = n;
    }

    /// The screen `SetCurrentScreen` last named (`"login"`, `"charselect"`, …).
    pub fn glue_current_screen(&self) -> String {
        self.model_ref().glue.current_screen.clone()
    }
}

fn push(lua: &Lua, r: GlueRequest) {
    lua.app_data_mut::<Model>()
        .expect("model app_data")
        .glue
        .requests
        .push(r);
}

fn text(lua: &Lua, v: Option<&Value>) -> mlua::Result<String> {
    Ok(v.cloned()
        .map(|v| lua.coerce_string(v))
        .transpose()?
        .flatten()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default())
}

fn set(
    lua: &Lua,
    name: &str,
    f: impl Fn(&Lua, MultiValue) -> mlua::Result<MultiValue> + Send + 'static,
) -> mlua::Result<()> {
    lua.globals()
        .set(name, lua.create_function(move |lua, args| f(lua, args))?)
}

fn none() -> mlua::Result<MultiValue> {
    Ok(MultiValue::new())
}

fn one(v: Value) -> mlua::Result<MultiValue> {
    Ok(MultiValue::from_vec(vec![v]))
}

/// A function that returns `v` (an integer, or nil for `None`) and does nothing else.
fn constant(lua: &Lua, name: &str, v: Option<i64>) -> mlua::Result<()> {
    set(lua, name, move |_, _| {
        one(v.map_or(Value::Nil, Value::Integer))
    })
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    set(lua, "DefaultServerLogin", |lua, args| {
        let account = text(lua, args.front())?;
        let password = text(lua, args.get(1))?;
        push(lua, GlueRequest::Login { account, password });
        none()
    })?;
    set(lua, "CancelLogin", |lua, _| {
        push(lua, GlueRequest::CancelLogin);
        none()
    })?;
    set(lua, "QuitGame", |lua, _| {
        push(lua, GlueRequest::Quit);
        none()
    })?;
    set(lua, "StatusDialogClick", |lua, _| {
        push(lua, GlueRequest::StatusDialogClick);
        none()
    })?;
    set(lua, "GetSavedAccountName", |lua, _| {
        let name = lua
            .app_data_ref::<Model>()
            .expect("model")
            .glue
            .saved_account
            .clone();
        one(Value::String(lua.create_string(&name)?))
    })?;
    set(lua, "SetSavedAccountName", |lua, args| {
        let name = text(lua, args.front())?;
        lua.app_data_mut::<Model>()
            .expect("model")
            .glue
            .saved_account = name.clone();
        push(lua, GlueRequest::SaveAccountName(name));
        none()
    })?;
    set(lua, "IsConnectedToServer", |lua, _| {
        let connected = lua.app_data_ref::<Model>().expect("model").glue.connected;
        one(super::binding_abi::flag(connected))
    })?;
    set(lua, "GetServerName", |lua, _| {
        let name = lua
            .app_data_ref::<Model>()
            .expect("model")
            .glue
            .server_name
            .clone();
        one(Value::String(lua.create_string(&name)?))
    })?;
    set(lua, "SetCurrentScreen", |lua, args| {
        let name = text(lua, args.front())?;
        lua.app_data_mut::<Model>()
            .expect("model")
            .glue
            .current_screen = name;
        none()
    })?;
    set(lua, "GetNumCharacters", |lua, _| {
        let n = lua
            .app_data_ref::<Model>()
            .expect("model")
            .glue
            .num_characters;
        one(Value::Integer(i64::from(n)))
    })?;
    // Wrath of the Lich King: expansion level 2.
    constant(lua, "GetClientExpansionLevel", Some(2))?;
    // The glue state's own `GetBuildInfo`, five values for `VERSION_TEMPLATE`
    // (`"%s %s (%s) (%s)\n%s"`), which `AccountLogin_OnLoad` reads as versionType, buildType,
    // version, internalVersion, date and prints "Version 3.3.5 (12340) (Release x86)" over the
    // date. Labelled: the strings are the shipped login screen's, not read off the binding.
    set(lua, "GetBuildInfo", |lua, _| {
        Ok(MultiValue::from_vec(
            ["Version", "Release x86", "3.3.5", "12340", "Jun 24 2010"]
                .into_iter()
                .map(|s| lua.create_string(s).map(Value::String))
                .collect::<mlua::Result<_>>()?,
        ))
    })?;
    // The SSE check `AccountLogin`'s `OnShow` asks: a machine running benilla is supported.
    constant(lua, "IsSystemSupported", Some(1))?;
    // The select and create screens' model frames, wired by their own units: the name is kept.
    for name in ["SetCharSelectModelFrame", "SetCharCustomizeFrame"] {
        set(lua, name, |_, _| none())?;
    }

    // ── Inert on a 3.3.5a AzerothCore realm (labelled in the module doc) ──
    for name in ["IsTrialAccount", "IsStreamingTrial", "GetUsesToken"] {
        constant(lua, name, None)?;
    }
    // No notice to show: each reads as already accepted.
    for name in [
        "ShowTOSNotice",
        "ShowEULANotice",
        "ShowTerminationWithoutNoticeNotice",
        "ShowScanningNotice",
        "ShowContestNotice",
        "ShowChangedOptionWarnings",
    ] {
        constant(lua, name, None)?;
    }
    constant(lua, "IsScanDLLFinished", Some(1))?;
    for name in ["GetNumGameAccounts", "AccountMsg_GetNumUnreadMsgs"] {
        constant(lua, name, Some(0))?;
    }
    for name in [
        "GetSavedAccountList",
        "GetGameAccountInfo",
        "GetChangedOptionWarnings",
        "AccountMsg_GetIndexNextUnreadMsg",
        "AccountMsg_GetHeaderSubject",
    ] {
        constant(lua, name, None)?;
    }
    // Queries, not setters (`AccountLogin_ShowUserAgreements`): every agreement reads as
    // accepted, so the login panel shows and the TOS frame never does.
    for name in [
        "TOSAccepted",
        "EULAAccepted",
        "TerminationWithoutNoticeAccepted",
        "ScanningAccepted",
        "ContestAccepted",
    ] {
        constant(lua, name, Some(1))?;
    }
    for name in [
        "ScanDLLStart",
        "ScanDLLContinueAnyway",
        "SetSavedAccountList",
        "SetGameAccount",
        "SetUsesToken",
        "TokenEntered",
        "PINEntered",
        "MatrixEntered",
        "MatrixRevert",
        "MatrixCommit",
        "SurveyNotificationDone",
        "AccountMsg_LoadHeaders",
        "AccountMsg_LoadBody",
        "AccountMsg_SetMsgRead",
        "SetClearConfigData",
        "LaunchURL",
        "ResetLights",
        "PlayCreditsMusic",
        "PlayGlueMusic",
        "PlayGlueAmbience",
        "StopGlueMusic",
        "StopGlueAmbience",
        "StopAllSFX",
    ] {
        set(lua, name, |_, _| none())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::GlueRequest as R;
    use crate::script::UiScript;

    fn vm() -> UiScript {
        let s = UiScript::new().unwrap();
        s.install_glue_api().unwrap();
        s
    }

    #[test]
    fn the_login_calls_queue_for_app_in_order() {
        let mut s = vm();
        s.run("DefaultServerLogin('Bob', 'hunter2') StatusDialogClick() CancelLogin() SetSavedAccountName('Bob') QuitGame()")
            .unwrap();
        let requests = s.take_glue_requests();
        assert_eq!(
            requests,
            vec![
                R::Login {
                    account: "Bob".into(),
                    password: "hunter2".into()
                },
                R::StatusDialogClick,
                R::CancelLogin,
                R::SaveAccountName("Bob".into()),
                R::Quit,
            ]
        );
        assert!(
            !format!("{requests:?}").contains("hunter2"),
            "Debug never prints it"
        );
    }

    #[test]
    fn the_reads_answer_what_app_seeds() {
        let mut s = vm();
        s.set_glue_saved_account("Alice");
        s.set_glue_connection(true, "AzerothCore");
        s.set_glue_num_characters(3);
        s.run("local a, b, v, i, d = GetBuildInfo(); assert(a == 'Version' and v == '3.3.5' and i == '12340' and d == 'Jun 24 2010'); assert(GetSavedAccountName() == 'Alice'); assert(IsConnectedToServer() == 1); assert(GetServerName() == 'AzerothCore'); assert(GetNumCharacters() == 3); assert(GetClientExpansionLevel() == 2); assert(not IsStreamingTrial()); assert(not ShowTOSNotice()); assert(TOSAccepted() == 1 and EULAAccepted() == 1); SetCurrentScreen('login')")
            .unwrap();
        assert_eq!(s.glue_current_screen(), "login");
    }

    #[test]
    fn the_in_game_vm_has_no_glue_api() {
        let s = UiScript::new().unwrap();
        assert!(s.eval::<bool>("return DefaultServerLogin == nil").unwrap());
    }
}
