//! The instance lock's wire-order changes and `GetInstanceLockTimeRemaining`'s view
//! ([`super`]'s module docs).

use benilla_ui::script::WrathInstanceLock;

/// A pending instance lock: when it forces (`GetTime` seconds), whether accepting extends, and
/// the completed-encounter mask.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PendingLock {
    pub(super) expires_at: f64,
    pub(super) extending: bool,
    pub(super) completed_mask: u32,
}

impl PendingLock {
    /// `GetInstanceLockTimeRemaining`'s view, its encounter counts over `rows` of the instance
    /// (`00553830`).
    pub(crate) fn view(
        self,
        rows: &[benilla_ui::script::WrathLfgEncounter],
        instance: Option<(u32, u32)>,
    ) -> WrathInstanceLock {
        let (mut total, mut completed) = (0, 0);
        for row in rows {
            if instance != Some((row.map as u32, row.difficulty as u32)) {
                continue;
            }
            total += 1;
            if 1u32
                .checked_shl(row.bit & 31)
                .is_some_and(|b| self.completed_mask & b != 0)
            {
                completed += 1;
            }
        }
        WrathInstanceLock {
            expires_at: self.expires_at,
            extending: self.extending,
            encounters_completed: completed,
            encounters_total: total,
            completed_mask: self.completed_mask,
        }
    }
}

/// The lock's queued changes ([`crate::ui_instance::InstanceState::take_lock_changes`]) on the
/// `GetTime` clock: `Some` a set, `None` a clear.
pub(crate) fn lock_changes(raw: Vec<Option<(u32, u32, u8)>>, now: f64) -> Vec<Option<PendingLock>> {
    raw.into_iter()
        .map(|change| {
            change.map(|(timeout_ms, encounter_mask, raw_flag)| PendingLock {
                expires_at: now + f64::from(timeout_ms) / 1000.0,
                extending: raw_flag != 0,
                completed_mask: encounter_mask,
            })
        })
        .collect()
}
