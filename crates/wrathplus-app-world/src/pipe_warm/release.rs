//! The warm pass's release rule: hold the cover while it progresses, with two nets for a hang.

/// The pass holds the cover while it progresses, however slow the machine: a flat timeout released
/// it with rigs unrevealed on a loaded machine (8 of 10 probe runs, 2026-10-27), and what was left
/// compiled live in play. It releases on [`WARM_STALL_SECS`] with neither a rig revealed nor a
/// pipeline settled (a hung or erroring build), and at [`WARM_HARD_CAP_SECS`] whatever the progress.
pub(super) const WARM_STALL_SECS: f32 = 5.0;
/// The safety net: six times an unloaded drain (7–10 s), past any loaded one seen.
pub(super) const WARM_HARD_CAP_SECS: f32 = 60.0;

/// What the pass does this frame.
#[derive(Debug, PartialEq)]
pub(super) enum Verdict {
    Drained,
    Stalled,
    Capped,
    Wait,
}

/// The release rule, in seconds of `Time<Real>`: drained once every rig is revealed, nothing is
/// pending and the settle window has passed; else a stall or the cap; else wait.
pub(super) fn verdict(now: f32, spawned: f32, last_progress: f32, drained: bool) -> Verdict {
    if drained {
        Verdict::Drained
    } else if now - spawned >= WARM_HARD_CAP_SECS {
        Verdict::Capped
    } else if now - last_progress >= WARM_STALL_SECS {
        Verdict::Stalled
    } else {
        Verdict::Wait
    }
}
