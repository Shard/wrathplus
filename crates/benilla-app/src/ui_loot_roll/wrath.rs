//! Lossless native roll inbox. Presentation drains updates separately from the 1.12 roll model.
use crate::net::NetHandlerApp;
use benilla_protocol::{wrath_roll::Roll, SessionEvent, SessionEventKind};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(super) struct NativeRolls {
    /// A start carries the authoritative map/count/vote mask, never inferred from a template.
    pub active: Vec<Roll>,
    /// Pending native announcements retain auto-pass and disenchant rather than legacy projection.
    pub updates: std::collections::VecDeque<Roll>,
}

impl NativeRolls {
    fn receive(&mut self, roll: Roll) {
        match &roll {
            Roll::Start { item, .. } => {
                self.active.retain(|r| {
                    !matches!(r, Roll::Start { item: old, .. }
                    if old.guid == item.guid && old.slot == item.slot)
                });
                self.active.push(roll.clone());
            }
            Roll::Won { item, .. } | Roll::AllPassed { item } => {
                self.active.retain(|r| {
                    !matches!(r, Roll::Start { item: old, .. }
                    if old.guid == item.guid && old.slot == item.slot)
                });
            }
            Roll::Vote { .. } => {}
        }
        // Bounded until the native presentation consumer drains this queue.
        if self.updates.len() == 256 {
            self.updates.pop_front();
        }
        self.updates.push_back(roll);
    }
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeRolls>()
        .net_handler(SessionEventKind::WrathLootRoll, on_roll)
        .net_handler(SessionEventKind::Disconnected, on_reset);
}
fn on_roll(In(event): In<SessionEvent>, mut rolls: ResMut<NativeRolls>) {
    if let SessionEvent::WrathLootRoll(roll) = event {
        rolls.receive(roll);
    }
}
fn on_reset(In(_): In<SessionEvent>, mut rolls: ResMut<NativeRolls>) {
    *rolls = NativeRolls::default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::wrath_roll::Item;
    fn item() -> Item {
        Item {
            guid: 42,
            slot: 7,
            entry: 2589,
            suffix_factor: 876,
            random_property: -17,
        }
    }
    #[test]
    fn native_owner_retains_start_vote_resolution_and_clears_on_disconnect() {
        let mut app = App::new();

        register(&mut app);
        let start = Roll::Start {
            item: item(),
            map: 571,
            count: 1000,
            countdown_ms: 60000,
            vote_mask: 15,
        };
        let vote = Roll::Vote {
            item: item(),
            player: 88,
            number: 128,
            kind: 3,
            auto_pass: true,
        };
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![
                SessionEvent::WrathLootRoll(start.clone()),
                SessionEvent::WrathLootRoll(vote.clone()),
            ],
        );
        let rolls = app.world().resource::<NativeRolls>();
        assert_eq!(rolls.active, vec![start.clone()]);
        assert_eq!(
            rolls.updates.iter().cloned().collect::<Vec<_>>(),
            vec![start, vote]
        );
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::WrathLootRoll(Roll::Won {
                item: item(),
                player: 88,
                number: 100,
                kind: 3,
            })],
        );
        assert!(app.world().resource::<NativeRolls>().active.is_empty());
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::Disconnected {
                reason: "test".into(),
                end: benilla_protocol::SessionEnd::Lost,
            }],
        );
        assert!(app.world().resource::<NativeRolls>().updates.is_empty());
    }
}
