use super::*;
use wrathplus_formats::SpellCatalog;
use wrathplus_ui::script::UnitGuids;
use crossbeam_channel::Receiver;

const ME: u64 = 0x10;
const MOB: u64 = 0x20;
const FIREBALL: u32 = 133;
const HIDDEN: u32 = 999;
const DRAIN: u32 = 689;

fn spells() -> Spells {
    let row = |id, name: &str, attributes, attributes_ex3| SpellDisplay {
        id,
        name: name.into(),
        rank: Some("Rank 1".into()),
        icon: Some("Interface\\Icons\\Spell_Fire_FlameBolt".into()),
        attributes,
        attributes_ex3,
        ..Default::default()
    };
    Spells {
        catalog: SpellCatalog::from_displays(HashMap::from([
            (FIREBALL, row(FIREBALL, "Fireball", 0, 0)),
            // `Attributes & 0x80`: shown for the player only.
            (HIDDEN, row(HIDDEN, "Hidden", 0x80, 0)),
            (DRAIN, row(DRAIN, "Drain Life", 0, 0)),
        ])),
        ..Spells::empty_for_tests()
    }
}

fn app() -> (App, Receiver<crate::net::ClientCommand>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    let mut index = GuidIndex::default();
    index.0.insert(ME, app.world_mut().spawn_empty().id());
    index.0.insert(MOB, app.world_mut().spawn_empty().id());
    app.add_plugins(bevy::state::app::StatesPlugin)
        .insert_state(crate::app_core::state::ClientState::InWorld)
        .insert_resource(NetCommands(tx))
        .insert_resource(SelfGuid(Some(ME)))
        .insert_resource(index)
        .insert_resource(spells())
        .init_resource::<crate::spell::AutoRepeatActive>()
        .init_resource::<CastEdges>()
        .init_resource::<UnitCasts>()
        .add_systems(Update, feed);
    let mut script = UiScript::new().unwrap();
    script.set_unit_guids(&UnitGuids {
        player: ME,
        target: MOB,
        focus: MOB,
        ..Default::default()
    });
    script
        .run(
            "SEEN = {} local f = CreateFrame('Frame') f:RegisterAllEvents() \
             f:SetScript('OnEvent', function(_, e, u, n, r, c) \
               if e:find('^UNIT_SPELLCAST') or e == 'STOP_AUTOREPEAT_SPELL' then \
                 table.insert(SEEN, table.concat({e, tostring(u), tostring(n), tostring(r), tostring(c)}, ',')) \
               end end)",
        )
        .unwrap();
    app.insert_non_send_resource(script);
    (app, rx)
}

fn edges(app: &mut App, edges: impl IntoIterator<Item = CastEdge>) {
    app.world_mut().resource_mut::<CastEdges>().0.extend(edges);
    app.update();
}

fn seen(app: &App) -> Vec<String> {
    app.world()
        .non_send_resource::<UiScript>()
        .eval::<Vec<String>>("local out = SEEN SEEN = {} return out")
        .unwrap()
}

/// START for every token naming the caster with name, rank and count; the snapshot is in place
/// for the handler; GO's SUCCEEDED, then STOP for the displayed cast.
#[test]
fn a_cast_starts_for_every_token_then_succeeds_and_stops() {
    let (mut app, _rx) = app();
    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .run(
            "CAST = nil local f = CreateFrame('Frame') f:RegisterEvent('UNIT_SPELLCAST_START') \
             f:SetScript('OnEvent', function(_, _, u) if u == 'target' then CAST = UnitCastingInfo(u) end end)",
        )
        .unwrap();
    edges(
        &mut app,
        [CastEdge::Start {
            caster: MOB,
            spell_id: FIREBALL,
            cast_count: 3,
            cast_ms: 2500,
        }],
    );
    assert_eq!(
        seen(&app),
        [
            "UNIT_SPELLCAST_START,target,Fireball,Rank 1,3",
            "UNIT_SPELLCAST_START,focus,Fireball,Rank 1,3",
        ]
    );
    assert_eq!(
        app.world()
            .non_send_resource::<UiScript>()
            .eval::<Option<String>>("return CAST")
            .unwrap()
            .as_deref(),
        Some("Fireball"),
        "the snapshot is published before the event"
    );
    edges(
        &mut app,
        [CastEdge::Result {
            caster: MOB,
            spell_id: FIREBALL,
            cast_count: 3,
            reason: Some(REASON_SUCCEEDED),
            failure: false,
        }],
    );
    assert_eq!(
        seen(&app),
        [
            "UNIT_SPELLCAST_SUCCEEDED,target,Fireball,Rank 1,3",
            "UNIT_SPELLCAST_SUCCEEDED,focus,Fireball,Rank 1,3",
            "UNIT_SPELLCAST_STOP,target,Fireball,Rank 1,3",
            "UNIT_SPELLCAST_STOP,focus,Fireball,Rank 1,3",
        ]
    );
    assert!(app.world().resource::<UnitCasts>().casts.is_empty());
}

/// The reason ladder; a result that is not the displayed cast fires no STOP; a hidden spell shows
/// only for the player; delays push the slot.
#[test]
fn the_result_ladder_visibility_and_delay() {
    let (mut app, _rx) = app();
    let result = |reason, count| CastEdge::Result {
        caster: MOB,
        spell_id: FIREBALL,
        cast_count: count,
        reason,
        failure: false,
    };
    edges(
        &mut app,
        [CastEdge::Start {
            caster: MOB,
            spell_id: FIREBALL,
            cast_count: 1,
            cast_ms: 1000,
        }],
    );
    let _ = seen(&app);
    edges(
        &mut app,
        [CastEdge::Delayed {
            caster: MOB,
            delay_ms: 500,
        }],
    );
    let slot = app.world().resource::<UnitCasts>().casts[&MOB];
    assert_eq!(
        slot.end_ms - slot.start_ms,
        1000,
        "the pushback moves the whole bar"
    );
    assert_eq!(
        seen(&app)[0],
        "UNIT_SPELLCAST_DELAYED,target,Fireball,Rank 1,1"
    );
    edges(&mut app, [result(Some(0x28), 9)]);
    assert_eq!(
        seen(&app),
        [
            "UNIT_SPELLCAST_INTERRUPTED,target,Fireball,Rank 1,9",
            "UNIT_SPELLCAST_INTERRUPTED,focus,Fireball,Rank 1,9",
        ],
        "another count is not the displayed cast: no STOP"
    );
    for (reason, event) in [
        (Some(0x1b), "UNIT_SPELLCAST_FAILED_QUIET"),
        (Some(0x0c), "UNIT_SPELLCAST_FAILED"),
        (None, "UNIT_SPELLCAST_FAILED"),
    ] {
        edges(&mut app, [result(reason, 9)]);
        assert!(seen(&app)[0].starts_with(event), "{reason:?}");
    }
    // `Attributes & 0x80` hides another unit's cast, never ours.
    edges(
        &mut app,
        [
            CastEdge::Start {
                caster: MOB,
                spell_id: HIDDEN,
                cast_count: 2,
                cast_ms: 1000,
            },
            CastEdge::Start {
                caster: ME,
                spell_id: HIDDEN,
                cast_count: 2,
                cast_ms: 1000,
            },
        ],
    );
    assert_eq!(seen(&app), ["UNIT_SPELLCAST_START,player,Hidden,Rank 1,2"]);
}

/// Channels: START with count 0, UPDATE re-times, 0 is CHANNEL_STOP; a caster leaving the object
/// manager ends its bars.
#[test]
fn channels_update_and_stop_and_a_despawn_ends_the_bars() {
    let (mut app, _rx) = app();
    edges(
        &mut app,
        [
            CastEdge::ChannelStart {
                caster: MOB,
                spell_id: DRAIN,
                duration_ms: 5000,
            },
            CastEdge::ChannelUpdate {
                caster: MOB,
                remaining_ms: 2000,
            },
            CastEdge::ChannelUpdate {
                caster: MOB,
                remaining_ms: 0,
            },
        ],
    );
    let events: Vec<String> = seen(&app)
        .into_iter()
        .filter(|e| e.contains(",target,"))
        .collect();
    assert_eq!(
        events,
        [
            "UNIT_SPELLCAST_CHANNEL_START,target,Drain Life,Rank 1,0",
            "UNIT_SPELLCAST_CHANNEL_UPDATE,target,Drain Life,Rank 1,0",
            "UNIT_SPELLCAST_CHANNEL_STOP,target,Drain Life,Rank 1,0",
        ]
    );
    edges(
        &mut app,
        [CastEdge::Start {
            caster: MOB,
            spell_id: FIREBALL,
            cast_count: 4,
            cast_ms: 3000,
        }],
    );
    let _ = seen(&app);
    app.world_mut().resource_mut::<GuidIndex>().0.remove(&MOB);
    app.update();
    assert_eq!(
        seen(&app),
        [
            "UNIT_SPELLCAST_STOP,target,Fireball,Rank 1,4",
            "UNIT_SPELLCAST_STOP,focus,Fireball,Rank 1,4",
        ]
    );
}

/// SPELL_FAILURE for the player at reason 0x34 stops auto-repeat once, through the one sender.
#[test]
fn a_local_failure_at_0x34_stops_auto_repeat_once() {
    let (mut app, rx) = app();
    app.world_mut()
        .resource_mut::<crate::spell::AutoRepeatActive>()
        .0 = Some(75);
    let failure = CastEdge::Result {
        caster: ME,
        spell_id: FIREBALL,
        cast_count: 1,
        reason: Some(0x34),
        failure: true,
    };
    edges(&mut app, [failure, failure]);
    let cancels = rx
        .try_iter()
        .filter(|c| matches!(c, crate::net::ClientCommand::CancelAutoRepeat))
        .count();
    assert_eq!(cancels, 1, "one 0x26D per edge");
    assert!(app
        .world()
        .resource::<crate::spell::AutoRepeatActive>()
        .0
        .is_none());
}

fn spell(effects: [u32; 3], auras: [u32; 3], misc: [i32; 3], school_mask: u32) -> SpellDisplay {
    SpellDisplay {
        effects,
        effect_apply_aura: auras,
        effect_misc_value: misc,
        school_mask,
        ..Default::default()
    }
}

/// `0x53ca70`: the book's interrupts, silences and non-player-only variants, by school mask.
#[test]
fn the_interrupt_masks_gather_the_books_reach_by_school() {
    let kick = spell([0x44, 0, 0], [0; 3], [0; 3], 0x1);
    let silence = spell([6, 0, 0], [0x1b, 0, 0], [0; 3], 0x20);
    let mut npc_only = spell([0x44, 0, 0], [0; 3], [0; 3], 0x4);
    npc_only.attributes_ex7 = 0x800;
    let masks = InterruptMasks::of([&kick, &silence, &npc_only]);
    assert_eq!(
        masks,
        InterruptMasks {
            interrupt: 0x5,
            interrupt_npc: 0x4,
            silence: 0x20,
            silence_npc: 0,
        }
    );
}

/// `0x7262e0`: a silence-prevented cast is uninterruptible when the caster's immunities strip all
/// the player's reach, and only then.
#[test]
fn a_cast_is_uninterruptible_only_when_immunities_strip_every_interrupt() {
    let mut cast = SpellDisplay {
        prevention_type: 1,
        ..Default::default()
    };
    let kick_only = InterruptMasks {
        interrupt: 0x1,
        ..Default::default()
    };
    let none: [&SpellDisplay; 0] = [];
    assert!(
        !not_interruptible(&cast, false, kick_only, none),
        "no immunity"
    );

    let physical_immune = spell([6, 0, 0], [0x27, 0, 0], [0x1, 0, 0], 0);
    assert!(not_interruptible(
        &cast,
        false,
        kick_only,
        [&physical_immune]
    ));

    // A silence in another school still reaches it.
    let both = InterruptMasks {
        silence: 0x20,
        ..kick_only
    };
    assert!(!not_interruptible(&cast, false, both, [&physical_immune]));
    let silence_immune = spell([6, 0, 0], [0x26, 0, 0], [0x1b, 0, 0], 0);
    assert!(not_interruptible(
        &cast,
        false,
        both,
        [&physical_immune, &silence_immune]
    ));

    // A player with no reach never sees the shield; a player caster drops the non-player masks.
    let npc_only = InterruptMasks {
        interrupt_npc: 0x1,
        ..Default::default()
    };
    assert!(!not_interruptible(
        &cast,
        true,
        npc_only,
        [&physical_immune]
    ));
    assert!(!not_interruptible(
        &cast,
        false,
        InterruptMasks::default(),
        [&physical_immune]
    ));

    // Only silence-prevented casts, and AttributesEx7 0x2000 exempts.
    cast.prevention_type = 0;
    assert!(!not_interruptible(
        &cast,
        false,
        kick_only,
        [&physical_immune]
    ));
    cast.prevention_type = 1;
    cast.attributes_ex7 = 0x2000;
    assert!(!not_interruptible(
        &cast,
        false,
        kick_only,
        [&physical_immune]
    ));
}
