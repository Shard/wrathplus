//! The world-state table, the server's key/value scoreboard from `SMSG_INIT_WORLD_STATES` and
//! `SMSG_UPDATE_WORLD_STATE`. Read by the NPC-text `$<n>w`/`$<n>e` tokens ([`crate::npc_text`])
//! and by the world map's landmark pass ([`crate::ui_world_map`]), where an `AreaPOI.dbc` row with
//! a `WorldStateID` shows only while that state is nonzero.
//!
//! The reference keeps an open hash keyed by the raw wire dword (`[0xb71ec8]`, bucket
//! `mask & key`, value at node `+0x18`); keys stay raw because `$<n>e` reads the negated key
//! (`$2077e` looks up `0xFFFFF7E3`).
//!
//! An init first runs `0x4c5650(ecx = map, edx = area)`: it frees every entry, stores the pair as
//! the display filter `[0xb71e84]`/`[0xb71ea8]` ([`WorldStates::scope`]) and rebuilds the
//! world-state UI list, and only then are the packet's pairs applied. The filter is the server's
//! last init, never the avatar's position; only this clear and the logout reset write it.

pub(crate) use crate::app_core::state::WorldStates;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_miss_reads_zero_and_a_value_keeps_its_sign() {
        let mut table = WorldStates::default();
        assert_eq!(table.get(2077), 0, "empty table — the reference's `\"0\"`");
        table.write(&[(2077, 7), (2264, 0xFFFF_FFFF)]);
        assert_eq!(table.get(2077), 7);
        assert_eq!(table.get(2264), -1, "printed through `%d`");
        assert_eq!(table.get(9999), 0, "still a miss");
    }

    #[test]
    fn the_negated_key_is_a_separate_entry() {
        let mut table = WorldStates::default();
        table.write(&[(2077, 7)]);
        assert_eq!(table.get(2077u32.wrapping_neg()), 0);
        table.write(&[(2077u32.wrapping_neg(), 42)]);
        assert_eq!(table.get(2077u32.wrapping_neg()), 42);
        assert_eq!(table.get(2077), 7, "the positive key is untouched");
    }

    #[test]
    fn a_write_replaces_and_the_terminator_is_just_data() {
        let mut table = WorldStates::default();
        table.write(&[(2264, 1), (0, 0)]);
        table.write(&[(2264, 5)]);
        assert_eq!(table.get(2264), 5);
        assert_eq!(table.get(0), 0);
    }
}
