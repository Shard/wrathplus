use super::*;

fn w(b: &mut Vec<u8>, n: u32) {
    b.extend(n.to_le_bytes());
}

/// AC's player entry for a solo searcher: character info, roles, comment, bound, area.
fn solo_player(b: &mut Vec<u8>) {
    b.extend(5u64.to_le_bytes());
    let flags = FLAG_CHARACTER_INFO | FLAG_ROLES | FLAG_COMMENT | FLAG_BOUND | FLAG_AREA;
    w(b, flags);
    b.extend([80, 1, 1, 10, 20, 41]);
    for _ in 0..6 {
        w(b, 100);
    }
    b.extend(1.5f32.to_le_bytes());
    b.extend(2.5f32.to_le_bytes());
    for _ in 0..5 {
        w(b, 7);
    }
    b.extend(213.0f32.to_le_bytes());
    for _ in 0..6 {
        w(b, 9);
    }
    b.extend(b"LF tank\0");
    b.push(2); // roles
    w(b, 4395); // area
    b.extend(0x77u64.to_le_bytes());
    w(b, 3);
}

#[test]
fn a_full_list_reads_groups_and_players_by_their_flags() {
    let mut b = Vec::new();
    w(&mut b, 2);
    w(&mut b, 251);
    b.push(0);
    w(&mut b, 1);
    w(&mut b, 0);
    // AC's group entry: comment, roles, bound.
    b.extend(0x99u64.to_le_bytes());
    w(&mut b, FLAG_COMMENT | FLAG_ROLES | FLAG_BOUND);
    b.extend(b"ICC 10\0");
    b.extend([0, 0, 0]);
    b.extend(0x77u64.to_le_bytes());
    w(&mut b, 1);
    w(&mut b, 1);
    w(&mut b, 0);
    solo_player(&mut b);
    let Packet::List {
        dungeon,
        difference,
        groups,
        players,
        deleted,
        ..
    } = decode(0x360, &b).unwrap()
    else {
        panic!()
    };
    assert_eq!((dungeon, difference, deleted.len()), (251, false, 0));
    assert_eq!(groups[0].comment.as_deref(), Some("ICC 10"));
    assert_eq!(
        groups[0].bound,
        Some(Bound {
            instance: 0x77,
            encounter_mask: 1
        })
    );
    let p = &players[0];
    assert_eq!(
        p.character
            .as_ref()
            .map(|c| (c.level, c.class, c.average_item_level)),
        Some((80, 1, 213.0))
    );
    assert_eq!(
        (
            p.comment.as_deref(),
            p.roles,
            p.area,
            p.group_guid,
            p.status
        ),
        (Some("LF tank"), Some(2), Some(4395), None, None)
    );
    for end in 0..b.len() {
        assert!(decode(0x360, &b[..end]).is_err(), "{end}");
    }
    assert!(decode(0x360, &[b.clone(), vec![0]].concat()).is_err());
}

#[test]
fn a_difference_lists_removals_first_and_the_search_flag_decodes() {
    let mut b = Vec::new();
    w(&mut b, 2);
    w(&mut b, 251);
    b.push(1);
    w(&mut b, 2);
    b.extend(1u64.to_le_bytes());
    b.extend(2u64.to_le_bytes());
    w(&mut b, 0);
    w(&mut b, 0);
    w(&mut b, 0);
    w(&mut b, 0);
    let Packet::List {
        difference,
        deleted,
        groups,
        players,
        ..
    } = decode(0x360, &b).unwrap()
    else {
        panic!()
    };
    assert!(difference && groups.is_empty() && players.is_empty());
    assert_eq!(deleted, [1, 2]);
    assert!(decode(0x360, &{
        let mut bad = b.clone();
        bad[8] = 2;
        bad
    })
    .is_err());
    assert_eq!(
        decode(0x369, &[1]).unwrap(),
        Packet::Searching { searching: true }
    );
    assert!(decode(0x369, &[]).is_err());
    assert_eq!(
        encode(&Command::Join { dungeon: 251 }),
        (0x35e, 251u32.to_le_bytes().to_vec())
    );
    assert_eq!(encode(&Command::Leave { dungeon: 251 }).0, 0x35f);
}
