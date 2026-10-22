//! Death and resurrection, the world half: the wire-fed stores ([`DeathNet`], [`net`]) and the
//! ghost-world look. The Lua feed and the intent drain are `ui_death`'s.

use bevy::prelude::*;

use crate::net::{ObjectStore, SelfPlayer};

pub(crate) mod net;

pub(crate) use crate::app_core::death::CorpsePoint;

pub(crate) use crate::app_core::death::ResurrectOffer;

pub(crate) use crate::app_core::death::DeathNet;

/// Drive the FFXDeath screen pass off `PLAYER_FLAGS_GHOST`, instant both ways; in the reference
/// the flag's watcher `0x5ee990` also drives the death light and ambience.
fn drive_death_look(
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    mut ffx: ResMut<wrathplus_world::ffx_glow::FfxDeathFade>,
) {
    let ghost = ghost_probe()
        .unwrap_or_else(|| self_q.single().is_ok_and(|store| store.0.player_is_ghost()));
    let target = if ghost { 1.0 } else { 0.0 };
    if ffx.0 != target {
        ffx.0 = target;
    }
}

/// `WOW_GHOST_PROBE=1|0`: pin the ghost-world look on or off without dying: the screen pass,
/// the death light (`LightParams` slot 4) and the DeathClouds sky. The death events still follow
/// the wire.
pub(crate) fn ghost_probe() -> Option<bool> {
    static PROBE: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
    *PROBE.get_or_init(|| match std::env::var("WOW_GHOST_PROBE").ok()?.trim() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    })
}

/// The death stores, the root and water-walk messages, and the ghost look.
pub(crate) struct DeathPlugin;

impl Plugin for DeathPlugin {
    fn build(&self, app: &mut App) {
        net::register(app);
        app.init_resource::<DeathNet>()
            .add_systems(Update, drive_death_look);
    }
}
