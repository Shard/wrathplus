//! The party and raid state (roster, loot rules, the Wrath raid extension, saved instances and the
//! party pets), read by world and net code; `ui_party` keeps the systems and re-exports these from
//! their old paths.

use std::collections::HashMap;

use benilla_protocol::messages::{GroupLootInfo, GroupMemberEntry, PartyMemberStatsInfo};
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

#[derive(Default)]
pub(crate) struct NativeSavedInstances {
    pub(crate) snapshot: Option<RaidInstanceInfo>,
    pub(crate) receipts: Vec<RaidInstanceInfo>,
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
