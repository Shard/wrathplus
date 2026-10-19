//! The arena registrar's petition vendor (ArenaRegistrarFrame.lua), an NPC session opened by a
//! `SMSG_PETITION_SHOWLIST` from a petitioner that is not a tabard designer (AC sends it the three
//! arena charters). The feed publishes the rows (`set_wrath_petition_vendor`: each charter's name
//! and icon from the item cache, and its price) and the bag scan behind `HasFilledPetition`, and
//! fires `PETITION_VENDOR_SHOW` on the open, `PETITION_VENDOR_UPDATE` when a row's item lands or
//! the scan flips, and `PETITION_VENDOR_CLOSED` on a host-side close (a walk-away); Lua's
//! `ClosePetitionVendor` fires its own. The drain sends the purchase and the banner turn-in.
//!
//! Labelled (the reference's `0x6dd680` and the event fire sites are untraced): "filled" is an
//! arena charter carried in the bags, and the turn-in takes the bracket's charter
//! (AC `PetitionMgr.h:32-34`); the update fires on those edges.

use benilla_protocol::messages::{PetitionShowList, PetitionShowListEntry};
use benilla_ui::script::{ScriptDialect, UiScript, WrathPetitionItem, WrathPetitionVendorRequest};
use bevy::prelude::*;

use crate::net::{ClientCommand, NetCommands, ObjectStore, Objects, SelfPlayer};
use crate::ui_items::{find_item, ItemSearch};
use crate::ui_script::{UiFeed, UiInput};
use crate::ui_session::NpcSession;

/// AC's arena charters by bracket (`ARENA_TEAM_CHARTER_2v2` ..).
const ARENA_CHARTERS: [(i64, u32); 3] = [(2, 23560), (3, 23561), (5, 23562)];

#[derive(Resource, Default)]
pub(crate) struct ArenaPetitionVendor {
    open: Option<Open>,
    /// Lua's `ClosePetitionVendor` closed it and fired `PETITION_VENDOR_CLOSED` itself.
    closed_by_script: bool,
}

struct Open {
    npc: u64,
    rows: Vec<PetitionShowListEntry>,
}

impl ArenaPetitionVendor {
    pub(super) fn open(&mut self, list: PetitionShowList) {
        self.open = Some(Open {
            npc: list.npc,
            rows: list.entries,
        });
        self.closed_by_script = false;
    }
}

impl NpcSession for ArenaPetitionVendor {
    fn npc(&self) -> Option<u64> {
        self.open.as_ref().map(|o| o.npc)
    }

    fn close(&mut self) {
        self.open = None;
    }
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<ArenaPetitionVendor>().add_systems(
        Update,
        (
            crate::ui_session::close_npc_session_out_of_range::<ArenaPetitionVendor>
                .before(feed_vendor)
                .before(crate::ui_session::feed_interact_npc),
            feed_vendor
                .in_set(UiFeed)
                .run_if(crate::ui_script::ingame_ui_up),
            drain_vendor
                .after(UiInput)
                .after(super::feed::drain_petition),
        ),
    );
}

/// What the feed last published.
#[derive(Default)]
struct Fed {
    npc: Option<u64>,
    rows: Option<Vec<WrathPetitionItem>>,
    filled: bool,
}

/// Whether any arena charter is carried.
fn carries_arena_charter(store: &ObjectStore, objects: &Objects) -> bool {
    ARENA_CHARTERS
        .iter()
        .any(|&(_, entry)| find_item(&store.0, objects, entry, ItemSearch::default()).is_some())
}

#[allow(clippy::too_many_arguments)]
fn feed_vendor(
    script: Option<NonSendMut<UiScript>>,
    mut vendor: ResMut<ArenaPetitionVendor>,
    items: Res<crate::items::Items>,
    icons: Option<Res<crate::entities::ItemDisplays>>,
    commands: Res<NetCommands>,
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    objects: Objects,
    mut fed: Local<crate::ui_script::VmMemo<Fed>>,
) {
    let Some(mut script) = script else {
        return;
    };
    if script.dialect() != ScriptDialect::Wrath12340 {
        return;
    }
    let fed = fed.get(&script);
    let npc = vendor.npc();
    let rows = vendor.open.as_ref().map(|open| {
        open.rows
            .iter()
            .map(|row| {
                let template = items.template(row.charter_entry, 0, &commands);
                WrathPetitionItem {
                    name: template.map(|t| t.name.clone()),
                    texture: template.and_then(|t| {
                        icons
                            .as_deref()
                            .and_then(|i| i.catalog.get(t.display_info_id))
                            .and_then(|d| d.icon.clone())
                    }),
                    cost: row.charter_cost as u32,
                }
            })
            .collect::<Vec<_>>()
    });
    let filled = self_q
        .iter()
        .next()
        .is_some_and(|store| carries_arena_charter(store, &objects));
    if filled != fed.filled {
        fed.filled = filled;
        script.set_wrath_has_filled_petition(filled);
    }
    if npc != fed.npc {
        let closed_by_script = std::mem::take(&mut vendor.closed_by_script);
        if fed.npc.is_some() && !closed_by_script {
            script.set_wrath_petition_vendor(None);
            script.fire_event("PETITION_VENDOR_CLOSED", vec![]);
        }
        fed.npc = npc;
        fed.rows = rows.clone();
        if npc.is_some() {
            script.set_wrath_petition_vendor(rows);
            script.fire_event("PETITION_VENDOR_SHOW", vec![]);
            script.fire_event("PETITION_VENDOR_UPDATE", vec![]);
        }
        return;
    }
    if npc.is_some() && (rows != fed.rows || filled != fed.filled) {
        fed.rows = rows.clone();
        script.set_wrath_petition_vendor(rows);
        script.fire_event("PETITION_VENDOR_UPDATE", vec![]);
    }
}

/// The verbs' sends: the purchase at the open vendor, and the bracket's charter with its banner.
fn drain_vendor(
    script: Option<NonSendMut<UiScript>>,
    mut vendor: ResMut<ArenaPetitionVendor>,
    commands: Res<NetCommands>,
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    objects: Objects,
    mut petition: ResMut<super::PetitionState>,
) {
    let Some(mut script) = script else {
        return;
    };
    for request in script.take_wrath_petition_vendor_requests() {
        match request {
            WrathPetitionVendorRequest::Buy { index, name } => {
                let Some(npc) = vendor.npc() else { continue };
                let _ = commands
                    .0
                    .send(ClientCommand::PetitionBuy { npc, name, index });
            }
            WrathPetitionVendorRequest::NameRefused(key) => {
                petition.lines.push(super::lines::name_refused_line(key));
            }
            WrathPetitionVendorRequest::Close => {
                vendor.close();
                vendor.closed_by_script = true;
            }
            WrathPetitionVendorRequest::TurnIn { team_size, banner } => {
                let charter = ARENA_CHARTERS
                    .iter()
                    .find(|(size, _)| *size == team_size)
                    .and_then(|&(_, entry)| {
                        let store = self_q.iter().next()?;
                        find_item(&store.0, &objects, entry, ItemSearch::default())
                    });
                match charter {
                    Some((_, _, item)) => {
                        let _ = commands.0.send(ClientCommand::TurnInPetition {
                            item,
                            banner: Some(banner),
                        });
                    }
                    None => debug!("ui_petition: no {team_size}v{team_size} charter carried"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(npc: u64) -> PetitionShowList {
        PetitionShowList {
            npc,
            entries: ARENA_CHARTERS
                .iter()
                .enumerate()
                .map(|(i, &(size, entry))| PetitionShowListEntry {
                    index: i as u32 + 1,
                    charter_entry: entry,
                    charter_display_id: 16161,
                    charter_cost: 800_000 * (i as i32 + 1),
                    entry_flags: size as i32,
                })
                .collect(),
        }
    }

    #[test]
    fn a_petitioner_without_the_tabard_flag_opens_the_vendor_not_the_registrar() {
        use crate::target::cursor_mode::npc_flags::{PETITIONER, TABARDDESIGNER};
        let mut registrar = super::super::GuildRegistrarState::default();
        let mut vendor = ArenaPetitionVendor::default();
        super::super::net::show_list(&mut registrar, &mut vendor, list(0x77), Some(PETITIONER));
        assert_eq!((vendor.npc(), registrar.npc()), (Some(0x77), None));
        vendor.close();
        super::super::net::show_list(
            &mut registrar,
            &mut vendor,
            list(0x78),
            Some(PETITIONER | TABARDDESIGNER),
        );
        assert_eq!((vendor.npc(), registrar.npc()), (None, Some(0x78)));
    }

    #[test]
    fn buy_and_turn_in_reach_the_wire_and_a_scripted_close_stays_quiet() {
        let mut app = App::new();
        let (tx, rx) = crossbeam_channel::unbounded();
        app.insert_resource(NetCommands(tx))
            .init_resource::<crate::net::GuidIndex>()
            .init_resource::<ArenaPetitionVendor>()
            .init_resource::<super::super::PetitionState>()
            .add_systems(Update, drain_vendor);
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script.set_wrath_petition_vendor(Some(vec![WrathPetitionItem::default(); 3]));
        script
            .run("BuyPetition(2, 'Gladiators') TurnInArenaPetition(3) ClosePetitionVendor()")
            .unwrap();
        app.world_mut().insert_non_send_resource(script);
        app.world_mut()
            .resource_mut::<ArenaPetitionVendor>()
            .open(list(0x77));
        app.update();
        let sent: Vec<String> = rx.try_iter().map(|c| format!("{c:?}")).collect();
        assert_eq!(
            sent,
            [format!(
                "{:?}",
                ClientCommand::PetitionBuy {
                    npc: 0x77,
                    name: "Gladiators".into(),
                    index: 2
                }
            )],
            "no charter carried, so the turn-in sends nothing"
        );
        let vendor = app.world().resource::<ArenaPetitionVendor>();
        assert!(vendor.npc().is_none() && vendor.closed_by_script);
    }
}
