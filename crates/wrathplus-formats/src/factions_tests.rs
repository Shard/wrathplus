use super::*;

/// A template with everything zeroed except what a test sets.
fn tpl(faction: u32) -> FactionTemplate {
    FactionTemplate {
        faction,
        flags: 0,
        group_mask: 0,
        friend_group_mask: 0,
        enemy_group_mask: 0,
        enemies: [0; 4],
        friends: [0; 4],
    }
}

/// Every branch, in the order `00715440` tests them, and enemy-beats-friend precedence.
#[test]
fn comparator_branch_domain() {
    let b = FactionTemplate {
        group_mask: 0b0010,
        ..tpl(50)
    };

    let a = FactionTemplate {
        enemy_group_mask: 0b0010,
        ..tpl(1)
    };
    assert_eq!(a.reaction_toward(&b), Reaction::Hostile);

    // A.enemies[i] == B.faction → hostile, found in the last of the four slots.
    let a = FactionTemplate {
        enemies: [7, 8, 9, 50],
        ..tpl(1)
    };
    assert_eq!(a.reaction_toward(&b), Reaction::Hostile);

    // The enemies list is 0-terminated: a match after a 0 is never reached.
    let a = FactionTemplate {
        enemies: [7, 0, 50, 0],
        ..tpl(1)
    };
    assert_eq!(a.reaction_toward(&b), Reaction::Neutral);

    let a = FactionTemplate {
        friend_group_mask: 0b0010,
        ..tpl(1)
    };
    assert_eq!(a.reaction_toward(&b), Reaction::Friendly);

    let a = FactionTemplate {
        friends: [50, 0, 0, 0],
        ..tpl(1)
    };
    assert_eq!(a.reaction_toward(&b), Reaction::Friendly);

    // Friendship is read both ways: B's friend mask covers A's group.
    let a = FactionTemplate {
        group_mask: 0b1000,
        ..tpl(1)
    };
    let b_friendly = FactionTemplate {
        friend_group_mask: 0b1000,
        ..b
    };
    assert_eq!(a.reaction_toward(&b_friendly), Reaction::Friendly);

    let b_names_a = FactionTemplate {
        friends: [1, 0, 0, 0],
        ..b
    };
    assert_eq!(tpl(1).reaction_toward(&b_names_a), Reaction::Friendly);

    assert_eq!(tpl(1).reaction_toward(&b), Reaction::Neutral);
    let default_hostile = FactionTemplate {
        flags: template_flags::HOSTILE_BY_DEFAULT,
        ..tpl(1)
    };
    assert_eq!(default_hostile.reaction_toward(&b), Reaction::Hostile);
    assert_eq!(
        default_hostile.reaction_toward(&b_names_a),
        Reaction::Friendly
    );

    // Hostility is tested first: an enemy-mask hit beats any friendship.
    let a = FactionTemplate {
        enemy_group_mask: 0b0010,
        friend_group_mask: 0b0010,
        friends: [50, 0, 0, 0],
        ..tpl(1)
    };
    assert_eq!(a.reaction_toward(&b_names_a), Reaction::Hostile);
}

/// Unit templates toward a human player (template 1), the direction `0x7cbaa0` colours by;
/// ids from vmangos `creature_template.faction`, expected colours from a reference capture.
#[test]
fn real_dbc_reactions_match_reference_capture() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_faction_catalog(&mut chain).expect("load faction catalog");
    assert!(
        cat.len() > 200,
        "FactionTemplate loaded ({} rows)",
        cat.len()
    );

    let human_player = cat.template(1).expect("human player template (1)");
    let tpl = |id: u32| {
        cat.template(id)
            .unwrap_or_else(|| panic!("faction template {id}"))
    };
    let reaction = |id: u32| tpl(id).reaction_toward(human_player);
    assert_eq!(
        reaction(12),
        Reaction::Friendly,
        "Marshal Dughan (Stormwind)"
    );
    assert_eq!(reaction(31), Reaction::Neutral, "Chicken (critter)");
    assert_eq!(reaction(17), Reaction::Hostile, "Defias Trapper");
    assert_eq!(
        reaction(26),
        Reaction::Hostile,
        "Kobold Miner (aggro mine kobold)"
    );
    assert_eq!(
        reaction(25),
        Reaction::Neutral,
        "Kobold Vermin (non-aggro starter kobold)"
    );
    // The player is hostile toward the vermin (attackable) while the vermin is neutral to it.
    assert_eq!(human_player.reaction_toward(tpl(25)), Reaction::Hostile);

    // Stormwind (faction 72) has slot 19 and a human base of 4000, friendly before any
    // template comparison; the Chicken's faction 28 has no slot.
    let stormwind = cat
        .reputation_faction(72)
        .expect("Stormwind (72) has reputation");
    assert_eq!(stormwind.rep_index, 19);
    assert_eq!(stormwind.base_for(1, 1), 4000, "human base with Stormwind");
    assert_eq!(reputation_rank(stormwind.base_for(1, 1)), 4, "friendly");
    assert!(cat.reputation_faction(28).is_none(), "critter: no rep");
    // Orc (race 2) base with Stormwind is hated (−42000): rank 0, hostile.
    assert_eq!(stormwind.base_for(2, 1), -42000);
    assert_eq!(reputation_rank(stormwind.base_for(2, 1)), 0);

    // The unit tooltip (`0x529fe0`) shows "Stormwind" under a Stormwind guard to a human.
    assert_eq!(cat.faction_name(72), Some("Stormwind"));
    assert!(
        stormwind.tooltip_shows_for(1, 1),
        "Stormwind's matched slot is not rep-hidden for a human"
    );
    // Players never get the line: player factions have no reputation slot.
    assert!(
        cat.reputation_faction(tpl(1).faction).is_none(),
        "the human player faction has no reputation slot"
    );
}

#[test]
fn reputation_rank_thresholds() {
    for (standing, rank) in [
        (-42000, 0),
        (-6001, 0),
        (-6000, 1),
        (-3001, 1),
        (-3000, 2),
        (-1, 2),
        (0, 3),
        (2999, 3),
        (3000, 4),
        (8999, 4),
        (9000, 5),
        (20999, 5),
        (21000, 6),
        (41999, 6),
        (42000, 7),
        (42999, 7),
    ] {
        assert_eq!(reputation_rank(standing), rank, "standing {standing}");
    }
    // The nameplate's collapse: <= 1 hostile, >= 4 friendly, else neutral.
    assert_eq!(Reaction::from_rank(0), Reaction::Hostile);
    assert_eq!(Reaction::from_rank(1), Reaction::Hostile);
    assert_eq!(Reaction::from_rank(2), Reaction::Neutral);
    assert_eq!(Reaction::from_rank(3), Reaction::Neutral);
    assert_eq!(Reaction::from_rank(4), Reaction::Friendly);
    assert_eq!(Reaction::from_rank(7), Reaction::Friendly);
}

#[test]
fn real_gameobject_factions_resolve_toward_both_player_templates() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_faction_catalog(&mut chain).expect("load factions");
    let (alliance, horde) = (
        cat.template(1).expect("Alliance player template"),
        cat.template(2).expect("Horde player template"),
    );

    // 114 "monster" (Deadmines' Factory Door): hostile to both, so no one can highlight it.
    let monster = cat.template(114).expect("FactionTemplate 114");
    assert_eq!(monster.reaction_toward(alliance), Reaction::Hostile);
    assert_eq!(monster.reaction_toward(horde), Reaction::Hostile);

    // 35 (levers, torches, Arathi Basin's banners): friendly to both, never gated.
    let usable = cat.template(35).expect("FactionTemplate 35");
    assert_eq!(usable.reaction_toward(alliance), Reaction::Friendly);
    assert_eq!(usable.reaction_toward(horde), Reaction::Friendly);
}

#[test]
fn real_faction_group_names_by_mask() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let cat = load_faction_catalog(&mut chain).expect("load factions");
    assert_eq!(cat.faction_group_name(2), Some("Alliance"));
    assert_eq!(cat.faction_group_name(4), Some("Horde"));
    assert_eq!(cat.faction_group_name(0), None);
}
