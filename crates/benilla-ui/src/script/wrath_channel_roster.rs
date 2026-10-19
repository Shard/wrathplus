//! Build-12340 selected channel roster (native 0x504490/0x503990), not a per-channel cache.
//! Display indices are resolved by the UI display list before selecting this state.

/// Native channel requests, emitted in order on a server-channel selection change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WrathChannelRosterRequest {
    Watch { name: String },
    DisplayList { name: String },
}

/// A received member. `None` is an outstanding name query, not an empty display name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrathChannelRosterMember {
    pub guid: u64,
    pub flags: u8,
    pub name: Option<String>,
}

/// One `SMSG_USERLIST_ADD`/`_UPDATE`/`_REMOVE` (`0x3F0`/`0x3F2`/`0x3F1`), which the server sends
/// a channel's watchers after `SetChannelWatch` (AC `Channel.cpp`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WrathChannelRosterDelta {
    Add { guid: u64, flags: u8 },
    Update { guid: u64, flags: u8 },
    Remove { guid: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Identity {
    Server(String),
    Synthetic(u8),
}

#[derive(Default)]
pub(crate) struct WrathChannelRoster {
    pub selected: Option<Identity>,
    pub members: Vec<WrathChannelRosterMember>,
    pub flags: u8,
    pub requests: Vec<WrathChannelRosterRequest>,
    revision: u64,
    received: bool,
}

impl WrathChannelRoster {
    fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.members.clear();
        self.flags = 0;
        self.received = false;
    }

    pub fn select(&mut self, identity: Identity) -> bool {
        let same = match (&self.selected, &identity) {
            (Some(Identity::Server(a)), Identity::Server(b)) => a.eq_ignore_ascii_case(b),
            (Some(a), b) => a == b,
            _ => false,
        };
        if same {
            return false;
        }
        self.invalidate();
        if let Identity::Server(name) = &identity {
            self.requests
                .push(WrathChannelRosterRequest::Watch { name: name.clone() });
            self.requests
                .push(WrathChannelRosterRequest::DisplayList { name: name.clone() });
        }
        self.selected = Some(identity);
        true
    }

    pub fn clear_selection(&mut self) {
        self.invalidate();
        self.selected = None;
    }

    pub fn set_synthetic(&mut self, members: Vec<WrathChannelRosterMember>) {
        self.members = members;
        self.received = true;
        self.sort_ready();
    }

    pub fn ready(&self) -> bool {
        self.received && self.members.iter().all(|row| row.name.is_some())
    }

    fn sort_ready(&mut self) {
        if self.ready() {
            // Native 0x4fb0f0 compares resolved names, not rank, GUID or arrival order.
            self.members
                .sort_by_cached_key(|row| row.name.as_deref().unwrap_or("").to_lowercase());
        }
    }

    pub fn receive(
        &mut self,
        channel_type: u8,
        channel: &str,
        flags: u8,
        members: Vec<WrathChannelRosterMember>,
    ) -> Option<u64> {
        if channel_type == 0
            || !matches!(&self.selected,
            Some(Identity::Server(name)) if name.eq_ignore_ascii_case(channel))
        {
            return None;
        }
        self.invalidate();
        self.members = members;
        self.flags = flags;
        self.received = true;
        self.sort_ready();
        Some(self.revision)
    }

    /// Patch the received roster of the selected server channel. Labelled: the native delta
    /// handlers are unread; an add of a present member updates it, and the re-sort follows the
    /// full roster's.
    pub fn apply(
        &mut self,
        channel: &str,
        delta: WrathChannelRosterDelta,
        channel_flags: u8,
    ) -> Option<u64> {
        if !self.received
            || !matches!(&self.selected, Some(Identity::Server(name)) if name.eq_ignore_ascii_case(channel))
        {
            return None;
        }
        match delta {
            WrathChannelRosterDelta::Add { guid, flags } | WrathChannelRosterDelta::Update { guid, flags } => {
                match self.members.iter_mut().find(|r| r.guid == guid) {
                    Some(row) => row.flags = flags,
                    None if matches!(delta, WrathChannelRosterDelta::Add { .. }) => {
                        self.members.push(WrathChannelRosterMember { guid, flags, name: None })
                    }
                    None => return None,
                }
            }
            WrathChannelRosterDelta::Remove { guid } => self.members.retain(|r| r.guid != guid),
        }
        self.flags = channel_flags;
        self.sort_ready();
        Some(self.revision)
    }

    pub fn resolve(&mut self, revision: u64, guid: u64, name: String) -> bool {
        if !self.received || revision != self.revision {
            return false;
        }
        let mut changed = false;
        for row in &mut self.members {
            if row.guid == guid && row.name.is_none() {
                row.name = Some(name.clone());
                changed = true;
            }
        }
        self.sort_ready();
        changed
    }
}

impl super::UiScript {
    pub fn take_wrath_channel_roster_requests(&mut self) -> Vec<WrathChannelRosterRequest> {
        std::mem::take(&mut self.model_mut().wrath_channel_roster.requests)
    }

    /// Invalidate pending callbacks and selected rows at session/world teardown.
    pub fn reset_wrath_channel_roster(&mut self) {
        let mut model = self.model_mut();
        let state = &mut model.wrath_channel_roster;
        state.invalidate();
        state.selected = None;
        state.requests.clear();
    }
}

fn notify_ready(model: &mut super::Model) {
    if !model.wrath_channel_roster.ready() {
        return;
    }
    if let Some(index) = super::wrath_channel_display::selected_index(model) {
        let count = model.wrath_channel_roster.members.len();
        for event in ["CHANNEL_COUNT_UPDATE", "CHANNEL_ROSTER_UPDATE"] {
            model.pending_events.push((
                event.into(),
                vec![
                    super::ScriptValue::Number(index as f64),
                    super::ScriptValue::Number(count as f64),
                ],
            ));
        }
    }
}

impl super::UiScript {
    /// Consume a newly received display response once. Ordinary lists and stale channels
    /// return None; the revision binds later name completions to this accepted response.
    pub fn receive_wrath_channel_roster(
        &mut self,
        channel_type: u8,
        channel: &str,
        flags: u8,
        members: Vec<WrathChannelRosterMember>,
    ) -> Option<u64> {
        let mut model = self.model_mut();
        let revision = model
            .wrath_channel_roster
            .receive(channel_type, channel, flags, members)?;
        notify_ready(&mut model);
        Some(revision)
    }

    /// Apply one roster delta to the selected server channel, then fire the count and roster
    /// updates as a received roster does; the revision binds the added member's name completion.
    /// `count`, the server's member count, is the roster's own length here.
    pub fn apply_wrath_channel_roster_delta(
        &mut self,
        channel: &str,
        delta: WrathChannelRosterDelta,
        channel_flags: u8,
        _count: u32,
    ) -> Option<u64> {
        let mut model = self.model_mut();
        let revision = model
            .wrath_channel_roster
            .apply(channel, delta, channel_flags)?;
        notify_ready(&mut model);
        Some(revision)
    }

    pub fn resolve_wrath_channel_roster_name(
        &mut self,
        revision: u64,
        guid: u64,
        name: String,
    ) -> bool {
        let mut model = self.model_mut();
        let changed = model.wrath_channel_roster.resolve(revision, guid, name);
        if changed {
            notify_ready(&mut model);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(guid: u64, name: Option<&str>, flags: u8) -> WrathChannelRosterMember {
        WrathChannelRosterMember {
            guid,
            name: name.map(str::to_owned),
            flags,
        }
    }

    #[test]
    fn selection_requests_once_and_does_not_replay_old_rosters() {
        let mut s = WrathChannelRoster::default();
        assert!(s.select(Identity::Server("World".into())));
        assert_eq!(
            s.requests,
            vec![
                WrathChannelRosterRequest::Watch {
                    name: "World".into()
                },
                WrathChannelRosterRequest::DisplayList {
                    name: "World".into()
                },
            ]
        );
        assert!(!s.select(Identity::Server("WORLD".into())));
        assert_eq!(s.requests.len(), 2);
        assert!(s.receive(0, "World", 0, vec![]).is_none());
        assert!(s.receive(1, "Trade", 0, vec![]).is_none());
        s.receive(2, "World", 0x91, vec![member(1, Some("Zed"), 0xe9)])
            .unwrap();
        assert!(s.ready());
        assert_eq!(s.members[0].flags, 0xe9);
        assert_eq!(s.flags, 0x91);
        s.select(Identity::Server("Trade".into()));
        assert!(!s.ready());
        assert!(s.members.is_empty());
        s.select(Identity::Server("World".into()));
        assert!(!s.ready());
        assert!(s.members.is_empty());
    }

    #[test]
    fn all_names_gate_sorting_and_revisions_reject_stale_callbacks() {
        let mut s = WrathChannelRoster::default();
        s.select(Identity::Server("World".into()));
        let rev = s
            .receive(
                1,
                "World",
                0,
                vec![
                    member(1, Some("Zed"), 1),
                    member(2, None, 2),
                    member(3, None, 8),
                ],
            )
            .unwrap();
        assert!(!s.ready());
        assert!(s.resolve(rev, 2, "alpha".into()));
        assert!(!s.ready());
        assert!(s.resolve(rev, 3, "Beta".into()));
        assert!(s.ready());
        assert_eq!(
            s.members.iter().map(|r| r.guid).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
        let next = s.receive(1, "World", 0, vec![member(2, None, 0)]).unwrap();
        assert!(!s.resolve(rev, 2, "Old".into()));
        assert!(!s.ready());
        assert!(s.resolve(next, 2, String::new()));
        assert!(s.ready());
        s.select(Identity::Synthetic(2));
        assert!(!s.resolve(next, 2, "Stale".into()));
    }
}
