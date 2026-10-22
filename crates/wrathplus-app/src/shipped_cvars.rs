//! The shipped CVar table without an app: the core rows ([`crate::cvars::REGISTERED`]) and every
//! feature's own ([`crate::cvars::RegisterCvars`]). For the code that seeds a VM with no registry
//! behind it (the addon harness, the capture fixtures) and the tests that judge every row against
//! the reference; `game_plugins::client_tests::shipped_cvars` pins it to what a built client
//! registers.

use crate::cvars::{Registered, REGISTERED};

/// Each feature's rows, by module.
pub(crate) const FEATURE_ROWS: &[&[Registered]] = &[
    crate::char_select::CVARS,
    crate::game_tip::CVARS,
    crate::minimap::CVARS,
    crate::player::approach::CVARS,
    crate::player::camera_view::CVARS,
    crate::portrait::CVARS,
    crate::realmlist::CVARS,
    crate::text_filter::CVARS,
    crate::ui_chat::combat::CVARS,
    crate::ui_gamma::CVARS,
    crate::ui_guild::CVARS,
    crate::ui_loot::CVARS,
    crate::ui_paperdoll::CVARS,
    crate::ui_trade::CVARS,
    crate::ui_trainer::net::CVARS,
    crate::video::CVARS,
    crate::world_backdrop::CVARS,
    crate::world_state_ui::CVARS,
];

/// Every shipped row: the core table, then the features'.
pub(crate) fn rows() -> impl Iterator<Item = &'static Registered> {
    REGISTERED
        .iter()
        .chain(FEATURE_ROWS.iter().flat_map(|rows| rows.iter()))
}

/// Every shipped row's name and default, as a VM's `register_cvars` takes them.
pub(crate) fn pairs() -> impl Iterator<Item = (&'static str, &'static str)> {
    rows().map(|r| (r.name, r.default))
}
