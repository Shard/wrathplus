//! The raid rows as guids, the order the RaidFrame verbs and the `raidN` tokens address.

use super::group::GroupState;
use super::state::GROUPTYPE_RAID;

/// The raid rows as guids, in the order `ui_party`'s `raid_roster` builds them, which the
/// RaidFrame's drag, kick and menu verbs address by index: the two must agree. Us first, then the
/// wire's members; the reference appends the local player after them (`0x5e6e73`).
/// Empty outside a raid.
pub(crate) fn raid_row_guids(group: &GroupState, self_guid: Option<u64>) -> Vec<u64> {
    raid_rows(group, self_guid).collect()
}

/// [`raid_row_guids`] as an iterator, so a per-token lookup allocates nothing.
fn raid_rows(group: &GroupState, self_guid: Option<u64>) -> impl Iterator<Item = u64> + '_ {
    let in_raid = group.group_type == GROUPTYPE_RAID;
    self_guid.filter(|_| in_raid).into_iter().chain(
        group
            .members
            .iter()
            .filter(move |_| in_raid)
            .map(|m| m.guid),
    )
}

/// The guid at a 1-based raid row, [`raid_row_guids`]`[index - 1]` without the allocation.
pub(crate) fn raid_row_guid(
    group: &GroupState,
    self_guid: Option<u64>,
    index: usize,
) -> Option<u64> {
    raid_rows(group, self_guid).nth(index.checked_sub(1)?)
}
