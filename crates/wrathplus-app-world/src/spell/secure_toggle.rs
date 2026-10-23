//! The `secureAbilityToggle` guard on the press-again cancel. Build 12340's cast entry (`0x80da80`)
//! takes a lit spell's re-press as a cancel (`0x802f80`) unless the CVar is set (`[DAT_00bd097c +
//! 0x30]`, read at `0x80daec`) and the list at `0xd3f5ac` holds `(spell, 0)` younger than 1500 ms
//! (`0x807d40`, and `0x809100` asks the same); then the press does nothing. A double-click on an
//! aura just cast therefore does not cancel it.
//!
//! The writer is inferred: `0x807980`, a method on the same list taking `(spell, item, …)`, is
//! called by three small cast wrappers (`0x809000` with item 0, `0x809030`, `0x8090c0`), which reads
//! as "record a cast request with its time". Here the request is recorded when a press goes on to
//! cast ([`RecentCastRequests::record`]).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use bevy::prelude::*;

/// `0x807d40`'s window: `(now - node.time) - 0x5dc < 0`.
pub const SECURE_TOGGLE_WINDOW: Duration = Duration::from_millis(1500);

/// The CVar row this guard owns ([`crate::cvars::RegisterCvars`]).
pub const CVARS: &[crate::cvars::Registered] = {
    use crate::cvars::same;
    // Registered at `0x51e3c9` with default `0x9e1464`, the string "1" (`lockActionBars` shares it).
    &[same("secureAbilityToggle", "1")]
};

/// The CVar's value and each spell's last cast request: the list `0x807d40` scans.
#[derive(Resource, Debug)]
pub struct RecentCastRequests {
    enabled: bool,
    requested: HashMap<u32, Instant>,
}

impl Default for RecentCastRequests {
    fn default() -> Self {
        Self {
            enabled: true,
            requested: HashMap::new(),
        }
    }
}

impl RecentCastRequests {
    /// A press of `spell_id` went on to cast.
    pub fn record(&mut self, spell_id: u32, now: Instant) {
        self.requested
            .retain(|_, &mut t| now.duration_since(t) < SECURE_TOGGLE_WINDOW);
        self.requested.insert(spell_id, now);
    }

    /// The guard holds a lit spell's re-press: the CVar is set and its request is under 1500 ms old.
    pub fn blocks(&self, spell_id: u32, now: Instant) -> bool {
        self.enabled
            && self
                .requested
                .get(&spell_id)
                .is_some_and(|&t| now.duration_since(t) < SECURE_TOGGLE_WINDOW)
    }
}

/// `secureAbilityToggle`'s change callback: a flag.
pub fn on_cvar(ev: On<crate::cvars::CvarChanged>, mut recent: ResMut<RecentCastRequests>) {
    if ev.is("secureAbilityToggle") {
        recent.enabled = ev.flag();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_holds_its_re_press_for_1500_ms_while_the_cvar_is_set() {
        let t0 = Instant::now();
        let mut recent = RecentCastRequests::default();
        assert!(!recent.blocks(465, t0), "nothing requested yet");
        recent.record(465, t0);
        assert!(recent.blocks(465, t0 + Duration::from_millis(1499)));
        assert!(
            !recent.blocks(465, t0 + SECURE_TOGGLE_WINDOW),
            "the window has passed"
        );
        assert!(!recent.blocks(7294, t0), "only the spell requested");
        recent.enabled = false;
        assert!(!recent.blocks(465, t0), "the CVar off: no guard");
    }
}
