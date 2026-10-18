use super::*;

fn w(n: u32) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}

#[test]
fn the_showlist_row_carries_a_required_count_and_the_query_ends_with_the_type() {
    let mut list = 0x77u64.to_le_bytes().to_vec();
    list.push(1);
    for word in [1, 5863, 16161, 1000, 0, 9] {
        list.extend(w(word));
    }
    let SessionEvent::PetitionShowList(shown) = decode(0x1bc, &list).unwrap() else {
        panic!()
    };
    assert_eq!((shown.npc, shown.entries.len()), (0x77, 1));
    assert_eq!(
        (
            shown.entries[0].charter_entry,
            shown.entries[0].charter_cost
        ),
        (5863, 1000)
    );
    for end in 0..list.len() {
        assert!(decode(0x1bc, &list[..end]).is_err(), "{end}");
    }

    let mut query = w(42);
    query.extend(0x99u64.to_le_bytes());
    query.extend(b"Stormguard\0\0");
    for word in [9, 9, 0, 0, 0, 0, 0] {
        query.extend(w(word));
    }
    query.extend(0u16.to_le_bytes());
    for word in [0, 0, 0, 0, 0] {
        query.extend(w(word));
    }
    let SessionEvent::PetitionQueryResponse(q) = decode(0x1c7, &query).unwrap() else {
        panic!()
    };
    assert_eq!(
        (q.petition_id, q.owner, q.name.as_str()),
        (42, 0x99, "Stormguard")
    );
    assert_eq!((q.min_signatures, q.max_signatures), (9, 9));
    assert!(q.choices.is_empty());
    assert!(
        decode(0x1c7, &query[..query.len() - 4]).is_err(),
        "the type word is required"
    );
    assert!(decode(0x1c7, &[query.clone(), vec![0]].concat()).is_err());
}

#[test]
fn signatures_results_decline_rename_and_turn_in_decode() {
    let mut sigs = 1u64.to_le_bytes().to_vec();
    sigs.extend(2u64.to_le_bytes());
    sigs.extend(w(42));
    sigs.push(1);
    sigs.extend(3u64.to_le_bytes());
    sigs.extend(w(0));
    let SessionEvent::PetitionShowSignatures(s) = decode(0x1bf, &sigs).unwrap() else {
        panic!()
    };
    assert_eq!((s.item, s.owner, s.signatures[0].signer), (1, 2, 3));
    assert!(matches!(
        decode(
            0x1c1,
            &[1u64.to_le_bytes(), 2u64.to_le_bytes()]
                .concat()
                .into_iter()
                .chain(w(0))
                .collect::<Vec<_>>()
        )
        .unwrap(),
        SessionEvent::PetitionSignResults(PetitionSignResults {
            item: 1,
            player: 2,
            result: 0
        })
    ));
    assert!(matches!(
        decode(0x1c2, &5u64.to_le_bytes()).unwrap(),
        SessionEvent::PetitionDeclined { player: 5 }
    ));
    assert!(matches!(
        decode(0x1c5, &w(4)).unwrap(),
        SessionEvent::TurnInPetitionResults { result: 4 }
    ));
    let mut rename = 7u64.to_le_bytes().to_vec();
    rename.extend(b"New\0");
    let SessionEvent::PetitionRenamed(r) = decode(0x2c1, &rename).unwrap() else {
        panic!()
    };
    assert_eq!((r.item, r.name.as_str()), (7, "New"));
}

#[test]
fn commands_encode_ac_read_order() {
    let (op, body) = encode(&Command::Buy {
        npc: 0x77,
        name: "G".into(),
        index: 1,
    });
    assert_eq!(op, 0x1bd);
    // npc, u32, u64, "G\0", "\0", 7*u32, u16, 3*u32, ten "\0", index, u32.
    assert_eq!(body.len(), 8 + 4 + 8 + 2 + 1 + 28 + 2 + 12 + 10 + 4 + 4);
    assert_eq!(&body[body.len() - 8..body.len() - 4], &1u32.to_le_bytes());
    assert_eq!(
        encode(&Command::Offer { item: 1, player: 2 }),
        (
            0x1c3,
            [
                w(0),
                1u64.to_le_bytes().to_vec(),
                2u64.to_le_bytes().to_vec()
            ]
            .concat()
        )
    );
    assert_eq!(encode(&Command::Sign { item: 1, byte: 1 }).1.len(), 9);
    assert_eq!(
        encode(&Command::Query {
            petition_id: 4,
            item: 5
        })
        .0,
        0x1c6
    );
    assert_eq!(encode(&Command::Decline { item: 1 }).0, 0x1c2);
    assert_eq!(
        encode(&Command::Rename {
            item: 1,
            name: "N".into()
        })
        .1,
        [1u64.to_le_bytes().to_vec(), b"N\0".to_vec()].concat()
    );
}
