use super::*;
use benilla_protocol::messages::{Field, FieldLayout, ObjectFields};
use benilla_protocol::wrath_group::{Member, Roster};
use std::collections::HashMap;

const ME: u64 = 0x10;
const MATE: u64 = 0x11;
const PET: u64 = 0xf140_0000_0000_0007;
const MOB: u64 = 0xf130_0000_0000_0009;
const CHEST: u64 = 0xf110_0000_0000_0003;

fn unit(player: bool, created_by: u64, summoned_by: u64) -> ObjectStore {
    let layout = FieldLayout::Wrath12340;
    ObjectStore(ObjectFields::from_field_slots(
        layout,
        &[
            (Field::ObjectType, 0, if player { 0x19 } else { 9 }),
            (Field::UnitCreatedby, 0, created_by as u32),
            (Field::UnitCreatedby, 1, (created_by >> 32) as u32),
            (Field::UnitSummonedby, 0, summoned_by as u32),
            (Field::UnitSummonedby, 1, (summoned_by >> 32) as u32),
        ],
    ))
}

fn group(raid: bool) -> GroupState {
    let member = |guid, subgroup, flags| Member {
        name: String::new(),
        guid,
        status: 1,
        subgroup,
        flags,
        roles: 0,
    };
    GroupState {
        in_group: true,
        group_type: u8::from(raid),
        wrath_roster: Some(Roster {
            group_type: u8::from(raid),
            subgroup: 0,
            flags: 4,
            roles: 0,
            lfg: None,
            group_guid: 1,
            counter: 1,
            members: vec![member(MATE, 0, 2)],
            leader: ME,
            loot: None,
        }),
        ..Default::default()
    }
}

fn flags(guid: u64, group: Option<&GroupState>, reaction: u8) -> u32 {
    let stores: HashMap<u64, ObjectStore> = HashMap::from([
        (ME, unit(true, 0, 0)),
        (MATE, unit(true, 0, 0)),
        (PET, unit(false, ME, ME)),
        (MOB, unit(false, 0, 0)),
    ]);
    let store = |g: u64| stores.get(&g);
    let reaction = move |_: &ObjectStore| reaction;
    unit_flags(
        guid,
        &FlagInputs {
            me: ME,
            target: MOB,
            focus: 0,
            group,
            store: &store,
            reaction: &reaction,
        },
    )
}

/// `0x74dcb0` on the canonical units: self, own pet, a hostile targeted mob, an unknown object.
#[test]
fn the_canonical_units_flag_as_native() {
    assert_eq!(flags(0, None, 4), NONE);
    assert_eq!(
        flags(ME, None, 4),
        TYPE_PLAYER | CONTROL_PLAYER | AFFILIATION_MINE | REACTION_FRIENDLY
    );
    assert_eq!(
        flags(PET, None, 4),
        TYPE_PET | CONTROL_PLAYER | AFFILIATION_MINE | REACTION_FRIENDLY
    );
    assert_eq!(
        flags(MOB, None, 1),
        TYPE_NPC | CONTROL_NPC | AFFILIATION_OUTSIDER | REACTION_HOSTILE | TARGET
    );
    assert_eq!(
        flags(MOB, None, 3),
        TYPE_NPC | CONTROL_NPC | AFFILIATION_OUTSIDER | REACTION_NEUTRAL | TARGET
    );
    assert_eq!(
        flags(CHEST, None, 4),
        TYPE_OBJECT | CONTROL_NPC | AFFILIATION_OUTSIDER | REACTION_NEUTRAL
    );
}

/// Group affiliation and roles: a party mate is PARTY with its main-tank flag; in a raid a
/// non-subgroup member reads RAID; our own row's flags give main assist; raid icons mark.
#[test]
fn group_affiliation_roles_and_raid_icons() {
    let party = group(false);
    assert_eq!(
        flags(MATE, Some(&party), 4),
        TYPE_PLAYER | CONTROL_PLAYER | AFFILIATION_PARTY | REACTION_FRIENDLY | MAINTANK
    );
    assert_eq!(
        flags(ME, Some(&party), 4),
        TYPE_PLAYER | CONTROL_PLAYER | AFFILIATION_MINE | REACTION_FRIENDLY | MAINASSIST
    );
    let mut raid = group(true);
    raid.wrath_roster.as_mut().unwrap().members[0].subgroup = 3;
    raid.apply_raid_target(7, MOB);
    assert_eq!(
        flags(MATE, Some(&raid), 4) & (AFFILIATION_PARTY | AFFILIATION_RAID),
        AFFILIATION_RAID
    );
    assert_eq!(flags(MOB, Some(&raid), 1) & 0xff0_0000, RAIDTARGET1 << 7);
}
