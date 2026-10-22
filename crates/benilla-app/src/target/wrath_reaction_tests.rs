use super::*;
use crate::target::wrath_relations::{attack, ByGuid, NoStores, StoreLookup};
use benilla_formats::vehicles::{Vehicle, VehicleCatalog};
use benilla_formats::{FactionInfo, FactionTemplate};
use benilla_protocol::types::ObjectFields;
use std::collections::HashMap;

const ME: u64 = 0x10;
const MATE: u64 = 0x11;
const HUMAN: u32 = 1;
const GUARD: u32 = 11;
const CONTESTED: u32 = 12;
const DEFAULT_HOSTILE: u32 = 13;
const STORMWIND: u32 = 72;

/// Human player 1 (masks 3/2/12), Stormwind guard 11 and contested guard 12 (faction 72, slot 0),
/// and 13, hostile by default with no reputation.
fn catalog() -> FactionCatalog {
    let tpl = |faction, flags, group_mask, friend_group_mask, enemy_group_mask| FactionTemplate {
        faction,
        flags,
        group_mask,
        friend_group_mask,
        enemy_group_mask,
        enemies: [0; 4],
        friends: [0; 4],
    };
    let mut catalog = FactionCatalog::from_rows(
        HashMap::from([
            (HUMAN, tpl(1, 0, 3, 2, 12)),
            (GUARD, tpl(STORMWIND, 0, 2, 2, 4)),
            (
                CONTESTED,
                tpl(STORMWIND, template_flags::CONTESTED_GUARD, 2, 2, 4),
            ),
            (
                DEFAULT_HOSTILE,
                tpl(99, template_flags::HOSTILE_BY_DEFAULT, 0, 0, 0),
            ),
        ]),
        HashMap::new(),
    );
    catalog.insert_faction(STORMWIND, FactionInfo::for_tests(0, 0, [0.0; 2], [0; 2]));
    catalog
}

fn unit(
    guid: u64,
    player: bool,
    template: u32,
    unit_flags: u32,
    extra: &[(Field, u32)],
) -> ObjectStore {
    let at = |field: Field| field.span().unwrap().index;
    let mut pairs = vec![
        (at(Field::ObjectGuid), guid as u32),
        (at(Field::ObjectGuid) + 1, (guid >> 32) as u32),
        (at(Field::ObjectType), if player { 0x19 } else { 9 }),
        (at(Field::UnitFlags), unit_flags),
        (at(Field::UnitFactionTemplate), template),
        (at(Field::UnitBytes0), 1 | (1 << 8)),
    ];
    pairs.extend(extra.iter().map(|&(field, value)| (at(field), value)));
    ObjectStore(ObjectFields::from_pairs(&pairs))
}

fn player(guid: u64, extra: &[(Field, u32)]) -> ObjectStore {
    unit(guid, true, HUMAN, PC, extra)
}

fn ctx<'a>(stores: &'a dyn StoreLookup, world: ReactionState<'a>) -> Context<'a> {
    Context {
        names: None,
        stores,
        world,
    }
}

fn rank(
    catalog: &FactionCatalog,
    reps: &Reputations,
    ctx: Context,
    me: &ObjectStore,
    from: &ObjectStore,
    to: &ObjectStore,
) -> u8 {
    let rx = Reactions {
        factions: Some(catalog),
        reputations: reps,
        ctx,
        me: Some(me),
    };
    unit_reaction(&rx, from, to)
}

#[test]
fn the_player_answers_by_war_and_the_npc_by_its_uncapped_standing() {
    let (catalog, me, guard) = (catalog(), player(ME, &[]), unit(0x20, false, GUARD, 0, &[]));
    let none = Context::none();
    for (flags, standing, toward, back) in [
        (0, 42_000, 4, 7),
        (faction_flags::AT_WAR, 42_000, 1, 7),
        (0, -7_000, 4, 0),
        (0, 21_000, 4, 6),
    ] {
        let reps = Reputations(vec![(flags, standing)]);
        assert_eq!(rank(&catalog, &reps, none, &me, &me, &guard), toward);
        assert_eq!(
            rank(&catalog, &reps, none, &me, &guard, &me),
            back,
            "{standing}"
        );
    }
    assert_eq!(
        rank(&catalog, &Reputations::default(), none, &me, &me, &me),
        4
    );
    let rx = Reactions {
        factions: None,
        reputations: &Reputations::default(),
        ctx: none,
        me: Some(&me),
    };
    assert_eq!(
        unit_reaction(&rx, &guard, &me),
        3,
        "a missing row reads neutral"
    );
}

#[test]
fn a_forced_reaction_wins_and_ignoring_reputation_reads_the_templates() {
    let (catalog, guard) = (catalog(), unit(0x20, false, GUARD, 0, &[]));
    let at_war = Reputations(vec![(faction_flags::AT_WAR, 42_000)]);
    let forced = ForcedReactions(vec![(STORMWIND, 2), (STORMWIND, 6)]);
    let world = ReactionState {
        forced: Some(&forced),
        ..ReactionState::default()
    };
    let me = player(ME, &[]);
    assert_eq!(
        rank(&catalog, &at_war, ctx(&NoStores, world), &me, &me, &guard),
        2
    );
    assert_eq!(
        rank(&catalog, &at_war, ctx(&NoStores, world), &me, &guard, &me),
        2
    );
    let me = player(ME, &[(Field::UnitFlags2, IGNORE_REPUTATION)]);
    let none = Context::none();
    assert_eq!(
        rank(&catalog, &at_war, none, &me, &me, &guard),
        4,
        "no war leg: friend masks"
    );
    assert_eq!(
        rank(&catalog, &at_war, none, &me, &guard, &me),
        4,
        "no standing leg"
    );
}

#[test]
fn a_contested_guard_is_hostile_to_a_contested_player_both_ways() {
    let (catalog, reps) = (catalog(), Reputations(vec![(0, 0)]));
    let guard = unit(0x20, false, CONTESTED, 0, &[]);
    let none = Context::none();
    let flagged = player(ME, &[(Field::PlayerFlags, PLAYER_FLAGS_CONTESTED_PVP)]);
    assert_eq!(rank(&catalog, &reps, none, &flagged, &flagged, &guard), 1);
    assert_eq!(rank(&catalog, &reps, none, &flagged, &guard, &flagged), 1);
    let me = player(ME, &[]);
    assert_eq!(rank(&catalog, &reps, none, &me, &me, &guard), 4);
    assert_eq!(rank(&catalog, &reps, none, &me, &guard, &me), 3);
}

#[test]
fn a_group_member_skips_ffa_and_a_pet_answers_for_its_owner() {
    let catalog = catalog();
    let ffa = [(Field::UnitBytes2, u32::from(PVP_FFA) << 8)];
    let (me, mate) = (player(ME, &ffa), player(MATE, &ffa));
    let reps = Reputations(vec![(faction_flags::AT_WAR, 0)]);
    assert_eq!(
        rank(&catalog, &reps, Context::none(), &me, &me, &mate),
        1,
        "mutual FFA"
    );
    let roster = GroupRoster {
        party: vec![MATE],
        raid: vec![],
    };
    let world = ReactionState {
        roster: Some(&roster),
        ..ReactionState::default()
    };
    assert_eq!(
        rank(&catalog, &reps, ctx(&NoStores, world), &me, &me, &mate),
        4
    );
    let raid = GroupRoster {
        party: vec![],
        raid: vec![ME, MATE],
    };
    let world = ReactionState {
        roster: Some(&raid),
        ..ReactionState::default()
    };
    assert_eq!(
        rank(&catalog, &reps, ctx(&NoStores, world), &me, &mate, &me),
        4
    );

    let me = player(ME, &[]);
    let pet = unit(0x30, false, HUMAN, PC, &[(Field::UnitCreatedby, ME as u32)]);
    let guard = unit(0x20, false, GUARD, 0, &[]);
    let lookup = ByGuid::new(|guid| (guid == ME).then_some(&me));
    let world = ReactionState::default();
    assert_eq!(
        rank(&catalog, &reps, ctx(&lookup, world), &me, &pet, &guard),
        1,
        "the owner's war"
    );
    assert_eq!(
        rank(&catalog, &reps, Context::none(), &me, &pet, &guard),
        4,
        "an unresolved owner"
    );
}

#[test]
fn hostile_by_default_templates_answer_hostile_toward_the_player() {
    let (catalog, me) = (catalog(), player(ME, &[]));
    let wild = unit(0x40, false, DEFAULT_HOSTILE, 0, &[]);
    let reps = Reputations::default();
    assert_eq!(rank(&catalog, &reps, Context::none(), &me, &wild, &me), 1);
    assert_eq!(
        rank(&catalog, &reps, Context::none(), &me, &me, &wild),
        3,
        "masks only one way"
    );
}

#[test]
fn attack_refuses_a_free_standing_guarded_vehicle_under_flags_2_bit_16() {
    const KIT: u64 = 0x50;
    let me = player(ME, &[]);
    let row = |flags| Vehicle {
        id: 7,
        flags,
        turn_speed: 0.0,
        pitch_speed: 0.0,
        pitch_min: 0.0,
        pitch_max: 0.0,
        seats: [0; 8],
        ui_locomotion_type: 0,
        ui_seat_indicator_type: 0,
        power_display_id: 0,
    };
    let armed = VehicleSeats(VehicleCatalog::from_rows([row(VEHICLE_FLAG_GUARD)], []));
    let plain = VehicleSeats(VehicleCatalog::from_rows([row(0)], []));
    let mut vehicles = VehicleWorld {
        kits: HashMap::from([(KIT, 7)]),
        ..VehicleWorld::default()
    };
    let flagged = unit(KIT, false, GUARD, 0, &[(Field::UnitFlags2, 0x1_0000)]);
    let bare = unit(KIT, false, GUARD, 0, &[]);
    let world = |seats| ReactionState {
        vehicles: Some(&vehicles),
        seats: Some(seats),
        ..ReactionState::default()
    };
    assert!(!attack(&me, &flagged, 1, 1, ctx(&NoStores, world(&armed))));
    assert!(
        attack(&me, &bare, 1, 1, ctx(&NoStores, world(&armed))),
        "bit 16 arms the leg"
    );
    assert!(attack(&me, &flagged, 1, 1, ctx(&NoStores, world(&plain))));
    assert!(
        attack(&me, &flagged, 1, 1, Context::none()),
        "no vehicle tables"
    );
    vehicles.riders.insert(KIT, (0x60, 0));
    let world = ReactionState {
        vehicles: Some(&vehicles),
        seats: Some(&armed),
        ..ReactionState::default()
    };
    assert!(
        attack(&me, &flagged, 1, 1, ctx(&NoStores, world)),
        "a passenger vehicle"
    );
}
