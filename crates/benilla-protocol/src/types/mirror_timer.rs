//! Mirror timers: the breath, fatigue and feign-death bars. The server owns the countdown and the
//! client integrates it between packets. Any change is re-sent as a full `START`
//! (`Player::SendMirrorTimers`), so `START` also re-states a running timer; vmangos never sends
//! `PAUSE`, whose stock `MirrorTimer.lua` handler errors on it.

/// The wire's `timerType` (vmangos `Objects/MirrorTimer.h`), also the reference client's index
/// into its 3-entry timer name table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorTimerKind {
    /// `0`: swimming in deep, uncharted water.
    Fatigue,
    /// `1`: head under the surface.
    Breath,
    /// `2`: feigning death; its duration is zero unless a script sets it, so it rarely shows.
    FeignDeath,
}

impl MirrorTimerKind {
    /// The server's `NUM_CLIENT_TIMERS` gate keeps its internal `ENVIRONMENTAL` (3) off the wire.
    pub fn from_wire(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Fatigue),
            1 => Some(Self::Breath),
            2 => Some(Self::FeignDeath),
            _ => None,
        }
    }
}

/// `SMSG_START_MIRROR_TIMER`: starts, or wholly re-states, one mirror timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MirrorTimerStart {
    /// The raw `timerType`, left unmapped so an unknown one still decodes.
    pub kind: u32,
    /// Time left in ms, counting toward 0 while draining and toward `duration_ms` while refilling.
    pub remaining_ms: u32,
    /// The bar's maximum in ms (breath: 60 s by default, `CONFIG_UINT32_MIRRORTIMER_BREATH_MAX`).
    pub duration_ms: u32,
    /// Signed rate in bar seconds per second: `-1` while draining, `+10` while refilling after
    /// surfacing. The client integrates `scale * elapsed` between packets.
    pub scale: i32,
    /// Frozen: the bar holds its value and stops integrating.
    pub paused: bool,
    /// The spell driving the timer (water breathing) or `0`; the reference Lua never sees it.
    pub spell_id: u32,
}
