//! `SMSG_PVP_CREDIT` as build 12340's `0x50b6d0` gates it: what queues for the chat line and
//! whether `HONOR_GAINED` fires.

use super::*;
use crate::app_core::chat::Pending;
use crate::app_core::state::Factions;
use bevy::ecs::system::RunSystemOnce;
use wrathplus_formats::{FactionCatalog, FactionTemplate};
use wrathplus_protocol::types::{Field, ObjectFields, PvpCredit};

const VICTIM: u64 = 20;
const HORDE_TEMPLATE: u32 = 2;
/// Each queued honor line's `(victim, honor, rank)`.
type Queued = Vec<(u64, i32, Option<(u8, Option<u8>)>)>;

fn app() -> App {
    let mut app = App::new();
    let (tx, _rx) = crossbeam_channel::unbounded();
    app.add_message::<CombatTextSpawn>()
        .add_message::<UnitCombatFeedback>()
        .add_message::<CombatTextEvent>()
        .init_resource::<GuidIndex>()
        .init_resource::<ChatLog>()
        .init_resource::<NameCache>()
        .insert_resource(DamageTextGates::default())
        .insert_resource(crate::net::NetCommands(tx))
        .insert_resource(Factions::from_catalog(FactionCatalog::from_rows(
            std::collections::HashMap::from([(
                HORDE_TEMPLATE,
                FactionTemplate {
                    group_mask: 5,
                    ..Default::default()
                },
            )]),
            std::collections::HashMap::new(),
        )));
    let template = Field::UnitFactionTemplate.span().unwrap().index;
    let fields = ObjectFields::from_pairs(&[(template, HORDE_TEMPLATE)]);
    let victim = app.world_mut().spawn(ObjectStore(fields)).id();
    app.world_mut()
        .resource_mut::<GuidIndex>()
        .0
        .insert(VICTIM, victim);
    app
}

/// The queued `(victim, honor, rank)` and whether `HONOR_GAINED` fired.
fn credit(app: &mut App, honor: i32, victim_guid: u64, victim_rank: i32) -> (Queued, bool) {
    let ev = SessionEvent::PvpCredit(PvpCredit {
        honor,
        victim_guid,
        victim_rank,
    });
    app.world_mut()
        .run_system_once_with(on_pvp_credit, ev)
        .unwrap();
    let queued = std::mem::take(&mut app.world_mut().resource_mut::<ChatLog>().pending)
        .into_iter()
        .filter_map(|p| match p {
            Pending::HonorGain {
                victim,
                honor,
                rank,
                ..
            } => Some((victim, honor, rank)),
            _ => None,
        })
        .collect();
    let fired = app
        .world_mut()
        .resource_mut::<Messages<CombatTextEvent>>()
        .drain()
        .any(|e| e.message_type == "HONOR_GAINED");
    (queued, fired)
}

#[test]
fn a_kill_reports_only_for_a_streamed_victim_and_positive_honor() {
    let mut app = app();
    // A ranked Horde victim: the title side is 0.
    assert_eq!(
        credit(&mut app, 14, VICTIM, 5),
        (vec![(VICTIM, 14, Some((5, Some(0))))], true)
    );
    // A signed rank below 0 is the unranked form.
    assert_eq!(
        credit(&mut app, 14, VICTIM, -1),
        (vec![(VICTIM, 14, None)], true)
    );
    // No dishonorable line and no number: honor below 1 returns first.
    assert_eq!(credit(&mut app, 0, VICTIM, 5), (vec![], false));
    assert_eq!(credit(&mut app, -37, VICTIM, 5), (vec![], false));
    // A victim that is not streamed returns before anything.
    assert_eq!(credit(&mut app, 14, 99, 5), (vec![], false));
    // An award (no victim) always reports.
    assert_eq!(credit(&mut app, 42, 0, 0), (vec![(0, 42, None)], true));
}
