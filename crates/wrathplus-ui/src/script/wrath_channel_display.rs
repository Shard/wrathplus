//! Native display-list indices: group, world and custom headers occupy real rows.
//! Native builder 0x504840 iterates confirmed channel slots, not alphabetic names.

use super::{wrath_channel_roster::Identity, Model};

/// Native joined-channel metadata, including an inactive custom channel retained in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrathDisplayChannel {
    pub slot: u32,
    pub name: String,
    pub channel_id: u32,
    pub active: bool,
    /// Native custom-channel +0xa0 state: 1 hides the row; 0 is active.
    pub custom_state: u32,
}

#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub identity: Option<Identity>,
    pub header: Option<usize>,
    pub name: String,
    pub slot: Option<u32>,
    pub active: bool,
    pub count: usize,
}

#[derive(Default)]
pub(crate) struct Display {
    pub channels: Vec<WrathDisplayChannel>,
    pub collapsed: [bool; 3],
}

pub(super) fn entries(model: &Model) -> Vec<Entry> {
    let mut groups = Vec::new();
    let party = &model.party;
    // Native 0x572f50 tests Map.dbc InstanceType 3 or 4 (battleground/arena).
    let battleground = matches!(model.instance_type, Some(3 | 4)) && !party.raid.is_empty();
    if !party.raid.is_empty() && !battleground {
        groups.push((3, "CHAT_MSG_RAID", party.raid.len()));
    } else if party.leader_guid != 0 {
        groups.push((2, "CHAT_MSG_PARTY", party.members.len() + 1));
    }
    if battleground {
        groups.push((1, "CHAT_MSG_BATTLEGROUND", party.raid.len()));
    }
    let mut result = Vec::new();
    for category in 0..3 {
        let channels: Vec<_> = model
            .wrath_channel_display
            .channels
            .iter()
            .filter(|row| (row.channel_id != 0) == (category == 1))
            .filter(|_| category != 0)
            .collect();
        let count = if category == 0 {
            groups.len()
        } else {
            channels
                .iter()
                .filter(|row| category == 1 || row.custom_state != 1)
                .count()
        };
        result.push(Entry {
            identity: None,
            header: Some(category),
            name: [
                "CHANNEL_CATEGORY_GROUP",
                "CHANNEL_CATEGORY_WORLD",
                "CHANNEL_CATEGORY_CUSTOM",
            ][category]
                .into(),
            slot: None,
            active: false,
            count: if model.wrath_channel_display.collapsed[category] || category == 0 {
                count
            } else {
                0
            },
        });
        if model.wrath_channel_display.collapsed[category] {
            continue;
        }
        if category == 0 {
            for &(kind, name, count) in &groups {
                result.push(Entry {
                    identity: Some(Identity::Synthetic(kind)),
                    header: None,
                    name: name.into(),
                    slot: None,
                    active: true,
                    count,
                });
            }
        } else {
            for row in channels {
                if category == 2 && row.custom_state == 1 {
                    continue;
                }
                result.push(Entry {
                    identity: Some(Identity::Server(row.name.clone())),
                    header: None,
                    name: row.name.clone(),
                    slot: Some(row.slot),
                    active: if category == 2 {
                        row.custom_state == 0
                    } else {
                        row.active
                    },
                    count: if model.wrath_channel_roster.ready()
                        && matches!(&model.wrath_channel_roster.selected, Some(Identity::Server(name)) if name.eq_ignore_ascii_case(&row.name)) {
                        model.wrath_channel_roster.members.len()
                    } else { 0 },
                });
            }
        }
    }
    result
}

pub(super) fn selected_index(model: &Model) -> Option<usize> {
    let selected = model.wrath_channel_roster.selected.as_ref()?;
    entries(model)
        .iter()
        .position(|row| match (&row.identity, selected) {
            (Some(Identity::Server(a)), Identity::Server(b)) => a.eq_ignore_ascii_case(b),
            (Some(a), b) => a == b,
            _ => false,
        })
        .map(|i| i + 1)
}

impl super::UiScript {
    pub fn set_wrath_display_channels(&mut self, mut channels: Vec<WrathDisplayChannel>) {
        channels.sort_by_key(|row| row.slot);
        let mut model = self.model_mut();
        let left = match &model.wrath_channel_roster.selected {
            Some(Identity::Server(name)) => !channels.iter().any(|row| {
                let selectable = if row.channel_id == 0 {
                    row.custom_state == 0
                } else {
                    row.active
                };
                selectable && row.name.eq_ignore_ascii_case(name)
            }),
            _ => false,
        };
        let changed = model.wrath_channel_display.channels != channels;
        model.wrath_channel_display.channels = channels;
        if changed {
            model
                .pending_events
                .push(("CHANNEL_UI_UPDATE".into(), vec![]));
        }
        if left {
            model.wrath_channel_roster.clear_selection();
        }
    }
}
