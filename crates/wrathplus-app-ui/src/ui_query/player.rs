//! The player-state half of the snapshot: the local player's descriptor and movement word, the
//! area's flags, and each unit's held speed and sanctuary bit, which `IsMounted`, `IsFlying`,
//! `IsFlyableArea`, `GetUnitSpeed` and `UnitIsPVPSanctuary` read in the call.

use wrathplus_formats::UnitMotion;
use wrathplus_ui::script::PlayerState;

use super::FrameQuery;
use crate::app_core::movement::move_flags;

/// AreaTable flags `0x51a1d0` tests (AzerothCore `AreaFlags`): an expansion zone, and its
/// no-fly exception.
const AREA_FLAG_OUTLAND: u32 = 0x400;
const AREA_FLAG_NO_FLY_ZONE: u32 = 0x2000_0000;
/// Northrend (`0x7261da`) and Cold Weather Flying (`0x7261e6`), which flight there needs.
const MAP_NORTHREND: u32 = 571;
pub(super) const COLD_WEATHER_FLYING: u32 = 54197;

/// The world facts `IsFlyableArea` reads, one frame's.
#[derive(Default)]
pub(super) struct AreaFacts {
    /// The current area's AreaTable flags (`[0xbd0810]`'s row, else `[0xbd080c]`'s,
    /// `0x51a1d0`), taken as the leaf the area resolver names; `None` with no row.
    pub flags: Option<u32>,
    /// The current map (`[0xbd088c]`).
    pub map: u32,
}

/// `0x9880c0`: the cached speed `GetUnitSpeed` reads is refreshed on every movement change but
/// not while FALLING, so a jump holds its take-off speed. `held` carries it between frames.
pub(super) fn held_speed(held: &mut f32, motion: &UnitMotion) -> f32 {
    if motion.flags & move_flags::FALLING == 0 {
        *held = motion.speed;
    }
    *held
}

impl FrameQuery {
    /// The no-argument verbs' facts of the local player: `[player+0x9c0] > 0` (`0x6125c2`),
    /// `BYTES_1` byte 2 `& 2` (`0x612638`), and the movement word's SWIMMING (`0x6124c2`, the
    /// `[player+0xa30]` cache `0x730eb2` copies from it) and FLYING (`0x61255b`; never the wire
    /// literal, as the word holds FLYING in a bit of its own). Labelled: `IsMounted`'s
    /// deferred-dismount latch (`[player+0xa30] & 0x10000000`, `0x740487`) and `IsFlying`'s
    /// vehicle leg (`[player+0xf60]`, the vehicle's word) are not read.
    pub fn player_state(&self) -> Option<PlayerState> {
        let me = self.me.as_ref()?;
        let flags = me.range.motion.flags;
        Some(PlayerState {
            mounted: me.store.0.unit_mount_display_id() as i32 > 0,
            stealthed: me.store.0.unit_is_stealthed(),
            swimming: flags & move_flags::SWIMMING != 0,
            flying: flags & move_flags::FLYING != 0,
        })
    }

    /// `IsFlyableArea` (`0x612260`): the area flags carry OUTLAND and not NO_FLY_ZONE
    /// (`0x51a212`, `0x51a219`); with the player, on Northrend Cold Weather Flying must be known
    /// (`0x7261b0`); without one only the area test (`0x6122bd`). No riding skill is read.
    pub fn flyable_area(&self) -> Option<bool> {
        let flags = self.area.flags.unwrap_or(0);
        let area = flags & AREA_FLAG_OUTLAND != 0 && flags & AREA_FLAG_NO_FLY_ZONE == 0;
        let grounded = |me: &super::Me| self.area.map == MAP_NORTHREND && !me.cold_weather;
        Some(area && !self.me.as_ref().is_some_and(grounded))
    }

    /// `GetUnitSpeed`'s cached speed of a streamed unit, ours held through a fall
    /// ([`held_speed`]). Labelled: another unit's is its live speed, unheld.
    pub fn unit_speed(&self, unit: u64) -> Option<f32> {
        self.units.get(&unit).map(|row| row.speed)
    }

    /// `UnitIsPVPSanctuary`'s bit of a streamed unit.
    pub fn unit_sanctuary(&self, unit: u64) -> Option<bool> {
        self.units.get(&unit).map(|row| row.sanctuary)
    }
}
