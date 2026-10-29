use super::*;
use wrathplus_formats::MAX_TALENT_RANK;

fn talent(id: u32, tab: u32, row: u32, col: u32, ranks: &[u32]) -> Talent {
    let mut r = [0u32; MAX_TALENT_RANK];
    r[..ranks.len()].copy_from_slice(ranks);
    Talent {
        id,
        tab,
        row,
        col,
        ranks: r,
        prereq_talent: 0,
        prereq_rank: 0,
        required_spell: 0,
        exceptional: false,
    }
}

#[test]
fn rank_is_the_highest_known_rank_spell() {
    let t = talent(1, 81, 0, 0, &[100, 101, 102]);
    let known: BTreeSet<u32> = [100, 101].into_iter().collect();
    assert_eq!(rank_of(&t, &known), 2);
    assert_eq!(rank_of(&t, &BTreeSet::new()), 0);
    // Learning up to a rank leaves no gap on the wire, but a gap still reads by the max.
    let holey: BTreeSet<u32> = [102].into_iter().collect();
    assert_eq!(rank_of(&t, &holey), 3);
}

/// The rank reads the known spells alone, so a wipe shows once the `SMSG_REMOVED_SPELL` burst
/// empties them; the points are a descriptor field and arrive on their own.
#[test]
fn an_emptied_book_reads_as_a_wiped_tree() {
    let t = talent(1, 201, 0, 0, &[14522, 14788, 14789]);
    let spent: BTreeSet<u32> = [14522, 14788, 14789].into_iter().collect();
    assert_eq!(rank_of(&t, &spent), 3);
    assert_eq!(rank_of(&t, &BTreeSet::new()), 0);
}

/// `LearnPreviewTalents` reaches the wire as `CMSG_LEARN_PREVIEW_TALENTS`' pairs.
#[test]
fn a_learned_preview_sends_its_pairs() {
    use crate::net::NetCommands;
    use bevy::ecs::system::RunSystemOnce;

    let mut world = World::new();
    let mut script = UiScript::new().expect("VM");
    script.set_talents(TalentUiState {
        tabs: vec![TalentTabView::default()],
        talents: vec![vec![TalentView {
            id: 2232,
            name: "Improved Heroic Strike".into(),
            tier: 1,
            column: 1,
            max_rank: 3,
            meets_prereq: true,
            ..Default::default()
        }]],
        points: (2, 0),
    });
    script
        .run("AddPreviewTalentPoints(1, 1, 2) LearnPreviewTalents()")
        .expect("lua");
    world.insert_non_send_resource(script);
    let (tx, rx) = crossbeam_channel::unbounded();
    world.insert_resource(NetCommands(tx));
    world.init_resource::<PlayerActions>();
    world.run_system_once(drain_talent_learns).expect("drain");
    let sent: Vec<ClientCommand> = rx.try_iter().collect();
    assert!(
        matches!(
            sent.as_slice(),
            [ClientCommand::NativeInspect(
                wrathplus_protocol::wrath_inspect::Command::LearnPreviewTalents { talents }
            )] if talents == &vec![(2232, 1)]
        ),
        "{sent:?}"
    );
}
