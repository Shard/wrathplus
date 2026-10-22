//! The stock glue interface: build 12340's `Interface\GlueXML\GlueXML.toc` over the chain, the
//! screens `GlueParent.lua` switches between (`AccountLogin`, `CharacterSelect`, …), on its own VM
//! outside the world. The loader is FrameXML's ([`reference_ui::core_at`]) on the glue toc.

use benilla_ui::script::{ScriptValue, UiScript};
use bevy::prelude::*;

use super::reference_ui;

mod host;
pub(crate) use host::stock_glue;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod installed_tests;

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

/// The run condition of a native glue screen's systems: the stock screens are off.
pub(crate) fn native_glue() -> bool {
    !stock_glue_active()
}

/// UI input's run condition: the in-game UI in the world, or a screen the stock glue hosts.
pub(crate) fn ui_input_live(state: Option<Res<State<crate::char_select::ClientState>>>) -> bool {
    use crate::char_select::ClientState;
    let state = state.map(|s| *s.get());
    state == Some(ClientState::InWorld)
        || (stock_glue_active()
            && matches!(
                state,
                Some(ClientState::Login | ClientState::CharSelect | ClientState::CharCreate)
            ))
}

/// Run condition: the stock glue hosts the screen (login, realm list, character select and, with
/// Native UI's host, character create).
pub(crate) fn stock_glue_up(state: Option<Res<State<crate::char_select::ClientState>>>) -> bool {
    use crate::char_select::ClientState;
    stock_glue_active()
        && state.is_some_and(|s| {
            matches!(
                s.get(),
                ClientState::Login | ClientState::CharSelect | ClientState::CharCreate
            )
        })
}

/// The session of the glue VM in the world, if the VM there is one: kept across the glue screens,
/// as the reference's glue Lua state is; the world's own load replaces it at entry and a logout's
/// boot VM ends it, so the next glue screen builds a fresh one.
#[derive(Resource, Default)]
pub(crate) struct GlueVm(Option<u64>);

/// The glue VM in the world, building it if the VM there is not the glue's: a fresh VM with the
/// glue API, GlueXML loaded silenced (as the in-game load is), then GlueParent's start,
/// `FRAMES_LOADED`. Returns whether a glue VM is in place.
fn ensure_glue_vm(world: &mut World) -> bool {
    let current = world
        .get_non_send_resource::<UiScript>()
        .map(UiScript::session);
    if current.is_some() && world.get_resource::<GlueVm>().and_then(|g| g.0) == current {
        return true;
    }
    let Some(mut script) = super::lifecycle::new_vm(world) else {
        return false;
    };
    if let Err(e) = script.install_glue_api() {
        error!("ui_script: glue API install failed: {e}");
        return false;
    }
    super::lifecycle::seat_raster_seam_for_load(world, &mut script);
    script.set_glue_saved_account(&crate::login::load_saved_account());
    let (server, pinned) = world
        .get_resource::<crate::realmlist::Realmlist>()
        .map(|r| (r.address().to_string(), r.pinned_by_env()))
        .unwrap_or_default();
    script.set_glue_connection(false, &server);
    script.set_glue_realmlist(&server, pinned);
    super::manifest::silenced_ui_load(&mut script, |script| {
        for failure in load_glue_ui(script) {
            warn!("ui_script: GlueXML load: {failure}");
        }
    });
    script.fire_event("FRAMES_LOADED", vec![]);
    world.insert_resource(GlueVm(Some(script.session())));
    world.insert_non_send_resource(script);
    true
}

/// `SET_GLUE_SCREEN name` on the glue VM in the world.
fn set_glue_screen(world: &mut World, name: &str) {
    if let Some(mut script) = world.get_non_send_resource_mut::<UiScript>() {
        script.fire_event("SET_GLUE_SCREEN", vec![ScriptValue::Str(name.into())]);
    }
}

/// Entering the login screen with the stock glue: the glue VM, then the engine's
/// `SET_GLUE_SCREEN "login"` and the login's backdrop, AccountLogin's ModelFFX pane sampling the
/// glue booth's `UI_MainMenu_Northrend` (`portrait::glue_booth`).
pub(crate) fn enter_stock_login(world: &mut World) {
    if !stock_glue_active() || !ensure_glue_vm(world) {
        return;
    }
    set_glue_screen(world, "login");
    if let Some(mut preview) = world.get_resource_mut::<crate::portrait::GluePreview>() {
        preview.scene = Some(crate::portrait::GlueScene::MainMenu);
        preview.look = None;
        preview.yaw = 0.0;
    }
    info!("ui_script: stock glue — AccountLogin up");
}

/// Entering character select with the stock glue: the glue VM (rebuilt after a logout), the rows
/// it reads, then `SET_GLUE_SCREEN "charselect"`; the stage comes from the stock Lua's own
/// `SetCharSelectBackground`.
pub(crate) fn enter_stock_select(world: &mut World) {
    if !stock_glue_active() || !ensure_glue_vm(world) {
        return;
    }
    push_characters(world);
    set_glue_screen(world, "charselect");
    info!("ui_script: stock glue — CharacterSelect up");
}

/// The roster's rows onto the glue VM.
fn push_characters(world: &mut World) {
    let rows = {
        let roster = world.resource::<crate::char_select::Roster>();
        crate::char_select::glue_rows::glue_characters(
            roster,
            world.get_resource::<crate::area::AreaTableRes>(),
        )
    };
    if let Some(mut script) = world.get_non_send_resource_mut::<UiScript>() {
        script.set_glue_characters(rows, crate::char_select::glue_rows::EMPTY_SELECT_STAGE);
    }
}
