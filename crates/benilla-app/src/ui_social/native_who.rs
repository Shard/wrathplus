//! Build-12340 base WHO results. Native 006b8720 stores the first 50 rows,
//! keeps the second wire count independently, and routes by the uncapped count.
//! BG/arena cross-realm local augmentation (006b8290) is not implemented here.
use super::*;
use crate::net::wrath_who_capture::NativeWhoPackets;
use crate::net::ClientCommand;
use benilla_protocol::{
    types::{WhoEntry, WhoResults},
    wrath_who::Packet,
};

pub(crate) fn drain(packets: Option<ResMut<NativeWhoPackets>>, mut social: ResMut<SocialState>) {
    let Some(mut packets) = packets else {
        return;
    };
    // WHOIS remains private capture-only; consume WHO replies in FIFO order.
    let mut private = std::collections::VecDeque::new();
    for packet in packets.0.drain(..) {
        match packet {
            Packet::Who { total, entries } => {
                social.pending_native_who.push_back((total, entries));
            }
            packet @ Packet::Whois { .. } => private.push_back(packet),
        }
    }
    packets.0 = private;
}

pub(super) fn apply(
    social: &mut SocialState,
    total: u32,
    entries: Vec<benilla_protocol::wrath_who::Entry>,
) {
    let raw_count = entries.len();
    let rows = entries
        .iter()
        .take(50)
        .map(|e| WhoEntry {
            name: String::from_utf8_lossy(&e.name).into_owned(),
            guild: String::from_utf8_lossy(&e.guild).into_owned(),
            level: e.level,
            class: e.class,
            race: e.race,
            zone: e.zone,
        })
        .collect::<Vec<_>>();
    social.apply_who(WhoResults {
        displayed: rows.len() as u32,
        total,
        entries: rows,
    });
    social.native_who_count = Some(raw_count);
    social.native_who_rows = entries;
}

pub(super) fn command(
    filter: &str,
    areas: Option<&benilla_formats::AreaTableCatalog>,
) -> ClientCommand {
    let q = super::query::parse(filter, areas);
    ClientCommand::NativeWhoCommand(benilla_protocol::wrath_who::Command::Who(
        benilla_protocol::wrath_who::Query {
            level_min: q.level_min,
            level_max: q.level_max,
            player_name: q.player_name.into_bytes(),
            guild_name: q.guild_name.into_bytes(),
            race_mask: q.race_mask,
            class_mask: q.class_mask,
            zones: q.zones,
            search_terms: q.search_terms.into_iter().map(String::into_bytes).collect(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    #[test]
    fn caps_display_preserves_total_raw_count_gender_and_private_capture() {
        let mut app = App::new();
        app.init_resource::<SocialState>()
            .init_resource::<NativeWhoPackets>();
        let rows = (0..51)
            .map(|i| benilla_protocol::wrath_who::Entry {
                name: format!("Synthetic{i}").into_bytes(),
                guild: vec![],
                level: 80,
                class: 6,
                race: 10,
                gender: 255,
                zone: 12,
            })
            .collect();
        app.world_mut()
            .resource_mut::<NativeWhoPackets>()
            .0
            .push_back(Packet::Who {
                total: 999,
                entries: rows,
            });
        app.world_mut()
            .resource_mut::<NativeWhoPackets>()
            .0
            .push_back(Packet::Whois {
                message: b"synthetic-private".to_vec(),
            });
        app.world_mut().run_system_once(drain).unwrap();
        let (total, entries) = app
            .world_mut()
            .resource_mut::<SocialState>()
            .pending_native_who
            .pop_front()
            .unwrap();
        apply(
            &mut app.world_mut().resource_mut::<SocialState>(),
            total,
            entries,
        );
        let social = app.world().resource::<SocialState>();
        assert_eq!(social.who.len(), 50);
        assert_eq!(social.who_total, 999);
        assert_eq!(social.native_who_count, Some(51));
        assert_eq!(social.native_who_rows[0].gender, 255);
        assert_eq!(app.world().resource::<NativeWhoPackets>().0.len(), 1);
    }
    #[test]
    fn native_producer_adds_wrath_classes_and_races() {
        let ClientCommand::NativeWhoCommand(benilla_protocol::wrath_who::Command::Who(q)) =
            command("c-deathknight r-bloodelf", None)
        else {
            panic!()
        };
        assert_eq!(q.class_mask, 1 << 6);
        assert_eq!(q.race_mask, 1 << 10);
    }
}
