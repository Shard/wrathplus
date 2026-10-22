use super::*;

#[test]
fn npc_flags_retain_services_absent_from_vanilla() {
    let fields = ObjectFields::from_pairs(&[(82, 0x1100_1081)]);
    assert_eq!(fields.unit_npc_flags(), 0x1100_1081);
}

#[test]
fn final_quest_slot_preserves_unclamped_counters_and_timer() {
    let fields = ObjectFields::from_pairs(&[
        (278, 783),
        (279, 1),
        (280, 3 | (65535 << 16)),
        (281, 70 << 16),
        (282, 12345),
    ]);
    let quest = fields.player_quest_log(24).unwrap();
    assert_eq!(quest.quest_id, 783);
    assert_eq!(quest.counters, [3, 65535, 0, 70]);
    assert_eq!(quest.state, 1);
    assert_eq!(quest.timer, 12345);
    assert_eq!(fields.player_quest_log(25), None);
}
