//! Native START lifecycle: wait for successful template metadata, never timeout-show.
//! Reference build 12340 `0x6fb2c0` / success callback `0x6fb1f0`.
use benilla_protocol::wrath_roll::{Item, Roll};

#[derive(Default)]
pub(super) struct Lifecycle {
    pub entries: Vec<Entry>,
    next_id: u32,
    pub cancelled: Vec<u32>,
}
pub(super) struct Entry {
    pub id: u32,
    pub item: Item,
    pub map: u32,
    pub count: u32,
    pub countdown_ms: u32,
    pub remaining_ms: u32,
    pub vote_mask: u8,
    pub announced: bool,
}
impl Lifecycle {
    pub fn receive(&mut self, roll: Roll) {
        match roll {
            Roll::Start {
                item,
                map,
                count,
                countdown_ms,
                vote_mask,
            } => {
                // A retransmission does not restart the timer or reuse an id.
                if self
                    .entries
                    .iter()
                    .any(|r| r.item.guid == item.guid && r.item.slot == item.slot)
                {
                    return;
                }
                let Some(id) = self.next_id.checked_add(1) else {
                    return;
                };
                self.next_id = id;
                self.entries.push(Entry {
                    id,
                    item,
                    map,
                    count,
                    countdown_ms,
                    remaining_ms: countdown_ms,
                    vote_mask,
                    announced: false,
                });
            }
            Roll::Won { item, .. } | Roll::AllPassed { item } => {
                self.entries.retain(|r| {
                    let remove = r.item.guid == item.guid && r.item.slot == item.slot;
                    if remove && r.announced {
                        self.cancelled.push(r.id);
                    }
                    !remove
                });
            }
            Roll::Vote { .. } => {}
        }
    }
    pub fn tick(&mut self, elapsed_ms: u32) {
        for entry in &mut self.entries {
            entry.remaining_ms = entry.remaining_ms.saturating_sub(elapsed_ms);
        }
    }
    /// None is still pending (including unknown ownership reasons); false is an explicit
    /// query failure. A success may announce once only, with the original wire countdown.
    pub fn prepare(
        &mut self,
        mut readiness: impl FnMut(&Entry) -> Option<bool>,
    ) -> Vec<(u32, u32)> {
        let mut starts = Vec::new();
        self.entries.retain_mut(|entry| {
            if entry.announced {
                return true;
            }
            match readiness(entry) {
                Some(false) => false,
                Some(true) => {
                    entry.announced = true;
                    starts.push((entry.id, entry.countdown_ms));
                    true
                }
                None => true,
            }
        });
        starts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn start() -> Roll {
        Roll::Start {
            item: Item {
                guid: 42,
                slot: 7,
                entry: 2589,
                suffix_factor: 876,
                random_property: -17,
            },
            map: 571,
            count: 999,
            countdown_ms: 60000,
            vote_mask: 0x85,
        }
    }
    #[test]
    fn pending_never_timeout_shows_success_once_retains_native_authority() {
        let mut state = Lifecycle::default();
        state.receive(start());
        state.tick(5000);
        state.receive(start());
        assert!(state.prepare(|_| None).is_empty());
        let entry = &state.entries[0];
        assert_eq!(
            (
                entry.id,
                entry.map,
                entry.count,
                entry.vote_mask,
                entry.item.suffix_factor
            ),
            (1, 571, 999, 0x85, 876)
        );
        assert_eq!(entry.remaining_ms, 55000);
        assert_eq!(state.prepare(|_| Some(true)), vec![(1, 60000)]);
        assert!(state.prepare(|_| Some(true)).is_empty());
        state.tick(u32::MAX);
        assert_eq!(state.entries[0].remaining_ms, 0);
        assert!(state.entries[0].announced);
    }
    #[test]
    fn failed_query_drops_without_start_or_cancel_and_late_success_cannot_resurrect() {
        let mut state = Lifecycle::default();
        state.receive(start());
        assert!(state.prepare(|_| Some(false)).is_empty());
        assert!(state.entries.is_empty() && state.cancelled.is_empty());
        assert!(state.prepare(|_| Some(true)).is_empty());
    }
    #[test]
    fn resolution_cancels_only_announced_rolls_and_ids_are_not_reused() {
        for announced in [false, true] {
            let mut state = Lifecycle::default();
            state.receive(start());
            if announced {
                state.prepare(|_| Some(true));
            }
            let Roll::Start { item, .. } = start() else {
                unreachable!()
            };
            state.receive(Roll::AllPassed { item });
            assert!(state.entries.is_empty());
            assert_eq!(state.cancelled, if announced { vec![1] } else { vec![] });
            state.receive(start());
            assert_eq!(state.entries[0].id, 2);
        }
    }
}
