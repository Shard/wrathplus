use super::*;
use std::time::Instant;
use wrathplus_protocol::types::ObjectFields;
use wrathplus_protocol::wrath_battleground::{Status, StatusTail};

const ME: u64 = 0x10;
const FOE1: u64 = 0x21;
const FOE2: u64 = 0x22;
const ALLY: u64 = 0x23;

/// A player of `arena_faction` (`PLAYER_BYTES_3` byte 3) at 900 of 1000 health.
fn player(arena_faction: u8) -> ObjectStore {
    ObjectStore(ObjectFields::from_pairs(&[
        (Field::ObjectType.index(), 0x19),
        (Field::UnitHealth.index(), 900),
        (Field::UnitMaxHealth.index(), 1000),
        (Field::UnitLevel.index(), 80),
        (Field::PlayerBytes3.index(), u32::from(arena_faction) << 24),
    ]))
}

/// `SMSG_BATTLEFIELD_STATUS` status 3 for an arena map, our arena faction `ours`; `None` clears.
fn status(world: &mut World, ours: Option<u8>) {
    let status = ours.map(|arena_faction| Status {
        arena_type: 2,
        raw_arena_flag: 0x0E,
        bg_type: 4,
        raw_word: 0,
        min_level: 80,
        max_level: 80,
        client_instance: 1,
        rated: 0,
        status: 3,
        tail: StatusTail::InProgress {
            map: 559,
            raw_unknown: 0,
            auto_leave_ms: 0,
            elapsed_ms: 0,
            arena_faction,
        },
    });
    world
        .resource_mut::<BattlefieldQueue>()
        .apply(0, status, |_| ARENA, Instant::now());
}

fn spawn(world: &mut World, guid: u64, store: ObjectStore) -> Entity {
    let e = world.spawn((Guid(guid), store)).id();
    world.resource_mut::<GuidIndex>().0.insert(guid, e);
    e
}

fn despawn(world: &mut World, guid: u64) {
    let e = world.resource_mut::<GuidIndex>().0.remove(&guid).unwrap();
    world.despawn(e);
}

/// The feed over a minimal world, us at faction 0, with a Lua log of the events.
fn world() -> (World, bevy::ecs::system::SystemId) {
    let (tx, _rx) = crossbeam_channel::unbounded();
    let mut world = World::new();
    world.insert_resource(NetCommands(tx));
    world.init_resource::<GuidIndex>();
    world.init_resource::<NameCache>();
    world.init_resource::<Reputations>();
    world.init_resource::<crate::app_core::group::GroupState>();
    world.init_resource::<ArenaOpponents>();
    world.init_resource::<ArenaFeed>();
    world.init_resource::<NativeBattlegroundRest>();
    world.init_resource::<BattlefieldQueue>();
    {
        let mut names = world.resource_mut::<NameCache>();
        for (guid, name) in [(FOE1, "Rogue"), (FOE2, "Mage"), (ALLY, "Friend")] {
            names.insert_player(guid, name.to_string(), None);
        }
    }
    let script = UiScript::new().unwrap();
    script
        .run(
            "SEEN = {} local f = CreateFrame('Frame') f:RegisterEvent('ARENA_OPPONENT_UPDATE') \
             f:SetScript('OnEvent', function(_, _, u, k) SEEN[#SEEN + 1] = u .. ':' .. k end)",
        )
        .unwrap();
    world.insert_non_send_resource(script);
    let me = spawn(&mut world, ME, player(0));
    world.entity_mut(me).insert(SelfPlayer);
    let id = world.register_system(feed_arena);
    (world, id)
}

/// Run the feed once and take the events it fired.
fn run(world: &mut World, id: bevy::ecs::system::SystemId) -> Vec<String> {
    world.run_system(id).unwrap();
    let script = world.non_send_resource::<UiScript>();
    let seen: Vec<String> = script.eval("return SEEN").unwrap();
    script.run("SEEN = {}").unwrap();
    seen
}

fn script_of(world: &World) -> &UiScript {
    world.non_send_resource::<UiScript>()
}

/// The traced lifecycle: other-faction players take slots in sighting order; leaving visibility
/// is `unseen`, the slot and its name kept; a return is `seen` in the same slot; 0x4C7 is
/// `destroyed` with no `unseen` after it; the battleground's end clears all ten tokens.
#[test]
fn opponents_fill_in_sighting_order_and_follow_the_traced_events() {
    let (mut world, id) = world();
    status(&mut world, Some(0));
    spawn(&mut world, FOE1, player(1));
    spawn(&mut world, ALLY, player(0));
    spawn(&mut world, FOE2, player(1));
    assert_eq!(run(&mut world, id), ["arena1:seen", "arena2:seen"]);
    assert_eq!(
        world.resource::<ArenaOpponents>().arena,
        [FOE1, FOE2, 0, 0, 0]
    );
    assert_eq!(
        script_of(&world)
            .eval::<i64>("return GetNumArenaOpponents()")
            .unwrap(),
        2
    );
    assert_eq!(
        script_of(&world)
            .eval::<(bool, String, i64)>(
                "return UnitExists('arena2'), UnitName('arena2'), UnitHealth('arena2')"
            )
            .unwrap(),
        (true, "Mage".to_string(), 900)
    );
    assert!(
        run(&mut world, id).is_empty(),
        "a quiet frame fires nothing"
    );

    despawn(&mut world, FOE1);
    assert_eq!(run(&mut world, id), ["arena1:unseen"]);
    assert_eq!(
        script_of(&world)
            .eval::<(bool, String)>("return UnitExists('arena1'), UnitName('arena1')")
            .unwrap(),
        (false, "Rogue".to_string()),
        "the slot's cache answers while the unit is gone"
    );
    spawn(&mut world, FOE1, player(1));
    assert_eq!(run(&mut world, id), ["arena1:seen"]);
    assert_eq!(
        world.resource::<ArenaOpponents>().arena,
        [FOE1, FOE2, 0, 0, 0]
    );

    world
        .resource_mut::<NativeBattlegroundRest>()
        .0
        .push_back(Packet::ArenaUnitDestroyed { guid: FOE2 });
    despawn(&mut world, FOE2);
    assert_eq!(run(&mut world, id), ["arena2:destroyed"]);
    assert!(world.resource::<NativeBattlegroundRest>().0.is_empty());

    status(&mut world, None);
    let cleared = run(&mut world, id);
    assert_eq!(cleared.len(), 10);
    assert_eq!(cleared[..2], ["arena1:cleared", "arenapet1:cleared"]);
    assert_eq!(
        script_of(&world)
            .eval::<i64>("return GetNumArenaOpponents()")
            .unwrap(),
        0
    );
    assert!(!script_of(&world)
        .eval::<bool>("return UnitExists('arena1')")
        .unwrap());
}

/// Outside an arena nobody is an opponent, and a stray 0x4C7 is consumed and ignored.
#[test]
fn outside_an_arena_nothing_is_assigned() {
    let (mut world, id) = world();
    spawn(&mut world, FOE1, player(1));
    world
        .resource_mut::<NativeBattlegroundRest>()
        .0
        .push_back(Packet::ArenaUnitDestroyed { guid: FOE1 });
    assert!(run(&mut world, id).is_empty());
    assert_eq!(world.resource::<ArenaOpponents>().count(), 0);
    assert!(world.resource::<NativeBattlegroundRest>().0.is_empty());
}

/// The stock `Blizzard_ArenaUI` frames: `"seen"` shows the slot's frame with the opponent's name,
/// `"unseen"` greys its health bar, `"cleared"` hides it.
#[test]
fn the_stock_arena_frames_follow_the_opponent_events() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    crate::ui_script::test_ui::load_ui(&mut s, r"Interface\FrameXML\UIParent.xml");
    crate::ui_script::test_ui::seat_chain_addon(&mut s, "Blizzard_ArenaUI");
    s.run(r#"UIParentLoadAddOn("Blizzard_ArenaUI")"#).unwrap();
    s.set_instance_type(Some(ARENA));
    s.fire_event("PLAYER_ENTERING_WORLD", vec![]);
    let _ = s.errors();

    s.set_arena_units([FOE1, 0, 0, 0, 0], [0; 5]);
    s.set_unit(
        "arena1",
        Some(UnitState {
            exists: true,
            has_object: true,
            guid: FOE1,
            name: Some("Rogue".into()),
            health: 900,
            max_health: 1000,
            level: 80,
            is_player: true,
            ..Default::default()
        }),
    );
    let update = |s: &mut UiScript, kind: &str| {
        s.fire_event(
            "ARENA_OPPONENT_UPDATE",
            vec![
                ScriptValue::Str("arena1".into()),
                ScriptValue::Str(kind.into()),
            ],
        );
    };
    update(&mut s, "seen");
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert_eq!(
        s.eval::<(bool, bool, String)>(
            "return ArenaEnemyFrames:IsShown(), ArenaEnemyFrame1:IsShown(), ArenaEnemyFrame1.name:GetText()"
        )
        .unwrap(),
        (true, true, "Rogue".to_string())
    );

    update(&mut s, "unseen");
    assert!(
        s.eval::<bool>("return ArenaEnemyFrame1HealthBar.lockColor == true")
            .unwrap(),
        "an unseen opponent's bar is locked grey (`ArenaEnemyFrame_Lock`)"
    );

    update(&mut s, "cleared");
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert!(!s.eval::<bool>("return ArenaEnemyFrame1:IsShown()").unwrap());
}
