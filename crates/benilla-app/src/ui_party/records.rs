//! The roster records' readers for a guid the object manager does not hold: the member record
//! (`0x496400`), the pet record (`0x496420`) and the party and raid pets' guids (`0x4e81d0`,
//! `0x491960`).

#[cfg(test)]
use super::GroupState;
#[cfg(test)]
use benilla_protocol::types::{ObjectFields, PartyMemberStatsInfo};

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::types::{member_status, GroupMemberEntry};

    const MEMBER: u64 = 0x1234;
    const PET: u64 = 0xF140_0000_0000_0077;

    fn group_with(record: PartyMemberStatsInfo) -> GroupState {
        let mut g = GroupState::default();
        g.apply_list(
            0,
            0,
            vec![GroupMemberEntry {
                name: "Brisca".into(),
                guid: MEMBER,
                status: member_status::ONLINE,
                flags: 0,
            }],
            MEMBER,
            None,
            None,
        );
        g.stats.insert(MEMBER, record);
        g
    }

    fn record(status: u8) -> PartyMemberStatsInfo {
        PartyMemberStatsInfo {
            status: Some(status),
            auras: Some(vec![(0, 1126), (4, 21562)]),
            auras_negative: Some(vec![(32, 589)]),
            pet_guid: Some(PET),
            pet_auras: Some(vec![(2, 1126)]),
            pet_auras_negative: Some(vec![(40, 770)]),
            ..PartyMemberStatsInfo::default()
        }
    }

    #[test]
    fn a_member_record_answers_its_own_block_and_an_offline_one_answers_nothing() {
        let g = group_with(record(member_status::ONLINE));
        let slots = |g: &GroupState, guid| g.roster_aura_slots(guid).collect::<Vec<_>>();
        assert_eq!(
            slots(&g, MEMBER),
            [(0, 1126), (4, 21562), (32, 589)],
            "buffs then debuffs, ascending slot"
        );
        assert_eq!(slots(&g, PET), [(2, 1126), (40, 770)]);
        assert!(slots(&g, 0x9999).is_empty(), "a miss everywhere");

        let g = group_with(record(member_status::OFFLINE));
        assert!(slots(&g, MEMBER).is_empty());
        assert!(
            slots(&g, PET).is_empty(),
            "an offline owner's pet is not found"
        );
    }

    /// `0x4e82d0` copies the saved record back and then sets or clears bit 0 of its status byte
    /// from the row's (`0x4e8361`-`0x4e837b`); the raid roster's twin does the same to its row's
    /// record (`0x4ba947`-`0x4ba960`). Both test the row's byte as non-zero.
    #[test]
    fn a_list_rewrites_a_known_members_online_bit_and_keeps_the_rest_of_the_record() {
        use crate::ui_party::GROUPTYPE_RAID;
        for group_type in [0, GROUPTYPE_RAID] {
            let mut g = group_with(record(
                member_status::ONLINE | member_status::DEAD | member_status::PVP,
            ));
            let relist = |g: &mut GroupState, status: u8| {
                let row = |guid, name: &str| GroupMemberEntry {
                    name: name.into(),
                    guid,
                    status,
                    flags: 0,
                };
                g.apply_list(
                    group_type,
                    0,
                    vec![row(MEMBER, "Brisca"), row(0x5678, "Aldwyn")],
                    MEMBER,
                    None,
                    None,
                );
            };

            relist(&mut g, member_status::OFFLINE);
            assert_eq!(
                g.stats[&MEMBER],
                record(member_status::DEAD | member_status::PVP),
                "the bit clears and the rest of the record stays as it was"
            );
            assert!(g.roster_aura_slots(MEMBER).next().is_none());
            assert_eq!(g.party_pet_guid(MEMBER, None), None);

            relist(&mut g, member_status::ONLINE | member_status::AFK);
            assert_eq!(
                g.stats[&MEMBER],
                record(member_status::ONLINE | member_status::DEAD | member_status::PVP),
                "the bit sets, and the row's other bits are not copied"
            );
            assert_eq!(g.party_pet_guid(MEMBER, None), Some(PET));

            relist(&mut g, member_status::AFK);
            assert!(
                g.stats[&MEMBER].is_online(),
                "a non-zero byte is online, bit 0 or not"
            );
            assert!(
                !g.stats.contains_key(&0x5678),
                "a member new to the roster gets its record from the seat, not here"
            );
        }
    }

    #[test]
    fn the_party_pet_guid_is_the_live_charm_then_summon_else_the_records() {
        let g = group_with(record(member_status::ONLINE));
        assert_eq!(g.party_pet_guid(MEMBER, None), Some(PET));

        let summon = ObjectFields::from_field_slots(&[
            (benilla_protocol::types::Field::UnitSummon, 0, 0x55),
            (benilla_protocol::types::Field::UnitSummon, 1, 0),
        ]);
        assert_eq!(g.party_pet_guid(MEMBER, Some(&summon)), Some(0x55));
        let both = ObjectFields::from_field_slots(&[
            (benilla_protocol::types::Field::UnitCharm, 0, 0x66),
            (benilla_protocol::types::Field::UnitCharm, 1, 0),
            (benilla_protocol::types::Field::UnitSummon, 0, 0x55),
            (benilla_protocol::types::Field::UnitSummon, 1, 0),
        ]);
        assert_eq!(
            g.party_pet_guid(MEMBER, Some(&both)),
            Some(0x66),
            "CHARM first"
        );
        // A held member with neither is petless, whatever the record says.
        assert_eq!(
            g.party_pet_guid(MEMBER, Some(&ObjectFields::default())),
            None
        );

        let g = group_with(record(member_status::OFFLINE));
        assert_eq!(g.party_pet_guid(MEMBER, None), None);
    }

    /// `raidpetN` reads the same fields as `partypetN`, but its record leg has no online test
    /// (`0x4919ae`, against `partypetN`'s `0x4e8227`).
    #[test]
    fn the_raid_pet_guid_reads_the_record_whatever_its_online_bit() {
        let g = group_with(record(member_status::ONLINE));
        assert_eq!(g.raid_pet_guid(MEMBER, None), Some(PET));
        let summon = ObjectFields::from_field_slots(&[
            (benilla_protocol::types::Field::UnitSummon, 0, 0x55),
            (benilla_protocol::types::Field::UnitSummon, 1, 0),
        ]);
        assert_eq!(g.raid_pet_guid(MEMBER, Some(&summon)), Some(0x55));
        assert_eq!(
            g.raid_pet_guid(MEMBER, Some(&ObjectFields::default())),
            None
        );

        let g = group_with(record(member_status::OFFLINE));
        assert_eq!(g.raid_pet_guid(MEMBER, None), Some(PET));
        assert!(
            g.roster_aura_slots(PET).next().is_none(),
            "named, but the pet record is not found behind an offline owner"
        );
    }
}
