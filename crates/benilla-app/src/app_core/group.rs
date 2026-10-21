//! The party and raid state (roster, loot rules, the Wrath raid extension, saved instances and the
//! party pets), read by world and net code; `ui_party` keeps the systems and re-exports these from
//! their old paths.

use std::collections::HashMap;

use super::errors::UiError;
use super::state::GROUPTYPE_RAID;

use benilla_protocol::types::{
    party_operation, party_result, patch_auras, GroupLootInfo, GroupMemberEntry, ObjectFields,
    PartyMemberStatsInfo,
};
use benilla_protocol::wrath_group::Stats;
use benilla_protocol::wrath_instance::RaidInstanceInfo;
use bevy::prelude::*;

/// Native response fields that the shared presentation does not represent.
#[derive(Default)]
pub struct RaidState {
    pub command_result: Option<(u32, String, u32, u32)>,
    pub target_setter: Option<u64>,
    pub ready_initiator: Option<u64>,
    pub ready_answers: Vec<(u64, u8)>,
    pub ready_finished: u32,
    // Session-scoped ordered edges, like the shared ready_check_answers log.
    // Retained across group leave so same-frame leave/rejoin cannot reuse a cursor;
    // clear_session releases the history. Do not truncate without a sequence cursor.
    pub ready_events: Vec<benilla_protocol::wrath_group::raid::Packet>,
}

/// Native saved-instance receipt ownership. Receiver 0x501030 installs every list before
/// UPDATE_INSTANCE_INFO, including duplicates and empty answers. UI projection is a later seam;
/// keep ordered receipts as well as the latest snapshot so a feed cannot coalesce packet edges.
#[derive(Default)]
pub(crate) struct NativeSavedInstances {
    pub(crate) snapshot: Option<RaidInstanceInfo>,
    pub(crate) receipts: Vec<RaidInstanceInfo>,
}

impl NativeSavedInstances {
    pub(crate) fn receive(&mut self, info: RaidInstanceInfo) {
        self.snapshot = Some(info.clone());
        self.receipts.push(info);
    }
}

/// Which roster a pet token counts its owner in: the two resolve the pet's guid apart
/// (`0x4e81d0` behind the owner's online bit, `0x491960` with none).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Roster {
    Party,
    Raid,
}

/// The group session mirror: `SMSG_GROUP_LIST` as sent plus its side state, reset on disconnect.
#[derive(Resource, Default)]
pub struct GroupState {
    /// Authoritative native Wrath state, separate from the shared roster presentation.
    pub wrath_roster: Option<benilla_protocol::wrath_group::Roster>,
    pub wrath_stats: HashMap<u64, benilla_protocol::wrath_group::Stats>,
    pub wrath_raid: RaidState,
    /// True from any `SMSG_GROUP_LIST` naming a leader until the all-zero "you left" list.
    pub in_group: bool,
    /// 0 party, 1 raid (vmangos `GroupType`, `Group/Group.h:116-120`).
    pub group_type: u8,
    /// Our subgroup (bits 0-2) and raid-assistant bit (`0x80`).
    pub own_flags: u8,
    /// The other members in wire order; the list never contains the recipient.
    pub members: Vec<GroupMemberEntry>,
    /// How many `SMSG_GROUP_LIST`s landed, the all-zero and an unchanged one included. The
    /// reference signals `PARTY_MEMBERS_CHANGED` after each (`0x5e6c61`), with no compare against
    /// the roster it holds, and an empty list reaches it too (`0x5e6b3c`).
    pub lists_applied: u32,
    /// How many `RAID_ROSTER_UPDATE`s the lists signalled, which the feed answers one for one. A raid
    /// list signals after its roster rebuild once no member name is pending (`0x4babef`), with no
    /// compare against the roster it replaces; a list that is not a raid signals once for the roster
    /// it drops (`0x4ba57b`). A raid list with a name pending signals nothing then, and the name
    /// answer signals later (`0x4bada6`).
    pub roster_updates: u32,
    /// The leave ack, `SMSG_GROUP_UNINVITE` and `SMSG_GROUP_DESTROYED` empty the reference's party
    /// slots and leader (`0x4e84a0`, `0x4e8250(0)`) and keep its raid roster; the next list then
    /// finds no slot held.
    pub slots_emptied: bool,
    pub leader: u64,
    /// The loot tail, present whenever the list has members.
    pub loot: Option<GroupLootInfo>,
    /// The inviter's name while the invite popup is up.
    pub pending_invite: Option<String>,
    /// Each member's record by guid, seated at 1/1, patched by `SMSG_PARTY_MEMBER_STATS`, and given
    /// its online bit again by each `SMSG_GROUP_LIST` that lists the member.
    pub stats: HashMap<u64, PartyMemberStatsInfo>,
    /// The marked guid per raid-target icon 0-7 (star to skull), 0 when unset.
    pub raid_targets: [u64; 8],
    /// `/partytest` sandbox: the drain applies group intents to this mirror; a real list clears it.
    pub test: bool,
    /// Our raid lockouts, the character's rather than the group's (leaving the group keeps them):
    /// the latest list and the ordered receipts, each one `UPDATE_INSTANCE_INFO` (`0x501030`).
    pub(crate) wrath_saved_instances: NativeSavedInstances,
    /// Bumped by each ready check that pops for us; a count, so a second one still fires.
    pub ready_check: u32,
    /// Bumped by every ready-check open, ours as leader included, where [`Self::ready_check`] skips
    /// the leader's (`0x4ba360`'s leader arm never reaches the `READY_CHECK` fire at `0x4ba53a`).
    pub ready_check_requests: u32,
    /// The answers forwarded to us as leader, append-only until the next request clears them.
    pub ready_check_answers: Vec<(u64, bool)>,
}

impl GroupState {
    /// Apply one `SMSG_GROUP_LIST` and return what it shows, in the reference's order: the party
    /// slots' joins, the new-group line, the party slots' leaves, then the raid roster's lines.
    /// Kick, leave (`0x5e690b`) and disband lines come from their opcodes. Every name is taken as
    /// held: [`Self::apply_list_awaiting`] is the one that says otherwise.
    pub(crate) fn apply_list(
        &mut self,
        group_type: u8,
        own_flags: u8,
        members: Vec<GroupMemberEntry>,
        leader: u64,
        loot: Option<GroupLootInfo>,
        self_guid: Option<u64>,
    ) -> ListOutcome {
        self.apply_list_awaiting(
            group_type, own_flags, members, leader, loot, self_guid, false,
        )
    }

    /// [`Self::apply_list`] told what the name cache holds: `names_pending` when a row of a raid
    /// list has no cached name yet, the reference's pending count (`[0xb713f0]`, raised at `0x4ba9d0`
    /// for each row whose name lookup misses), which holds `RAID_ROSTER_UPDATE` back (`0x4babdf`).
    pub(crate) fn apply_list_awaiting(
        &mut self,
        group_type: u8,
        own_flags: u8,
        members: Vec<GroupMemberEntry>,
        leader: u64,
        loot: Option<GroupLootInfo>,
        self_guid: Option<u64>,
        names_pending: bool,
    ) -> ListOutcome {
        // Every path of the handler reaches the event, the leave list and a party's list alike.
        self.count_list();
        let mut out = ListOutcome::default();
        let raid = group_type == GROUPTYPE_RAID;
        // The two stores as this list finds them: the party slots unless an opcode emptied them,
        // and the raid roster, which only a list that is not a raid drops.
        let held_slots: Vec<(u64, String)> = if self.slots_emptied {
            Vec::new()
        } else {
            self.party_slots()
                .map(|m| (m.guid, m.name.clone()))
                .collect()
        };
        let raid_held = self.group_type == GROUPTYPE_RAID;
        // "Held a group" (`0x5e6aa9`): an occupied slot (`0x5e6afb`) or a raid roster (`0x5e6b29`).
        let held = !held_slots.is_empty() || raid_held;

        let own = own_flags & 0x7f;
        let seated: Vec<&GroupMemberEntry> = members
            .iter()
            .filter(|m| m.flags & 0x7f == own)
            .take(4)
            .collect();
        for m in &seated {
            if !held {
                // Seated into no group: no line, but the chime (`0x5e6dc6`).
                out.invite_accept = true;
            } else if !raid && !held_slots.iter().any(|(guid, _)| *guid == m.guid) {
                // New to a party we held (`0x5e6c19`, `0x5e6c24`).
                out.lines.push(UiError::s("ERR_JOINED_GROUP_S", &m.name));
                out.invite_accept = true;
            }
        }
        // A new group we lead: one line, for the last member listed (`0x5e6c94`-`0x5e6cc1`).
        if !held && self_guid.is_some_and(|me| me == leader) {
            if let Some(last) = members.last().filter(|m| !m.name.is_empty()) {
                out.lines.push(UiError::s("ERR_JOINED_GROUP_S", &last.name));
            }
        }
        // A party list's leaves: the held slots it no longer seats (`0x5e6cd7`-`0x5e6e09`).
        if !raid {
            for (guid, name) in &held_slots {
                if !seated.iter().any(|m| m.guid == *guid) {
                    out.lines.push(UiError::s("ERR_LEFT_GROUP_S", name));
                }
            }
        }
        if raid {
            // `0x4ba5f0`: arrivals only onto a held roster (`0x4ba725`), departures, and the
            // joined line when none was held (`0x4ba83a`).
            if raid_held {
                for m in &members {
                    if !self.members.iter().any(|old| old.guid == m.guid) {
                        out.lines
                            .push(UiError::s("ERR_RAID_MEMBER_ADDED_S", &m.name));
                    }
                }
                for old in &self.members {
                    if !members.iter().any(|m| m.guid == old.guid) {
                        out.lines
                            .push(UiError::s("ERR_RAID_MEMBER_REMOVED_S", &old.name));
                    }
                }
            } else {
                out.lines.push(UiError::key("ERR_RAID_YOU_JOINED"));
            }
            // The rebuild ends in the signal unless a name is pending (`0x4babdf`-`0x4babef`).
            if !names_pending {
                self.count_roster_update();
            }
        } else if raid_held {
            // `0x4ba550`: a list that is not a raid drops the held roster (`0x4ba55f`), and says so
            // (`0x4ba57b`). With no roster held it signals no `RAID_ROSTER_UPDATE`.
            out.lines.push(UiError::key("ERR_RAID_YOU_LEFT"));
            self.count_roster_update();
        }

        // The all-zero list means "not in a group" (vmangos `Server/Packets/Group.h:257`); only
        // `leader == 0` tells it apart, since a solo leader's list is empty but names a leader.
        if leader == 0 {
            self.leave_group();
            return out;
        }
        // Raid to party clears the raid-target board, as the reference's `0x4ba550` does on the
        // raid-flag-clear leg; a disband clears it in `leave_group`.
        if group_type != 1 && self.group_type == 1 {
            self.raid_targets = [0; 8];
        }

        self.stats
            .retain(|guid, _| members.iter().any(|m| m.guid == *guid));
        // Each listed member's record takes its online bit from the row's status byte and keeps the
        // rest: a known member's record is passed back to `0x4e82d0` (`0x5e6ddb`-`0x5e6de8`), which
        // writes the bit last (`0x4e8361`-`0x4e837b`), and the raid roster's twin does the same to
        // its row's record (`0x4ba947`-`0x4ba960`). A member new to the roster has no record yet;
        // `net::list` seats its placeholder.
        for m in &members {
            if let Some(record) = self.stats.get_mut(&m.guid) {
                record.set_online(m.listed_online());
            }
        }

        self.in_group = true;
        self.group_type = group_type;
        self.own_flags = own_flags;
        self.members = members;
        self.slots_emptied = false;
        self.leader = leader;
        self.loot = loot;
        // A real list ends the sandbox; `synthetic_roster` re-raises the flag after its own call.
        self.test = false;
        out
    }

    /// Count one `SMSG_GROUP_LIST`, which the feed answers with one `PARTY_MEMBERS_CHANGED`. The
    /// `/partytest` sandbox calls it for each group intent whose echo the server would send as a
    /// list.
    pub(crate) fn count_list(&mut self) {
        self.lists_applied = self.lists_applied.wrapping_add(1);
    }

    /// Count one `RAID_ROSTER_UPDATE` a list signals, which the feed answers with one.
    pub(crate) fn count_roster_update(&mut self) {
        self.roster_updates = self.roster_updates.wrapping_add(1);
    }

    /// The `party1..party4` slots: our own subgroup in packet order, at most four (`0x5e6baa`
    /// compares the flags under `0x7f`). A slot's member can change on any resync.
    pub(crate) fn party_slots(&self) -> impl Iterator<Item = &GroupMemberEntry> {
        let own = self.own_flags & 0x7f;
        self.members
            .iter()
            .filter(move |m| m.flags & 0x7f == own)
            .take(4)
    }

    /// `SMSG_GROUP_INVITE` we may accept: the popup plus a chat line. Build 12340's handler
    /// (`0x6df1a0`) opens the popup (`0x512540`) and raises message `0x41`,
    /// `ERR_INVITED_TO_GROUP_SS`, whose player link takes the name twice.
    pub(crate) fn apply_invited(&mut self, inviter: &str) -> Vec<UiError> {
        self.pending_invite = Some(inviter.to_string());
        vec![UiError::strings(
            "ERR_INVITED_TO_GROUP_SS",
            &[inviter, inviter],
        )]
    }

    /// `SMSG_GROUP_INVITE` with its accept byte 0, because we are already grouped: no popup, and
    /// message `0x42`, `ERR_INVITED_ALREADY_IN_GROUP_SS` (`0x6df1a0`).
    pub(crate) fn apply_invited_while_grouped(&self, inviter: &str) -> Vec<UiError> {
        vec![UiError::strings(
            "ERR_INVITED_ALREADY_IN_GROUP_SS",
            &[inviter, inviter],
        )]
    }

    /// `SMSG_GROUP_DECLINE`: our invitee said no; only the inviter gets it.
    pub(crate) fn apply_declined(&mut self, name: &str) -> Vec<UiError> {
        vec![UiError::s("ERR_DECLINE_GROUP_S", name)]
    }

    /// `SMSG_GROUP_UNINVITE`: we were kicked; empties the slots and prints unconditionally
    /// (`0x5e6850`).
    pub(crate) fn apply_uninvited(&mut self) -> Vec<UiError> {
        self.slots_emptied = true;
        vec![UiError::key("ERR_UNINVITE_YOU")]
    }

    /// `SMSG_GROUP_DESTROYED`: prints only while grouped (`0x5e6880` tests `0x4e86d0() != 0`), then
    /// empties the slots (`0x5e6893`). vmangos's two-member collapse sends none
    /// (`Group/Group.cpp:533`).
    pub(crate) fn apply_destroyed(&mut self) -> Vec<UiError> {
        let lines = if self.in_group {
            vec![UiError::key("ERR_GROUP_DISBANDED")]
        } else {
            Vec::new()
        };
        self.slots_emptied = true;
        lines
    }

    /// `SMSG_GROUP_SET_LEADER`; vmangos sends the full list right after (`Group::ChangeLeader`).
    pub(crate) fn apply_leader_changed(
        &mut self,
        name: &str,
        own_name: Option<&str>,
    ) -> Vec<UiError> {
        if own_name == Some(name) {
            vec![UiError::key("ERR_NEW_LEADER_YOU")]
        } else {
            vec![UiError::s("ERR_NEW_LEADER_S", name)]
        }
    }

    /// `SMSG_PARTY_COMMAND_RESULT` through the reference's jump table (`0x5e6a14`): one message
    /// per result, its catalog row giving text, surface and sound (result 7 is the red error line,
    /// the rest chat). Silent, as in the reference: results past 8, `OK` for an operation other
    /// than invite or leave, and an invite ack with no name (`0x5e6923`). The leave ack also
    /// empties the slots (`0x5e68f6`).
    pub(crate) fn apply_command_result(
        &mut self,
        operation: u32,
        member: &str,
        result: u32,
    ) -> Option<UiError> {
        let named = |key: &'static str| UiError::s(key, member);
        match result {
            party_result::OK => match operation {
                // The reference's guard: an invite ack with no name prints nothing.
                party_operation::INVITE if !member.is_empty() => Some(named("ERR_INVITE_PLAYER_S")),
                party_operation::LEAVE => {
                    self.slots_emptied = true;
                    Some(UiError::key("ERR_LEFT_GROUP_YOU"))
                }
                _ => None,
            },
            party_result::BAD_PLAYER_NAME => Some(named("ERR_BAD_PLAYER_NAME_S")),
            party_result::TARGET_NOT_IN_GROUP => Some(named("ERR_TARGET_NOT_IN_GROUP_S")),
            party_result::GROUP_FULL => Some(UiError::key("ERR_GROUP_FULL")),
            party_result::ALREADY_IN_GROUP => Some(named("ERR_ALREADY_IN_GROUP_S")),
            party_result::NOT_IN_GROUP => Some(UiError::key("ERR_NOT_IN_GROUP")),
            party_result::NOT_LEADER => Some(UiError::key("ERR_NOT_LEADER")),
            party_result::WRONG_FACTION => Some(UiError::key("ERR_PLAYER_WRONG_FACTION")),
            party_result::IGNORING_YOU => Some(named("ERR_IGNORING_YOU_S")),
            _ => None,
        }
    }

    /// `SMSG_PARTY_MEMBER_STATS`: a delta patches the record field by field (`0x5e519d`, seeded from
    /// the stored record), `_FULL` patches an emptied one (`0x5e51b1`). An aura block is patched
    /// slot by slot ([`patch_auras`]).
    pub(crate) fn apply_stats(&mut self, guid: u64, full: bool, info: PartyMemberStatsInfo) {
        if full {
            self.stats.insert(guid, PartyMemberStatsInfo::default());
        }
        let entry = self.stats.entry(guid).or_default();
        macro_rules! merge {
            ($($field:ident),* $(,)?) => {
                $(if info.$field.is_some() { entry.$field = info.$field; })*
            };
        }
        merge!(
            status,
            cur_hp,
            max_hp,
            power_type,
            cur_power,
            max_power,
            level,
            zone,
            position,
            pet_guid,
            pet_model_id,
            pet_cur_hp,
            pet_max_hp,
            pet_power_type,
            pet_cur_power,
            pet_max_power,
        );
        if info.pet_name.is_some() {
            entry.pet_name = info.pet_name;
        }
        for (record, patch) in [
            (&mut entry.auras, info.auras),
            (&mut entry.auras_negative, info.auras_negative),
            (&mut entry.pet_auras, info.pet_auras),
            (&mut entry.pet_auras_negative, info.pet_auras_negative),
        ] {
            if let Some(patch) = patch {
                patch_auras(record, &patch);
            }
        }
    }

    /// `MSG_RAID_TARGET_UPDATE` mode 0: one icon, cleared by guid 0.
    pub(crate) fn apply_raid_target(&mut self, icon: u8, guid: u64) {
        if let Some(slot) = self.raid_targets.get_mut(icon as usize) {
            *slot = guid;
        }
    }

    /// `MSG_RAID_TARGET_UPDATE` mode 1: the whole board; an absent icon is unset.
    pub(crate) fn apply_raid_target_list(&mut self, entries: &[(u8, u64)]) {
        self.raid_targets = [0; 8];
        for (icon, guid) in entries {
            self.apply_raid_target(*icon, *guid);
        }
    }

    /// A unit's mark on the Lua `GetRaidTargetIndex` scale, 1 to 8, or 0 when unmarked.
    pub(crate) fn raid_target_index(&self, guid: u64) -> u8 {
        if guid == 0 {
            return 0;
        }
        self.raid_targets
            .iter()
            .position(|g| *g == guid)
            .map_or(0, |i| i as u8 + 1)
    }

    /// The all-zero `SMSG_GROUP_LIST`: resets the group facts only, since the feed fires events on
    /// the ready-check and lockout state's edges. The destructure has no `..`, so a new field does
    /// not compile until it is sorted onto one side.
    pub(crate) fn leave_group(&mut self) {
        let GroupState {
            wrath_roster,
            wrath_stats,
            wrath_raid,
            // Group facts.
            in_group,
            group_type,
            own_flags,
            members,
            slots_emptied,
            leader,
            loot,
            stats,
            raid_targets,
            test,
            // Cleared with the group. The reference's side is untraced; vmangos tells an invitee
            // nothing on a disband (`Group::RemoveAllInvites`).
            pending_invite,
            // Session state the feed fires edges on: kept until `clear_session`.
            lists_applied: _,
            roster_updates: _,
            wrath_saved_instances: _,
            ready_check: _,
            ready_check_requests: _,
            ready_check_answers: _,
        } = self;
        *wrath_roster = None;
        wrath_stats.clear();
        // Preserve ordered presentation edges across leave/rejoin in one tick.
        // Otherwise a new journal of equal length would look already forwarded.
        let mut ready_events = std::mem::take(&mut wrath_raid.ready_events);
        if !matches!(
            ready_events.last(),
            None | Some(benilla_protocol::wrath_group::raid::Packet::ReadyFinished)
        ) {
            ready_events.push(benilla_protocol::wrath_group::raid::Packet::ReadyFinished);
        }
        *wrath_raid = Default::default();
        wrath_raid.ready_events = ready_events;
        *in_group = false;
        *group_type = 0;
        *own_flags = 0;
        members.clear();
        *slots_emptied = false;
        *leader = 0;
        *loot = None;
        stats.clear();
        *raid_targets = [0; 8];
        *test = false;
        *pending_invite = None;
    }

    /// Session teardown: everything resets with the socket.
    pub(crate) fn clear_session(&mut self) {
        *self = GroupState::default();
    }

    /// `MSG_RAID_READY_CHECK`, open form, echoed to the whole group: `0x4ba360` splits on the
    /// leader guid, and only a non-leader prints the leader's line and gets the popup. Both arms
    /// count the request and start a fresh answer log.
    pub(crate) fn apply_ready_check_request(&mut self, we_lead: bool) -> Vec<UiError> {
        self.ready_check_requests = self.ready_check_requests.wrapping_add(1);
        self.ready_check_answers.clear();
        if we_lead {
            return Vec::new();
        }
        self.ready_check = self.ready_check.wrapping_add(1);
        // The reference prints its name cache's entry for the leader; the roster holds the same.
        let leader = self
            .members
            .iter()
            .find(|m| m.guid == self.leader)
            .map(|m| m.name.as_str())
            .unwrap_or("");
        vec![UiError::s("ERR_RAID_LEADER_READY_CHECK_START_S", leader)]
    }

    /// `MSG_RAID_READY_CHECK`, answer form: one member's answer, forwarded to the leader alone.
    pub(crate) fn apply_ready_check_answer(&mut self, guid: u64, ready: bool) {
        self.ready_check_answers.push((guid, ready));
    }
}

impl GroupState {
    /// The member record for `guid`, party or raid (`0x496400`).
    pub(crate) fn member_record(&self, guid: u64) -> Option<&PartyMemberStatsInfo> {
        self.stats.get(&guid)
    }

    /// The record whose pet block names `guid` (`0x496420`); a pet is found only while its owner's
    /// record reads online, party (`0x4e8860`) or raid (`0x4bb130`).
    pub(crate) fn pet_record(&self, guid: u64) -> Option<&PartyMemberStatsInfo> {
        self.stats
            .values()
            .find(|r| guid != 0 && r.pet_guid == Some(guid) && r.is_online())
    }

    /// Whether the roster fallback of `UnitExists` (`0x491900`) names `member`'s pet, which the
    /// object manager may not hold and which `partypetN` or `raidpetN` resolved to a guid: a held
    /// member answers off its descriptor's `CHARM`/`SUMMON` (`0x4e814d`, `0x4bb022`), an unheld one
    /// off its record's pet guid while the record reads online (`0x4e816b`, `0x4bb040`). That test
    /// is the party pet's own online gate already, but not the raid pet's guid, which the resolver
    /// reads off the record with none (`0x4919ae`).
    pub(crate) fn roster_names_pet(&self, member: u64, member_held: bool) -> bool {
        member_held
            || self
                .wrath_record(member)
                .is_some_and(|r| r.status.unwrap_or(0) & 1 != 0)
            || self
                .member_record(member)
                .is_some_and(PartyMemberStatsInfo::is_online)
    }

    /// `partypetN`'s guid for the member `member` (`0x4e81d0`): `CHARM`, else `SUMMON`, off the
    /// member's descriptor while it is held (`0x4e8204`-`0x4e821a`), else the record's pet guid
    /// while the record reads online (`0x4e8227`-`0x4e8236`).
    pub(crate) fn party_pet_guid(&self, member: u64, live: Option<&ObjectFields>) -> Option<u64> {
        self.pet_guid_of(member, live, true)
    }

    /// `raidpetN`'s guid for the raid member `member` (`0x491960`): `CHARM`, else `SUMMON`, off the
    /// member's descriptor while it is held (`0x491993`-`0x4919a9`), else the record's pet guid
    /// with no online test (`0x4919ae`), where `partypetN` has one.
    pub(crate) fn raid_pet_guid(&self, member: u64, live: Option<&ObjectFields>) -> Option<u64> {
        self.pet_guid_of(member, live, false)
    }

    /// The guid `UNIT_PET` for the owner `member` keys on, party or raid: `CHARM`, else `SUMMON`, off
    /// the member's descriptor while it is held, else the record's pet guid, with no online test.
    /// The stats handler's diff fires it on a moved pet guid (`0x5e55fb`-`0x5e5617`, `0x5e57e9`); a
    /// moved status byte sets another bit (`0x5e5519`-`0x5e552a`), which is health and power events,
    /// and the list's own write of the online bit fires nothing (`0x4e8361`). A `partypetN` that
    /// appears or vanishes behind the bit is therefore no move of this guid.
    pub(crate) fn pet_edge_guid(&self, member: u64, live: Option<&ObjectFields>) -> Option<u64> {
        self.pet_guid_of(member, live, false)
    }

    pub(crate) fn pet_guid_of(
        &self,
        member: u64,
        live: Option<&ObjectFields>,
        online_only: bool,
    ) -> Option<u64> {
        match live {
            Some(fields) => fields.unit_pet_guid(),
            None if self.wrath_roster.is_some() => self
                .wrath_record(member)
                .filter(|r| !online_only || r.status.unwrap_or(0) & 1 != 0)
                .and_then(|r| r.pet_guid)
                .filter(|&g| g != 0),
            None => self
                .member_record(member)
                .filter(|r| !online_only || r.is_online())
                .and_then(|r| r.pet_guid)
                .filter(|&g| g != 0),
        }
    }

    /// The aura slots `UnitBuff` and `UnitDebuff` walk for a guid with no live object
    /// (`0x519735`, `0x519b25`), ascending, buffs then debuffs: a member record's block (`+0x1a`)
    /// unless it reads offline, which answers nothing and never tries the pet records
    /// (`0x519741`); on a miss, the block of the pet record naming the guid (`+0xe4`, `0x519760`).
    pub(crate) fn roster_aura_slots(&self, guid: u64) -> impl Iterator<Item = (u8, u16)> + '_ {
        let blocks = match self.member_record(guid) {
            Some(r) if !r.is_online() => None,
            Some(r) => Some((&r.auras, &r.auras_negative)),
            None => self
                .pet_record(guid)
                .map(|r| (&r.pet_auras, &r.pet_auras_negative)),
        };
        blocks
            .into_iter()
            .flat_map(|(buffs, debuffs)| buffs.iter().chain(debuffs).flatten().copied())
    }
}

impl GroupState {
    pub(crate) fn apply_wrath_list(
        &mut self,
        roster: benilla_protocol::wrath_group::Roster,
        me: Option<u64>,
    ) -> ListOutcome {
        let members = roster
            .members
            .iter()
            .map(|m| GroupMemberEntry {
                name: m.name.clone(),
                guid: m.guid,
                status: m.status,
                // Shared presentation only; the source subgroup, flags and roles stay in roster.
                flags: m.subgroup | if m.flags & 1 != 0 { 0x80 } else { 0 },
            })
            .collect();
        let own = roster.subgroup | if roster.flags & 1 != 0 { 0x80 } else { 0 };
        let loot = roster.loot.as_ref().map(|l| GroupLootInfo {
            method: l.method,
            master: l.master,
            threshold: l.threshold,
        });
        let out = self.apply_list(
            if roster.group_type & 2 != 0 { 1 } else { 0 },
            own,
            members,
            roster.leader,
            loot,
            me,
        );
        if roster.leader != 0 {
            self.wrath_stats
                .retain(|guid, _| roster.members.iter().any(|m| m.guid == *guid));
            for m in &roster.members {
                let s = self.wrath_stats.entry(m.guid).or_insert_with(|| Stats {
                    guid: m.guid,
                    cur_hp: Some(1),
                    max_hp: Some(1),
                    ..Default::default()
                });
                s.status = Some((s.status.unwrap_or(0) & !1) | u16::from(m.status & 1));
            }
            self.wrath_roster = Some(roster);
        }
        out
    }

    pub(crate) fn apply_wrath_stats(&mut self, patch: Stats) {
        if !self.members.iter().any(|m| m.guid == patch.guid) {
            return;
        }
        let record = self.wrath_stats.entry(patch.guid).or_default();
        if patch.full_prefix.is_some() {
            *record = Stats {
                guid: patch.guid,
                ..Default::default()
            };
        }
        record.guid = patch.guid;
        record.mask |= patch.mask;
        record.full_prefix = patch.full_prefix.or(record.full_prefix);
        if patch.status.is_some() {
            record.status = patch.status;
        }
        if patch.cur_hp.is_some() {
            record.cur_hp = patch.cur_hp;
        }
        if patch.max_hp.is_some() {
            record.max_hp = patch.max_hp;
        }
        if patch.power_type.is_some() {
            record.power_type = patch.power_type;
        }
        if patch.cur_power.is_some() {
            record.cur_power = patch.cur_power;
        }
        if patch.max_power.is_some() {
            record.max_power = patch.max_power;
        }
        if patch.level.is_some() {
            record.level = patch.level;
        }
        if patch.zone.is_some() {
            record.zone = patch.zone;
        }
        if patch.position.is_some() {
            record.position = patch.position;
        }
        if patch.pet_guid.is_some() {
            record.pet_guid = patch.pet_guid;
        }
        if patch.pet_name.is_some() {
            record.pet_name = patch.pet_name;
        }
        if patch.pet_model_id.is_some() {
            record.pet_model_id = patch.pet_model_id;
        }
        if patch.pet_cur_hp.is_some() {
            record.pet_cur_hp = patch.pet_cur_hp;
        }
        if patch.pet_max_hp.is_some() {
            record.pet_max_hp = patch.pet_max_hp;
        }
        if patch.pet_power_type.is_some() {
            record.pet_power_type = patch.pet_power_type;
        }
        if patch.pet_cur_power.is_some() {
            record.pet_cur_power = patch.pet_cur_power;
        }
        if patch.pet_max_power.is_some() {
            record.pet_max_power = patch.pet_max_power;
        }
        if patch.vehicle_seat.is_some() {
            record.vehicle_seat = patch.vehicle_seat;
        }
        if let Some(patch) = patch.auras {
            let slots = record.auras.get_or_insert_with(Vec::new);
            for aura in patch {
                slots.retain(|old| old.slot != aura.slot);
                if aura.spell != 0 {
                    slots.push(aura);
                }
            }
            slots.sort_by_key(|a| a.slot);
        }
        if let Some(patch) = patch.pet_auras {
            let slots = record.pet_auras.get_or_insert_with(Vec::new);
            for aura in patch {
                slots.retain(|old| old.slot != aura.slot);
                if aura.spell != 0 {
                    slots.push(aura);
                }
            }
            slots.sort_by_key(|a| a.slot);
        }
    }

    pub(crate) fn wrath_record(&self, guid: u64) -> Option<&Stats> {
        self.wrath_stats.get(&guid)
    }

    pub(crate) fn wrath_pet_record(&self, guid: u64) -> Option<&Stats> {
        self.wrath_stats
            .values()
            .find(|s| guid != 0 && s.pet_guid == Some(guid) && s.status.unwrap_or(0) & 1 != 0)
    }
}

impl GroupState {
    pub(crate) fn wrath_aura_slots(&self, guid: u64) -> Option<Vec<(u8, u32, u8, u8)>> {
        self.wrath_roster.as_ref()?;
        let (record, pet) = match self.wrath_record(guid) {
            Some(r) => (r, false),
            None => (self.wrath_pet_record(guid)?, true),
        };
        if record.status.unwrap_or(0) & 1 == 0 {
            return Some(Vec::new());
        }
        let auras = if pet {
            &record.pet_auras
        } else {
            &record.auras
        };
        Some(
            auras
                .iter()
                .flatten()
                .map(|a| (a.slot, a.spell, 1, a.flags))
                .collect(),
        )
    }
}

/// The subgroup bits (0-2) of a member's flags byte; `0x80` is the assistant bit.
pub(crate) const GROUP_MEMBER_SUBGROUP: u8 = 0x07;

/// What one `SMSG_GROUP_LIST` shows: its lines, and whether it plays `igPlayerInviteAccept`
/// (`0x5e6c83`-`0x5e6c8f`), the join's only sound, since `ERR_JOINED_GROUP_S`'s row has none.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ListOutcome {
    pub lines: Vec<UiError>,
    pub invite_accept: bool,
}
