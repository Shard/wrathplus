//! Native channel display metadata, independent of the Vanilla slot-state approximation.
//!
//! Wrath allocator 0x5076b0 initializes +0x9c=1/+0xa0=0; confirmation 0x507880
//! writes only +0x9c=0. Explicit leave 0x5093f0 writes +0xa0=1 before the reply.
//! Handler 0x50e120 retains a suspended row only while +0xa0=0; otherwise removes it.
//! This ledger contains observed confirmations only, never speculative walk allocations.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Membership {
    pub slot: u32,
    pub name: String,
    pub channel_id: u32,
    pub active: bool,
    pub custom_state: u32,
}

#[derive(bevy::prelude::Resource, Default)]
pub(super) struct NativeMembership {
    rows: Vec<Membership>,
}

impl NativeMembership {
    /// Constant channels retain their identity across zone-name replacements.
    pub fn replacement_name(&self, channel_id: u32) -> Option<&str> {
        self.rows
            .iter()
            .find(|row| channel_id != 0 && row.channel_id == channel_id && row.custom_state == 0)
            .map(|row| row.name.as_str())
    }

    /// Confirmation cannot allocate a missing slot. The caller resolves the actual slot
    /// after the existing chat lifecycle; no catalog or guessed slot identities are used.
    pub fn joined(&mut self, name: &str, channel_id: u32, slot: Option<u32>) {
        let Some(slot) = slot.filter(|slot| *slot != 0) else {
            return;
        };
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.name.eq_ignore_ascii_case(name))
        {
            row.slot = slot;
            row.name = name.to_owned();
            row.channel_id = channel_id;
            row.active = true;
            // Confirmation does not write +0xa0: an explicit pending leave stays hidden.
            return;
        }
        self.rows.retain(|row| row.slot != slot);
        self.rows.push(Membership {
            slot,
            name: name.to_owned(),
            channel_id,
            active: true,
            custom_state: 0,
        });
    }

    /// Only the resolved explicit local leave path calls this, not every outbound leave.
    pub fn explicit_leave(&mut self, name: &str) {
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.name.eq_ignore_ascii_case(name))
        {
            row.custom_state = 1;
        }
    }

    /// Preserve the raw trailing byte's native suspension semantics. Unknown replies
    /// cannot manufacture rows, and a pending explicit leave overrides suspension.
    pub fn left(&mut self, name: &str, suspended: u8) {
        let Some(index) = self
            .rows
            .iter()
            .position(|row| row.name.eq_ignore_ascii_case(name))
        else {
            return;
        };
        if suspended != 0 && self.rows[index].custom_state == 0 {
            self.rows[index].active = false;
        } else {
            self.rows.remove(index);
        }
    }

    pub fn retains(&self, name: &str) -> bool {
        self.rows
            .iter()
            .any(|row| row.name.eq_ignore_ascii_case(name))
    }

    pub fn clear(&mut self) {
        self.rows.clear();
    }

    /// Re-associate only with extant name-matching slots, preserving the current slot order.
    pub fn snapshot(&self, mut slot_for: impl FnMut(&str) -> Option<u32>) -> Vec<Membership> {
        let mut rows: Vec<_> = self
            .rows
            .iter()
            .filter_map(|row| {
                let slot = slot_for(&row.name).filter(|slot| *slot != 0)?;
                Some(Membership {
                    slot,
                    ..row.clone()
                })
            })
            .collect();
        rows.sort_by_key(|row| row.slot);
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_requires_actual_slot_and_preserves_raw_id() {
        let mut ledger = NativeMembership::default();
        ledger.joined("General", 9876, None);
        ledger.joined("General", 9876, Some(0));
        assert!(ledger.rows.is_empty());
        ledger.joined("General", 9876, Some(2));
        assert_eq!(
            ledger.rows[0],
            Membership {
                slot: 2,
                name: "General".into(),
                channel_id: 9876,
                active: true,
                custom_state: 0,
            }
        );
    }

    #[test]
    fn replacement_identity_excludes_custom_and_explicit_pending_leave() {
        let mut ledger = NativeMembership::default();
        ledger.joined("Custom", 0, Some(2));
        ledger.joined("General - Ironforge", 1, Some(1));
        assert_eq!(ledger.replacement_name(0), None);
        assert_eq!(ledger.replacement_name(22), None);
        assert_eq!(ledger.replacement_name(1), Some("General - Ironforge"));
        ledger.left("General - Ironforge", 1);
        assert_eq!(ledger.replacement_name(1), Some("General - Ironforge"));
        ledger.joined("General - Dun Morogh", 1, Some(1));
        assert!(!ledger.retains("General - Ironforge"));
        assert_eq!(ledger.snapshot(|_| Some(1)).len(), 2);
        ledger.explicit_leave("General - Dun Morogh");
        assert_eq!(ledger.replacement_name(1), None);
    }

    #[test]
    fn suspension_retains_known_row_but_true_leave_removes() {
        let mut ledger = NativeMembership::default();
        ledger.left("unknown", 255);
        assert!(ledger.rows.is_empty());
        ledger.joined("General", 1, Some(1));
        ledger.left("GENERAL", 255);
        assert!(!ledger.rows[0].active);
        assert_eq!(ledger.rows[0].custom_state, 0);
        ledger.joined("General", 1, Some(1));
        assert!(ledger.rows[0].active);
        ledger.left("General", 0);
        assert!(ledger.rows.is_empty());
    }

    #[test]
    fn explicit_leave_hides_before_reply_and_overrides_suspension() {
        let mut ledger = NativeMembership::default();
        ledger.joined("custom", 0, Some(3));
        ledger.explicit_leave("CUSTOM");
        assert_eq!(ledger.rows[0].custom_state, 1);
        ledger.joined("custom", 0, Some(3)); // confirmation leaves +0xa0 unchanged
        assert_eq!(ledger.rows[0].custom_state, 1);
        ledger.left("custom", 1);
        assert!(ledger.rows.is_empty());
    }

    #[test]
    fn snapshots_require_current_slots_and_reset_drops_membership() {
        let mut ledger = NativeMembership::default();
        ledger.joined("gone", 1, Some(1));
        ledger.joined("present", 0, Some(2));
        let rows = ledger.snapshot(|name| (name == "present").then_some(4));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].slot, 4);
        ledger.clear();
        assert!(ledger.snapshot(|_| Some(1)).is_empty());
    }
}
