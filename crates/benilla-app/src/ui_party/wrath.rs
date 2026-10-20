//! Native group state and the shared roster presentation boundary. No stats are
//! converted to Vanilla records; native u32 health/spells and aura flags remain held.
use super::{GroupState, ListOutcome};
use benilla_protocol::{
    messages::{GroupLootInfo, GroupMemberEntry},
    wrath_group::{Roster, Stats},
};

impl GroupState {
    pub(super) fn apply_wrath_list(&mut self, roster: Roster, me: Option<u64>) -> ListOutcome {
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
    pub(super) fn apply_wrath_stats(&mut self, patch: Stats) {
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
