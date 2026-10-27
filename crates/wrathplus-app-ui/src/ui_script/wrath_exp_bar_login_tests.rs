//! The experience bar at login, relog and `/reload`: stock shows it only from
//! `ReputationWatchBar_Update` (`ReputationFrame.lua:323-413`), which the watch bar runs on
//! `UPDATE_FACTION`; our reputation feed fires that once per VM, on its first push.

use bevy::prelude::*;
use wrathplus_ui::script::UiScript;

/// A stock session as world entry builds it, the player at level 10 with 2500 of 7600 xp.
fn session(data: &std::path::Path) -> UiScript {
    let (mut s, _failures) = super::super::stock_session(data);
    s.clear_instruction_budget();
    s.set_unit("player", Some(super::super::stock_player(10)));
    s.set_player_xp(2_500, 7_600);
    s.fire_event("PLAYER_XP_UPDATE", vec![]);
    s
}

fn exp_bar(app: &App) -> String {
    app.world()
        .non_send_resource::<UiScript>()
        .eval(
            "local lo, hi = MainMenuExpBar:GetMinMaxValues() \
             return tostring(MainMenuExpBar:IsShown())..' '..lo..'/'..hi..' '..MainMenuExpBar:GetValue()",
        )
        .unwrap()
}

/// The real feed over a loaded VM: the in-world VM is minted and loaded in one exclusive pass
/// (`lifecycle::load_ingame_ui_on_world_entry`) with the boot VM parked meanwhile, so the feed's
/// first push meets the loaded watch bar. Each new VM (a relog, a `/reload`) gets its own first
/// push, its memo keyed on the VM's session; each shows the bar with the player's values.
#[test]
fn login_relog_and_reload_each_show_the_exp_bar() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
    let catalog = wrathplus_formats::load_faction_catalog(&mut chain).expect("factions");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(crate::net::Reputations::default())
        .insert_resource(crate::app_core::state::Factions(catalog))
        .add_systems(Update, crate::ui_reputation::feed_reputation);
    app.world_mut().spawn((
        crate::app_core::session::ObjectStore(Default::default()),
        crate::app_core::session::SelfPlayer,
    ));
    for round in ["login", "relog", "reload"] {
        app.insert_non_send_resource(session(&data));
        app.update();
        assert_eq!(exp_bar(&app), "1 0/7600 2500", "{round}");
        let errors = app.world().non_send_resource::<UiScript>().errors();
        assert!(errors.is_empty(), "{round}: {errors:?}");
    }
}
