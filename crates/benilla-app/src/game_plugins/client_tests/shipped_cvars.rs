//! The registry a built client holds is the shipped table ([`crate::shipped_cvars`]): the core
//! rows and every feature's own, each once and at its default. The features register their rows
//! and their `on_cvar` observers in their own plugins, so both orders follow the plugins'; these
//! pin what that order may change and what it may not.

use bevy::ecs::observer::Observer;

use crate::cvars::Cvars;

/// The latched rows in the order [`Cvars::commit_latched`] fires them: the sound-init row, then
/// the `gx*` block `RestartGx` commits, as the core table listed them before the features owned
/// their rows.
const LATCHED_ORDER: [&str; 13] = [
    "SoundBufferSize",
    "gxApi",
    "gxVSync",
    "gxWindow",
    "gxResolution",
    "gxMultisample",
    "gxColorBits",
    "gxDepthBits",
    "gxMaximize",
    "gxTripleBuffer",
    "gxCursor",
    "gxFixLag",
    "gxRefresh",
];

#[test]
fn a_built_client_registers_exactly_the_shipped_cvar_rows() {
    let app = crate::game_plugins::schedule_tests::headless_client();
    let cvars = app.world().resource::<Cvars>();
    let row =
        |name: &str, default: &str, latched: bool| (name.to_string(), default.to_string(), latched);
    let mut built: Vec<_> = cvars
        .rows()
        .filter(|r| !r.addon)
        .map(|r| row(&r.name, &r.default, r.latched))
        .collect();
    let mut shipped: Vec<_> = crate::shipped_cvars::rows()
        .map(|r| row(r.name, r.default, r.latched))
        .collect();
    built.sort();
    shipped.sort();
    assert_eq!(
        built, shipped,
        "the client registers the shipped table, no more, no less"
    );
    assert_eq!(shipped.len(), 138, "the shipped row count moved");

    let latched: Vec<&str> = cvars
        .rows()
        .filter(|r| r.latched)
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(
        latched, LATCHED_ORDER,
        "the latched rows commit in their old order"
    );
}

/// The `on_cvar` observers, in the order their plugins registered them: each feature attaches
/// its own, and a registration-order change would reorder the hooks a change fires.
#[test]
fn the_cvar_hooks_register_in_plugin_order() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let world = app.world_mut();
    let mut hooks: Vec<(u32, String)> = world
        .query::<(bevy::prelude::Entity, &Observer)>()
        .iter(world)
        .map(|(e, o)| (e.index_u32(), o.system_name().to_string()))
        .filter(|(_, name)| name.ends_with("::on_cvar"))
        .collect();
    hooks.sort();
    let names: Vec<&str> = hooks
        .iter()
        .map(|(_, n)| n.trim_start_matches("benilla_app::"))
        .collect();
    assert_eq!(
        names, HOOK_ORDER,
        "the CVar hooks' registration order moved"
    );
}

/// The order before the rows moved: no `add_observer` call moved with them. `perf`'s hook is
/// absent because the headless client builds no `dev` tools plugin.
const HOOK_ORDER: &[&str] = &[
    "player::approach::on_cvar",
    "player::camera::on_cvar",
    "player::camera_dynamics::on_cvar",
    "sound::on_cvar",
    "target::on_cvar",
    "ui_gamma::on_cvar",
    "world_backdrop::on_cvar",
    "minimap::on_cvar",
    "portrait::on_cvar",
    "ui_script::on_cvar",
    "video::on_cvar",
    "realmlist::on_cvar",
    "game_tip::on_cvar",
    "text_filter::on_cvar",
    "ui_guild::on_cvar",
    "spell::cast_target::on_cvar",
    "combat_text::on_cvar",
    "nameplates::on_cvar",
    "chat_bubble::on_cvar",
    "ui_trade::on_cvar",
    "ui_loot::on_cvar",
    "ui_chat::combat::on_cvar",
];
