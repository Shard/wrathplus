use super::*;

const ME: u64 = 0x10;
const PET: u64 = 0xF140_0000_0000_0077;
const MOB: u64 = 0xF130_0000_0000_0001;
const P1: u64 = 0x21;
const P1_PET: u64 = 0xF140_0000_0000_0021;
const R3: u64 = 0x33;
const FAR: u64 = 0x44;

/// Us targeting a mob that targets party1, who targets us; party1's pet and raid row 3 held,
/// `FAR` a roster member out of range.
fn guids() -> UnitGuids {
    UnitGuids {
        player: ME,
        pet: PET,
        target: MOB,
        mouseover: MOB,
        npc: MOB,
        party: [P1, FAR, 0, 0],
        party_pets: [P1_PET, 0, 0, 0],
        raid: vec![ME, P1, R3],
        raid_pets: vec![PET, P1_PET, 0],
        held: HashMap::from([
            (ME, MOB),
            (PET, 0),
            (MOB, P1),
            (P1, ME),
            (P1_PET, MOB),
            (R3, 0),
        ]),
        ..Default::default()
    }
}

fn at(token: &str) -> Option<u64> {
    guids().resolve(token).expect("recognised")
}

#[test]
fn every_base_resolves_through_its_table() {
    assert_eq!(at("player"), Some(ME));
    assert_eq!(at("pet"), Some(PET));
    assert_eq!(at("target"), Some(MOB));
    assert_eq!(at("mouseover"), Some(MOB));
    assert_eq!(at("npc"), Some(MOB));
    assert_eq!(at("party1"), Some(P1));
    // Not held, but a roster member: the base needs no object.
    assert_eq!(at("party2"), Some(FAR));
    assert_eq!(at("partypet1"), Some(P1_PET));
    assert_eq!(at("raid3"), Some(R3));
    assert_eq!(at("raidpet2"), Some(P1_PET));
}

#[test]
fn the_compares_fold_ascii_case() {
    assert_eq!(at("PLAYER"), Some(ME));
    assert_eq!(at("Player"), Some(ME));
    assert_eq!(at("PartyPet1"), Some(P1_PET));
    assert_eq!(at("MouseOver"), Some(MOB));
    assert_eq!(at("NPC"), Some(MOB));
    assert_eq!(at("TargetTarget"), Some(P1));
}

#[test]
fn target_is_a_loop_on_every_base_but_npc() {
    assert_eq!(at("targettarget"), Some(P1));
    assert_eq!(at("targettargettarget"), Some(ME));
    assert_eq!(at("targettargettargettarget"), Some(MOB));
    assert_eq!(at("playertarget"), Some(MOB));
    assert_eq!(at("party1target"), Some(ME));
    assert_eq!(at("partypet1target"), Some(MOB));
    assert_eq!(at("raid2target"), Some(ME));
    assert_eq!(at("raid2targettarget"), Some(MOB));
    assert_eq!(at("mouseovertarget"), Some(P1));
    // A hop to nobody ends the chain (`0x515a2e`), as does a hop off a unit not held.
    assert_eq!(at("pettarget"), None);
    assert_eq!(at("party2target"), None);
    assert!(guids().resolve("npctarget").is_err());
}

/// The parse reads the text alone: the base by the resolver's compares, a numbered base's
/// 0-based row, then `target` hops to the end, whatever the depth.
#[test]
fn a_token_parses_to_a_base_and_its_hops() {
    use UnitBase::*;
    let unit = |base, hops| UnitTokenParse::Unit { base, hops };
    for (token, want) in [
        ("player", unit(Player, 0)),
        ("pet", unit(Pet, 0)),
        ("target", unit(Target, 0)),
        ("mouseover", unit(Mouseover, 0)),
        ("npc", unit(Npc, 0)),
        ("party1", unit(Party(0), 0)),
        ("partypet4", unit(PartyPet(3), 0)),
        ("raid40", unit(Raid(39), 0)),
        ("raidpet2", unit(RaidPet(1), 0)),
        ("targettarget", unit(Target, 1)),
        ("targettargettargettarget", unit(Target, 3)),
        ("playertarget", unit(Player, 1)),
        ("pettarget", unit(Pet, 1)),
        ("mouseovertarget", unit(Mouseover, 1)),
        ("party1target", unit(Party(0), 1)),
        ("partypet1target", unit(PartyPet(0), 1)),
        ("raid12targettargettarget", unit(Raid(11), 3)),
        ("raidpet3target", unit(RaidPet(2), 1)),
        // A missing or zero number is the row every bound rejects.
        ("party", unit(Party(u32::MAX), 0)),
        ("raid0target", unit(Raid(u32::MAX), 1)),
        // The compares fold case, the hops too.
        ("PARTY1TARGET", unit(Party(0), 1)),
        ("PartyPet2TargetTarget", unit(PartyPet(1), 2)),
    ] {
        assert_eq!(parse_unit_token(token), want, "{token}");
    }
    let deep = format!("player{}", "target".repeat(1000));
    assert_eq!(
        parse_unit_token(&deep),
        unit(Player, 1000),
        "no depth limit"
    );
}

/// Text after the base, or after any hop, that is not `target` names nobody; a token none of
/// the nine compares match is the raise, `npc` being an exact compare with no chain.
#[test]
fn a_token_that_is_not_base_then_hops_parses_to_nobody_or_the_raise() {
    for token in [
        "",
        "party1foo",
        "playerfoo",
        "player1",
        "pet1",
        "party1targetfoo",
        "party1targettarge",
        "party1 target",
        "targetfoo",
        "raid3x",
    ] {
        assert_eq!(parse_unit_token(token), UnitTokenParse::Nobody, "{token}");
    }
    for token in ["npctarget", "npc1", "bogus", "tar", "focus", "5", "targe"] {
        assert_eq!(parse_unit_token(token), UnitTokenParse::Unknown, "{token}");
    }
}

#[test]
fn a_recognised_token_naming_nobody_is_quiet() {
    for t in [
        "",
        "party",
        "party0",
        "party5",
        "partyX",
        "raid0",
        "raid4",
        "raidpet3",
        "playerfoo",
        "targetfoo",
        "partypet5",
    ] {
        assert_eq!(guids().resolve(t), Ok(None), "{t}");
    }
    let solo = UnitGuids {
        player: 0,
        pet: PET,
        ..guids()
    };
    assert_eq!(solo.resolve("pet"), Ok(None), "no player object, no pet");
    let gone = UnitGuids {
        held: HashMap::new(),
        ..guids()
    };
    assert_eq!(gone.resolve("mouseover"), Ok(None), "a mouseover not held");
    assert_eq!(
        gone.resolve("target"),
        Ok(Some(MOB)),
        "the target global is not gated"
    );
}

#[test]
fn an_unrecognised_token_is_the_raise() {
    for t in ["bogus", "focus", "none", "5", "npc1", "tar"] {
        assert_eq!(guids().resolve(t), Err(()), "{t}");
    }
}

#[test]
fn the_index_parse_wraps_and_stops_at_the_first_non_digit() {
    assert_eq!(at("party01"), Some(P1));
    assert_eq!(at("raid03"), Some(R3));
    assert_eq!(at("raid03target"), None, "raid3 targets nobody");
    assert_eq!(
        index_of(b"4294967297"),
        (0, 10),
        "2^32 + 1 wraps to 1, index 0"
    );
    assert_eq!(index_of(b"x"), (u32::MAX, 0));
}

/// The unit-event fan-out order of `0x60bb70`, pinned: player, pet, the first party slot or
/// its pet, the first raid row or its pet, target, focus, mouseover.
#[test]
fn event_tokens_follow_the_native_fan_out_order() {
    const G: u64 = 7;
    let all = UnitGuids {
        player: G,
        pet: G,
        target: G,
        mouseover: G,
        focus: G,
        party: [0, G, G, 0],
        party_pets: [G, 0, 0, 0],
        raid: vec![0, 0, G],
        raid_pets: vec![0, G],
        ..Default::default()
    };
    assert_eq!(
        all.event_tokens(G),
        [
            "player",
            "pet",
            "partypet1",
            "raidpet2",
            "target",
            "focus",
            "mouseover"
        ]
    );
    assert!(all.event_tokens(0).is_empty());
    assert!(all.event_tokens(8).is_empty());
    let raid = UnitGuids {
        raid: vec![0, G],
        party: [0, 0, 0, G],
        ..Default::default()
    };
    assert_eq!(raid.event_tokens(G), ["party4", "raid2"]);
}

/// `arenaN`/`arenapetN` read the arena feed's slots; `GetNumArenaOpponents` counts the slots before
/// the first empty one (`0x549630`), and the event fan-out names them after the raid rows
/// (`0x60bb70`).
#[test]
fn arena_slots_resolve_count_and_fan_out_after_the_raid() {
    const A1: u64 = 0x51;
    const A2_PET: u64 = 0xF140_0000_0000_0052;
    const A4: u64 = 0x54;
    let g = UnitGuids {
        wrath: true,
        raid: vec![A1],
        arena: [A1, 0, 0, A4, 0],
        arena_pets: [0, A2_PET, 0, 0, 0],
        ..Default::default()
    };
    assert_eq!(g.resolve("arena1"), Ok(Some(A1)));
    assert_eq!(g.resolve("arenapet2"), Ok(Some(A2_PET)));
    assert_eq!(g.resolve("arena2"), Ok(None));
    assert_eq!(
        g.arena_count(),
        2,
        "slot 3 is empty, so slot 4 is not counted"
    );
    assert_eq!(g.event_tokens(A1), ["raid1", "arena1"]);
    assert_eq!(g.event_tokens(A2_PET), ["arenapet2"]);
    assert!(g.event_tokens(A4).is_empty(), "past the count");
}

/// The app's unit push leaves the arena feed's slots alone, and `GetNumArenaOpponents` reads them.
#[test]
fn the_unit_push_keeps_the_arena_slots() {
    let mut s = crate::script::UiScript::new().unwrap();
    s.set_arena_units([0x51, 0x52, 0, 0, 0], [0; 5]);
    s.set_unit_guids(&UnitGuids {
        player: ME,
        ..Default::default()
    });
    assert_eq!(s.eval::<i64>("return GetNumArenaOpponents()").unwrap(), 2);
    assert_eq!(s.unit_event_tokens(0x52), ["arena2"]);
}
