//! The raid browser (LFRFrame) from `wrath_raid_browser` receipts: a full `SMSG_UPDATE_LFG_LIST`
//! replaces the browsed list, a difference removes its GUIDs then merges its entries, and the
//! result goes to Native UI's `set_wrath_raid_browser` whole, followed by `UPDATE_LFG_LIST`.
//! Names come from the name cache, class names from the class table, zones from AreaTable.
//! The search verbs (`take_wrath_raid_browser_requests`) send `CMSG_SEARCH_LFG_JOIN`/`LEAVE`.
//! Labelled: a member is never shown ignored or a friend; `unknown_64`/`flag_8c` read 0; the
//! totals are the listed counts.
use crate::net::wrath_social_lists_capture::NativeRaidBrowser;
pub(crate) use crate::net::wrath_social_lists_capture::RaidBrowser;
use crate::net::{ClientCommand, NetCommands};
use bevy::prelude::*;
use std::sync::Arc;
#[cfg(test)]
use wrathplus_protocol::wrath_raid_browser::Group;
use wrathplus_protocol::wrath_raid_browser::{Command, Packet, Player};
use wrathplus_ui::script::{
    UiScript, WrathRaidBrowserGroup, WrathRaidBrowserMember, WrathRaidBrowserRequest,
    WrathRaidBrowserStats, WrathRaidBrowserView,
};

pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        feed_raid_browser
            .in_set(crate::app_core::schedule::UiFeed)
            .run_if(crate::app_core::ui_state::ingame_ui_up),
    );
}

/// Apply one list packet.
fn apply(browser: &mut RaidBrowser, packet: Packet) {
    let Packet::List {
        dungeon,
        difference,
        deleted,
        groups,
        players,
        ..
    } = packet
    else {
        return;
    };
    if !difference || dungeon != browser.dungeon {
        browser.groups.clear();
        browser.players.clear();
    }
    browser.dungeon = dungeon;
    for guid in deleted {
        browser.groups.remove(&guid);
        browser.players.remove(&guid);
    }
    browser
        .groups
        .extend(groups.into_iter().map(|g| (g.guid, g)));
    browser
        .players
        .extend(players.into_iter().map(|p| (p.guid, p)));
    browser.dirty = true;
}

fn member(
    p: &Player,
    name: impl Fn(u64) -> Option<String>,
    area: impl Fn(u32) -> Option<String>,
) -> WrathRaidBrowserMember {
    let c = p.character.as_ref();
    let class = c.and_then(|c| crate::app_core::catalogs::class_names(c.class));
    WrathRaidBrowserMember {
        guid: p.guid,
        name: name(p.guid),
        class_name: class.map(|(n, _)| n.to_string()),
        class_file: class.map(|(_, f)| f.to_string()),
        level: c.map_or(0, |c| c.level),
        area: p.area.and_then(&area),
        comment: p.comment.clone().unwrap_or_default(),
        status: p.status.is_some_and(|s| s != 0),
        roles: p.roles.unwrap_or(0) | u8::from(p.group_leader.is_some()),
        encounter_mask: p.bound.as_ref().map_or(0, |b| b.encounter_mask),
        ignored: false,
        friend: false,
        stats: c.map_or_else(WrathRaidBrowserStats::default, |c| WrathRaidBrowserStats {
            unknown_64: 0,
            talents: c.talents,
            flag_8c: false,
            armor: c.armor,
            spell_damage: c.spell_damage,
            spell_heal: c.spell_heal,
            crit_melee: c.crit_melee,
            crit_ranged: c.crit_ranged,
            crit_spell: c.crit_spell,
            mp5: c.mp5,
            mp5_combat: c.mp5_combat,
            attack_power: c.attack_power,
            agility: c.agility,
            health: c.health,
            mana: c.mana,
            avg_item_level: c.average_item_level,
            defense: c.defense,
            dodge: c.dodge,
            block: c.block,
            parry: c.parry,
            haste: c.haste,
            expertise: c.expertise,
        }),
    }
}

/// The browsed list as the frame reads it: groups with their members (leader first), and the
/// players listed alone.
fn view(
    browser: &RaidBrowser,
    name: impl Fn(u64) -> Option<String> + Copy,
    area: impl Fn(u32) -> Option<String> + Copy,
) -> WrathRaidBrowserView {
    let groups: Vec<_> = browser
        .groups
        .values()
        .map(|g| {
            let mut members: Vec<_> = browser
                .players
                .values()
                .filter(|p| p.group_guid == Some(g.guid))
                .collect();
            members.sort_by_key(|p| p.group_leader.is_none());
            WrathRaidBrowserGroup {
                guid: g.guid,
                comment: g.comment.clone().unwrap_or_default(),
                encounter_mask: g.bound.as_ref().map_or(0, |b| b.encounter_mask),
                members: members.into_iter().map(|p| member(p, name, area)).collect(),
            }
        })
        .collect();
    let players: Vec<_> = browser
        .players
        .values()
        .filter(|p| p.group_guid.is_none())
        .map(|p| member(p, name, area))
        .collect();
    WrathRaidBrowserView {
        dungeon: browser.dungeon,
        total_groups: groups.len() as u32,
        total_players: players.len() as u32,
        groups,
        players,
    }
}

fn feed_raid_browser(
    script: Option<NonSendMut<UiScript>>,
    mut receipts: ResMut<NativeRaidBrowser>,
    mut browser: ResMut<RaidBrowser>,
    names: Res<crate::names::NameCache>,
    areas: Option<Res<crate::area::AreaTableRes>>,
    commands: Res<NetCommands>,
) {
    let Some(mut script) = script else {
        return;
    };
    for request in script.take_wrath_raid_browser_requests() {
        let command = match request {
            WrathRaidBrowserRequest::Join { dungeon } => Command::Join { dungeon },
            WrathRaidBrowserRequest::Leave { dungeon } => Command::Leave { dungeon },
        };
        let _ = commands.0.send(ClientCommand::NativeRaidBrowser(command));
    }
    // The searching flag has no getter of its own: the joined entry is the UI's.
    for packet in receipts.0.drain(..) {
        apply(&mut browser, packet);
    }
    if !browser.dirty && names.generation() == browser.names_seen {
        return;
    }
    browser.dirty = false;
    browser.names_seen = names.generation();
    let name = |guid| names.answered_player_name(guid).map(str::to_string);
    let area = |id| {
        areas
            .as_ref()
            .and_then(|a| a.0.name(id))
            .map(str::to_string)
    };
    let shown = view(&browser, name, area);
    // Ask for the names still missing; their answers republish the list.
    for guid in shown
        .players
        .iter()
        .chain(shown.groups.iter().flat_map(|g| &g.members))
        .filter(|m| m.name.is_none())
        .map(|m| m.guid)
    {
        names.resolve(guid, &commands);
    }
    script.set_wrath_raid_browser(Arc::new(shown));
    script.queue_event("UPDATE_LFG_LIST", Vec::new());
}

#[cfg(test)]
#[path = "wrath_raid_browser_feed_tests.rs"]
mod tests;
