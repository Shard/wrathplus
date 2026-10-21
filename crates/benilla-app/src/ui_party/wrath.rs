//! Native group state and the shared roster presentation boundary. No stats are
//! converted to Vanilla records; native u32 health/spells and aura flags remain held.

#[cfg(test)]
use super::GroupState;
#[cfg(test)]
use benilla_protocol::wrath_group::{Roster, Stats};

pub(super) fn power_scale(ty: u32) -> u32 {
    if matches!(ty, 1 | 6) {
        10
    } else if ty == 4 {
        1000
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::wrath_group::{self as g, Aura, Member};
    fn roster() -> Roster {
        Roster {
            group_type: 0,
            subgroup: 0,
            flags: 0,
            roles: 2,
            lfg: None,
            group_guid: 42,
            counter: 1,
            members: vec![Member {
                name: "Ally".into(),
                guid: 7,
                status: 1,
                subgroup: 0,
                flags: 0,
                roles: 4,
            }],
            leader: 7,
            loot: None,
        }
    }
    #[test]
    fn native_stats_retain_large_health_ids_and_delta_clear_without_narrowing() {
        let mut state = GroupState::default();
        state.apply_wrath_list(roster(), Some(1));
        state.apply_wrath_stats(Stats {
            guid: 7,
            mask: g::mask::CUR_HP | g::mask::AURAS,
            cur_hp: Some(500000),
            auras: Some(vec![Aura {
                slot: 63,
                spell: 123456,
                flags: 0x10,
            }]),
            ..Default::default()
        });
        assert_eq!(state.wrath_stats[&7].cur_hp, Some(500000));
        assert_eq!(state.wrath_aura_slots(7).unwrap(), [(63, 123456, 1, 0x10)]);
        state.apply_wrath_stats(Stats {
            guid: 7,
            cur_hp: Some(0),
            auras: Some(vec![Aura {
                slot: 63,
                spell: 0,
                flags: 0,
            }]),
            ..Default::default()
        });
        assert_eq!(state.wrath_stats[&7].cur_hp, Some(0));
        assert!(state.wrath_aura_slots(7).unwrap().is_empty());
        state.apply_wrath_stats(Stats {
            guid: 7,
            status: Some(0),
            pet_guid: Some(88),
            ..Default::default()
        });
        assert_eq!(state.party_pet_guid(7, None), None);
        state.clear_session();
        assert!(state.wrath_stats.is_empty());
        assert!(state.wrath_roster.is_none());
    }
}
