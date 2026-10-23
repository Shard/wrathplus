use super::*;
use wrathplus_protocol::wrath_raid_browser::{Bound, CharacterInfo};

fn player(guid: u64, group: Option<u64>, leader: bool) -> Player {
    Player {
        guid,
        flags: 0,
        character: Some(CharacterInfo {
            level: 80,
            class: 2,
            race: 1,
            talents: [51, 20, 0],
            armor: 1,
            spell_damage: 2,
            spell_heal: 3,
            crit_melee: 4,
            crit_ranged: 5,
            crit_spell: 6,
            mp5: 7.0,
            mp5_combat: 8.0,
            attack_power: 9,
            agility: 10,
            health: 11,
            mana: 12,
            raw_online: 1,
            average_item_level: 213.0,
            defense: 13,
            dodge: 14,
            block: 15,
            parry: 16,
            haste: 17,
            expertise: 18,
        }),
        comment: Some(format!("c{guid}")),
        group_leader: leader.then_some(1),
        group_guid: group,
        roles: Some(2),
        area: Some(4395),
        status: None,
        bound: None,
    }
}

fn list(difference: bool, deleted: Vec<u64>, groups: Vec<Group>, players: Vec<Player>) -> Packet {
    Packet::List {
        raw_type: 2,
        dungeon: 251,
        difference,
        deleted,
        raw_group_word: 0,
        groups,
        raw_player_word: 0,
        players,
    }
}

#[test]
fn full_lists_replace_differences_patch_and_groups_gather_their_members() {
    let mut browser = RaidBrowser::default();
    let group = Group {
        guid: 0x900,
        flags: 0,
        comment: Some("ICC".into()),
        roles: None,
        bound: Some(Bound {
            instance: 1,
            encounter_mask: 5,
        }),
    };
    apply(
        &mut browser,
        list(
            false,
            vec![],
            vec![group],
            vec![
                player(1, None, false),
                player(2, Some(0x900), false),
                player(3, Some(0x900), true),
            ],
        ),
    );
    let name = |guid: u64| (guid != 2).then(|| format!("P{guid}"));
    let area = |id: u32| (id == 4395).then(|| "Dalaran".to_string());
    let shown = view(&browser, name, area);
    assert_eq!(
        (shown.dungeon, shown.total_groups, shown.total_players),
        (251, 1, 1)
    );
    let g = &shown.groups[0];
    assert_eq!((g.comment.as_str(), g.encounter_mask), ("ICC", 5));
    assert_eq!(
        g.members.iter().map(|m| m.guid).collect::<Vec<_>>(),
        [3, 2],
        "the leader first"
    );
    assert_eq!(g.members[0].roles & 1, 1, "the leader bit");
    assert_eq!(g.members[1].name, None, "unresolved names stay None");
    let solo = &shown.players[0];
    assert_eq!(
        (solo.class_file.as_deref(), solo.area.as_deref(), solo.level),
        (Some("PALADIN"), Some("Dalaran"), 80)
    );
    assert_eq!(solo.stats.avg_item_level, 213.0);

    apply(
        &mut browser,
        list(true, vec![1, 0x900], vec![], vec![player(4, None, false)]),
    );
    let shown = view(&browser, name, area);
    assert!(shown.groups.is_empty());
    assert_eq!(
        shown.players.iter().map(|p| p.guid).collect::<Vec<_>>(),
        [4]
    );
    assert!(browser.dirty);

    apply(&mut browser, list(false, vec![], vec![], vec![]));
    assert!(
        view(&browser, name, area).players.is_empty(),
        "a full list replaces"
    );
}
