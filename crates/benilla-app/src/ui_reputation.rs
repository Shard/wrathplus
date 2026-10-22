//! The reputation pane's feed: the player's wire reputation slots resolved against `Faction.dbc`,
//! pushed as one snapshot with `UPDATE_FACTION` on a change, and the pane's three verbs sent.
//!
//! A faction is a row when it has a reputation slot and one of its four race/class mask slots
//! fits the player; the client adds it inside the accept block that picks the base value
//! (`0x4d5555`). The pane lists only rows with `VISIBLE`, which `SMSG_SET_FACTION_VISIBLE` sets,
//! but unlisted rows are pushed too: they carry the header names, and only one of the five header
//! factions is `VISIBLE`. Flag `0x08` is `HEADER`, not "force invisible".
//!
//! The wire standing excludes the race/class base, so the pane's total is base plus wire: vmangos
//! stores `standing - BaseRep` (`ReputationMgr.cpp:261`) and reports the sum
//! (`ReputationMgr.cpp:82`). The rank is [`benilla_formats::reputation_rank`], shared with the
//! unit-reaction decode, which caps it at 6, Revered (`0x606439`); the pane shows it uncapped.

use bevy::prelude::*;

use benilla_ui::script::{FactionEntry, ReputationSend, ReputationState, UiScript};

use crate::net::{ClientCommand, NetCommands, ObjectStore, Reputations, SelfPlayer};
use crate::target::Factions;
use crate::ui_script::{UiFeed, UiInput};

/// Rank `r` spans `RANK_BOUNDS[r]..RANK_BOUNDS[r + 1]`, the client's table at `0x80928c`: vmangos's
/// `PointsInRank` walked up from -42000 (`ReputationMgr.cpp:30`), the edges
/// [`benilla_formats::reputation_rank`] ranks by. The Lua `standingID` is the rank plus one.
const RANK_BOUNDS: [i32; 9] = [-42000, -6000, -3000, 0, 3000, 9000, 21000, 42000, 43000];

/// The reputation pane's feed and outbound drain; the bindings are `benilla-ui`'s.
pub(crate) struct UiReputationPlugin;

impl Plugin for UiReputationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                feed_reputation.in_set(UiFeed),
                drain_reputation_sends.after(UiInput),
            ),
        );
    }
}

/// One faction's row, or `None` when it is not in the player's list at all; a row the pane does
/// not draw is still a row, with `visible: false`.
pub(crate) fn reputation_row(
    faction_id: u32,
    info: &benilla_formats::FactionInfo,
    catalog: &benilla_formats::FactionCatalog,
    flags: u8,
    wire_standing: i32,
    race: u8,
    class: u8,
) -> Option<FactionEntry> {
    use benilla_formats::faction_flags as flag;
    // The membership gate: no fitting race/class slot, no row at all (`0x4d5555`).
    info.slot_for(race, class)?;
    let standing = info.base_for(race, class) + wire_standing;
    let rank = benilla_formats::reputation_rank(standing);
    Some(FactionEntry {
        faction_id,
        rep_list_id: u32::try_from(info.rep_index).ok()?,
        parent_id: info.team,
        name: catalog.faction_name(faction_id)?.to_string(),
        description: catalog
            .faction_description(faction_id)
            .unwrap_or_default()
            .to_string(),
        standing,
        standing_id: rank + 1,
        bar_min: RANK_BOUNDS[rank as usize],
        bar_max: RANK_BOUNDS[rank as usize + 1],
        visible: flags & flag::VISIBLE != 0,
        is_header: flags & flag::HEADER != 0,
        at_war: flags & flag::AT_WAR != 0,
        // What `GetFactionInfo` reports; the toggle itself is stricter (`0x4d5fd0`).
        can_toggle_at_war: flags & flag::PEACE_FORCED == 0 && standing >= -3000,
        inactive: flags & flag::INACTIVE != 0,
        // A header with its own bar: the fitting slot's ReputationFlags 0x80 (`0x5d0ce0`).
        has_rep: info.default_flags_for(race, class) & 0x80 != 0 && info.rep_index >= 0,
    })
}

/// The by-value inputs of the feed's gate, `(race, class, watched)`; the other two are resources.
type ReputationInputs = Option<(u8, u8, Option<u32>)>;

/// Push the snapshot and fire `UPDATE_FACTION` when it differs. The build (54 rows of names and
/// descriptions) runs only when the wire slots, the catalog, race/class or the watched index
/// move, and the equality check behind it keeps a change to no listed row quiet. The first push
/// fires too: the stock watch bar initializes off `UPDATE_FACTION` at login.
fn feed_reputation(
    script: Option<NonSendMut<UiScript>>,
    self_store: Query<&ObjectStore, With<SelfPlayer>>,
    reputations: Res<Reputations>,
    factions: Option<Res<Factions>>,
    mut last: Local<crate::ui_script::VmMemo<Option<ReputationState>>>,
    mut last_inputs: Local<crate::ui_script::VmMemo<ReputationInputs>>,
) {
    let Some(mut script) = script else {
        return;
    };
    let last = last.get(&script);
    let last_inputs = last_inputs.get(&script);
    let (Ok(store), Some(factions_res)) = (self_store.single(), factions.as_ref()) else {
        return;
    };
    let catalog = factions_res.catalog();
    let (race, class) = (
        store.0.unit_race().unwrap_or(0),
        store.0.unit_class().unwrap_or(0),
    );
    let watched = store
        .0
        .player_watched_faction()
        .and_then(|i| u32::try_from(i).ok());

    let inputs = (race, class, watched);
    let moved = reputations.is_changed()
        || factions_res.is_changed()
        || last_inputs.as_ref() != Some(&inputs)
        || last.is_none();
    if !moved {
        return;
    }
    *last_inputs = Some(inputs);

    let mut entries = Vec::new();
    for (faction_id, info) in catalog.reputation_factions() {
        // An uncovered slot reads `(0, 0)`, unmet, and its row stays: a header's name rides on it.
        let (flags, wire) = usize::try_from(info.rep_index)
            .ok()
            .and_then(|i| reputations.0.get(i))
            .copied()
            .unwrap_or((0, 0));
        if let Some(row) = reputation_row(faction_id, info, catalog, flags, wire, race, class) {
            entries.push(row);
        }
    }
    // Sorted before the equality check, so a map reshuffle is not a change.
    entries.sort_by_key(|e| e.faction_id);

    let fresh = ReputationState { entries, watched };
    if last.as_ref() == Some(&fresh) {
        return;
    }
    script.set_reputation(fresh.clone());
    *last = Some(fresh);
    script.fire_event("UPDATE_FACTION", vec![]);
}

/// Send the pane's queued verbs; none is acked, and the engine already holds its own copy.
fn drain_reputation_sends(script: Option<NonSendMut<UiScript>>, commands: Res<NetCommands>) {
    let Some(mut script) = script else {
        return;
    };
    for send in script.take_reputation_sends() {
        let cmd = match send {
            ReputationSend::AtWar {
                rep_list_id,
                at_war,
            } => ClientCommand::SetFactionAtWar {
                rep_list_id,
                at_war,
            },
            ReputationSend::Inactive {
                rep_list_id,
                inactive,
            } => ClientCommand::SetFactionInactive {
                rep_list_id,
                inactive,
            },
            // No watch is -1 on the wire: slot 0 is the Bloodsail Buccaneers.
            ReputationSend::Watch(slot) => ClientCommand::SetWatchedFaction {
                rep_list_id: slot
                    .map_or(benilla_protocol::types::WATCHED_FACTION_NONE, |s| s as i32),
            },
        };
        let _ = commands.0.send(cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pane's bar reads [`RANK_BOUNDS`], the nameplate the rank function: they must agree.
    #[test]
    fn rank_bounds_agree_with_the_rank_function() {
        for rank in 0u8..=7 {
            let (floor, ceiling) = (RANK_BOUNDS[rank as usize], RANK_BOUNDS[rank as usize + 1]);
            assert_eq!(
                benilla_formats::reputation_rank(floor),
                rank,
                "rank {rank}'s floor {floor}"
            );
            assert_eq!(
                benilla_formats::reputation_rank(ceiling - 1),
                rank,
                "rank {rank}'s top {}",
                ceiling - 1
            );
            if rank > 0 {
                assert_eq!(
                    benilla_formats::reputation_rank(floor - 1),
                    rank - 1,
                    "one below rank {rank}'s floor"
                );
            }
        }
        // vmangos's `PointsInRank` (`ReputationMgr.cpp:30`).
        let widths: Vec<i32> = RANK_BOUNDS.windows(2).map(|w| w[1] - w[0]).collect();
        assert_eq!(widths, [36000, 3000, 3000, 3000, 6000, 12000, 21000, 1000]);
    }

    /// Stormwind (72) has four race-gated base slots, `0x4c` 3100, `0xb2` (Horde) -42000, `0x01`
    /// (Human) 4000 and an empty one, so a human starts at 4000, Friendly, from the third slot.
    /// Skips without client data.
    #[test]
    fn real_stormwind_starts_friendly_for_a_human_and_hidden_factions_never_list() {
        let data = benilla_formats::wow_data_or_skip!();
        let mut chain = benilla_formats::open_chain(&data).expect("open chain");
        let cat = benilla_formats::load_faction_catalog(&mut chain).expect("factions");
        use benilla_formats::faction_flags as flag;
        // A human warrior (race 1, class 1) who has met Stormwind and gained nothing since.
        let sw = cat.reputation_faction(72).expect("Stormwind");
        let row = reputation_row(72, sw, &cat, flag::VISIBLE | flag::PEACE_FORCED, 0, 1, 1)
            .expect("Stormwind lists");
        assert_eq!(row.name, "Stormwind");
        assert_eq!(
            row.standing, 4000,
            "the human-gated base slot, with no wire gain on top"
        );
        assert_eq!(row.standing_id, 5, "FACTION_STANDING_LABEL5 = Friendly");
        assert_eq!((row.bar_min, row.bar_max), (3000, 9000));
        assert_eq!(row.parent_id, 469, "grouped under Alliance");
        assert!(
            !row.can_toggle_at_war,
            "PEACE_FORCED: you cannot go to war with your own people"
        );
        // A wire gain rides on top of the base.
        let honored = reputation_row(72, sw, &cat, flag::VISIBLE, 5000, 1, 1).expect("lists");
        assert_eq!(honored.standing, 9000);
        assert_eq!(honored.standing_id, 6, "Honored begins exactly at 9000");
        assert_eq!((honored.bar_min, honored.bar_max), (9000, 21000));

        // The same empty wire slot, read by an orc: the Horde-gated slot.
        let orc = reputation_row(72, sw, &cat, flag::VISIBLE, 0, 2, 1).expect("lists");
        assert_eq!(orc.standing, -42000, "the Horde race mask's base");
        assert_eq!(orc.standing_id, 1, "FACTION_STANDING_LABEL1 = Hated");

        // Unmet is not visible, not absent: the row comes back with its name.
        let unmet = reputation_row(72, sw, &cat, 0, 0, 1, 1).unwrap();
        assert!(!unmet.visible, "unmet");
        assert_eq!(unmet.name, "Stormwind", "and still carries its name");
        // `HIDDEN` suppresses the auto-reveal and the rank-change chat line, never the row.
        let hidden = reputation_row(72, sw, &cat, flag::VISIBLE | flag::HIDDEN, 0, 1, 1).unwrap();
        assert!(hidden.visible, "HIDDEN is not a list gate");
        assert!(!hidden.is_header, "and it is not the header bit either");
    }

    /// On 12340's data the membership gate (`0x4d5555`) excludes Cenarion Circle (609) for exactly
    /// the druids of the races that cannot be druids. Its first slot takes every race with class
    /// mask `0x1df`, no druids, and its second Night Elf and Tauren druids at 2000, so a rule that
    /// stopped at the first match would give a Night Elf druid 0. Skips without client data.
    #[test]
    fn real_membership_gate_excludes_only_druids_of_the_six_non_druid_races() {
        let data = benilla_formats::wow_data_or_skip!();
        let mut chain = benilla_formats::open_chain(&data).expect("open chain");
        let cat = benilla_formats::load_faction_catalog(&mut chain).expect("factions");

        // 3.3.5's ten classes (no 10); druid is 11. Races 1-8, 10 and 11 (9 is no player's).
        const CLASSES: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 11];
        const RACES: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 10, 11];
        const DRUID: u8 = 11;
        let mut excluded: Vec<(u32, u8, u8)> = Vec::new();
        for race in RACES {
            for class in CLASSES {
                for (id, info) in cat.reputation_factions() {
                    if info.slot_for(race, class).is_none() {
                        excluded.push((id, race, class));
                    }
                }
            }
        }
        // Cenarion Circle excludes exactly the druids of the races that cannot be druids. (12340
        // gates other factions too, by race or class, for reasons this test does not pin.)
        let cc: Vec<(u8, u8)> = excluded
            .iter()
            .filter(|e| e.0 == 609)
            .map(|&(_, race, class)| (race, class))
            .collect();
        let expected: Vec<(u8, u8)> = RACES
            .into_iter()
            .filter(|r| ![4, 6].contains(r))
            .map(|race| (race, DRUID))
            .collect();
        assert_eq!(
            cc, expected,
            "Cenarion Circle excludes only non-druid races' druids"
        );

        // Last match wins, and the druid slot is second.
        let cc = cat.reputation_faction(609).expect("Cenarion Circle");
        assert_eq!(
            cc.slot_for(4, DRUID),
            Some(1),
            "a Night Elf druid takes slot 1"
        );
        assert_eq!(cc.base_for(4, DRUID), 2000);
        assert_eq!(cc.slot_for(6, DRUID), Some(1), "so does a Tauren druid");
        assert_eq!(cc.slot_for(1, 1), Some(0), "a human warrior takes slot 0");
        assert_eq!(cc.base_for(1, 1), 0);
    }

    /// All five header factions carry `HEADER` and four lack `VISIBLE`, so filtering on visibility
    /// would leave the headers blank. The flags are the DBC defaults a human warrior is seeded with
    /// (`ReputationMgr::GetDefaultStateFlags`). Skips without client data.
    #[test]
    fn real_header_factions_carry_the_header_bit_and_still_reach_the_engine() {
        let data = benilla_formats::wow_data_or_skip!();
        let mut chain = benilla_formats::open_chain(&data).expect("open chain");
        let cat = benilla_formats::load_faction_catalog(&mut chain).expect("factions");
        use benilla_formats::faction_flags as flag;

        // Every faction some reputation faction names as its `team`: the pane's headers.
        let parents: std::collections::BTreeSet<u32> = cat
            .reputation_factions()
            .map(|(_, f)| f.team)
            .filter(|&t| t != 0)
            .collect();
        // 3.3.5 adds its own blocs (Outland, Northrend, the Vanguard and Expedition); the five 1.12
        // parents remain among them.
        for id in [67, 169, 469, 891, 892] {
            assert!(
                parents.contains(&id),
                "Horde, Steamwheedle, Alliance, and the two battleground blocs: {id} in {parents:?}"
            );
        }

        // 3.3.5's expansion blocs (980 and on) carry no default HEADER bit; the five 1.12 parents do.
        for id in [67u32, 169, 469, 891, 892] {
            let info = cat
                .reputation_faction(id)
                .unwrap_or_else(|| panic!("parent {id} has its own reputation slot"));
            let flags = u8::try_from(info.default_flags_for(1, 1)).expect("a flag byte");
            assert!(
                flags & flag::HEADER != 0,
                "parent {id} must carry the HEADER bit; got {flags:#04x}"
            );
            let row = reputation_row(id, info, &cat, flags, 0, 1, 1)
                .unwrap_or_else(|| panic!("parent {id} still resolves"));
            assert!(row.is_header, "parent {id} is a header row");
            assert!(
                !row.name.is_empty(),
                "parent {id} reaches the engine WITH its name — that name is the header"
            );
        }
        // Only one of the five is also `VISIBLE`.
        let visible_parents = [67u32, 169, 469, 891, 892]
            .into_iter()
            .filter(|&id| {
                let info = cat.reputation_faction(id).unwrap();
                u8::try_from(info.default_flags_for(1, 1)).unwrap() & flag::VISIBLE != 0
            })
            .count();
        assert_eq!(visible_parents, 1, "only Alliance carries VISIBLE as well");
    }
}
