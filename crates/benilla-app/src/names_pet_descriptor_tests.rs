//! A pet's name by its descriptor: the companion, permanent-pet and charmed legs of `0x609210`.

use super::*;
use benilla_protocol::types::Field;

/// `OBJECT_FIELD_ENTRY` and `UNIT_FIELD_PETNUMBER`, absolute descriptor indices.
const ENTRY: u16 = Field::ObjectEntry.index();
const PETNUMBER: u16 = Field::UnitPetnumber.index();

fn unit(fields: &[(u16, u32)]) -> ObjectStore {
    ObjectStore(benilla_protocol::ObjectFields::from_pairs(fields))
}

fn record(name: &str) -> CreatureRecord {
    CreatureRecord {
        name: name.into(),
        subname: None,
        creature_type: 0,
        pet_family: 0,
        rank: 0,
        type_flags: 0,
        civilian: false,
        racial_leader: false,
        display_id: 0,
        modifiers: None,
    }
}

/// `0x609210`'s `UNIT_FIELD_PETNUMBER == 0` leg: a vmangos companion pet's guid holds a pet
/// number the server never answers for (`PetHandler.cpp:190-192`).
#[test]
fn a_companion_pet_is_named_by_its_descriptor_entry() {
    let (cmds, rx) = commands();
    let mut cache = NameCache::default();
    let squirrel = compose(guid::HIGH_PET, 5_021, 3);
    let store = unit(&[(ENTRY, 2_671), (PETNUMBER, 0)]);
    cache.insert_creature(2_671, Some(record("Mechanical Squirrel")));

    assert_eq!(
        cache.resolve_unit(squirrel, Some(&store), &cmds),
        Some("Mechanical Squirrel")
    );
    assert_eq!(
        cache.peek_unit(squirrel, Some(&store)),
        Some("Mechanical Squirrel")
    );
    assert!(
        matches!(rx.try_recv(), Err(TryRecvError::Empty)),
        "no pet-name query for a unit whose PETNUMBER is 0"
    );
}

#[test]
fn a_cold_companion_pet_asks_the_creature_query_for_its_entry() {
    let (cmds, rx) = commands();
    let cache = NameCache::default();
    let squirrel = compose(guid::HIGH_PET, 5_021, 3);
    let store = unit(&[(ENTRY, 2_671)]);

    assert_eq!(cache.resolve_unit(squirrel, Some(&store), &cmds), None);
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientCommand::CreatureQuery { entry: 2_671, guid }) if guid == squirrel
    ));
    assert_eq!(cache.resolve_unit(squirrel, Some(&store), &cmds), None);
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
fn a_permanent_pet_asks_the_pet_query_for_its_descriptor_number() {
    let (cmds, rx) = commands();
    let mut cache = NameCache::default();
    let rex = compose(guid::HIGH_PET, 7, 42);
    let store = unit(&[(ENTRY, 3_122), (PETNUMBER, 7)]);
    cache.insert_creature(3_122, Some(record("Bloodtalon Taillasher")));

    assert_eq!(cache.resolve_unit(rex, Some(&store), &cmds), None);
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientCommand::PetNameQuery { pet_number: 7, guid }) if guid == rex
    ));
    cache.insert_pet(7, "Rex".into());
    assert_eq!(cache.resolve_unit(rex, Some(&store), &cmds), Some("Rex"));
    assert_eq!(cache.peek_unit(rex, Some(&store)), Some("Rex"));
}

/// A charmed creature gets a pet number (`SpellAuras.cpp:3248`) and is named under it.
#[test]
fn a_charmed_creature_is_named_by_its_pet_number() {
    let (cmds, rx) = commands();
    let cache = NameCache::default();
    let ogre = compose(guid::HIGH_UNIT, 1_000, 9);
    let store = unit(&[(ENTRY, 1_000), (PETNUMBER, 55)]);

    assert_eq!(cache.resolve_unit(ogre, Some(&store), &cmds), None);
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientCommand::PetNameQuery { pet_number: 55, .. })
    ));
}
