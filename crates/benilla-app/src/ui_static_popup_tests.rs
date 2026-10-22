use super::*;

/// The queued changes on the `GetTime` clock, and the view's encounter counts (`00553830`).
#[test]
fn lock_changes_set_clear_and_count_encounters() {
    let changes = lock_changes(
        vec![Some((60_000, 0b1010, 1)), None, Some((30_000, 0b1010, 0))],
        100.0,
    );
    assert_eq!(changes.len(), 3);
    assert!(changes[1].is_none());
    assert_eq!(changes[0].map(|l| l.expires_at), Some(160.0));
    let lock = changes[2].expect("the last warning sets");
    let row = |map, difficulty, bit| benilla_ui::script::WrathLfgEncounter {
        map,
        difficulty,
        bit,
        name: String::new(),
        icon: None,
    };
    let rows = [
        row(533, 0, 1),
        row(533, 0, 2),
        row(533, 0, 3),
        row(533, 1, 1),
        row(36, 0, 3),
    ];
    let view = lock.view(&rows, Some((533, 0)));
    assert_eq!((view.expires_at, view.extending), (130.0, false));
    assert_eq!((view.encounters_total, view.encounters_completed), (3, 2));
    let none = lock.view(&rows, None);
    assert_eq!((none.encounters_total, none.encounters_completed), (0, 0));
}

/// The active group's set glyph slots, 1-based.
#[test]
fn filled_sockets_are_the_active_groups_glyphs() {
    let mut progress = crate::net::wrath_notices::WrathProgress::default();
    let group = |glyphs: Vec<u16>| benilla_protocol::events::TalentGroup {
        talents: Vec::new(),
        glyphs,
    };
    progress.talents.insert(
        false,
        (
            0,
            1,
            vec![group(vec![5, 0, 0]), group(vec![0, 7, 0, 9, 0, 0])],
        ),
    );
    assert_eq!(filled_glyph_sockets(Some(&progress)), [2, 4]);
    assert!(filled_glyph_sockets(None).is_empty());
}

/// Each answer's send; `CancelSummon` needs a latched summoner, a refund a resolved item, notes a
/// known friend and a complaint a held line.
#[test]
fn answers_map_to_their_sends() {
    let team = |t: u8| u32::from(t) * 10;
    let item = |bag: i64, slot: u32, equipped: bool| {
        Some(bag as u64 * 100 + u64::from(slot) + u64::from(equipped))
    };
    let friend = |index: Option<u32>, name: Option<&str>| match (index, name) {
        (Some(2), _) => Some(0x42),
        (None, Some("Bob")) => Some(0x43),
        _ => None,
    };
    let line = |id: Option<i32>, _: Option<&str>| {
        (id == Some(9)).then_some(ComplaintLine {
            sender: 0x44,
            language: 7,
            chat_type: 1,
            seconds_since: 3,
        })
    };
    let none2 = |_: i64, _: u32, _: bool| None;
    let look = |summoner| Lookups {
        team_id: &team,
        summoner,
        granter: Some(0x55),
        refund_item: &item,
        friend: &friend,
        chat_line: &line,
        banker: Some(0x66),
    };
    let send = |request: WrathPopupRequest| answer(&request, &look(None));
    assert!(matches!(
        answer(&WrathPopupRequest::CancelSummon, &look(Some(7))),
        Some(ClientCommand::NativeDeath(
            benilla_protocol::wrath_death::Command::SummonResponse {
                summoner: 7,
                agree: false
            }
        ))
    ));
    assert!(send(WrathPopupRequest::CancelSummon).is_none());
    assert!(matches!(
        send(WrathPopupRequest::DeclineInvite {
            channel: "Trade".into()
        }),
        Some(ClientCommand::DeclineChannelInvite { name }) if name == "Trade"
    ));
    assert!(matches!(
        send(WrathPopupRequest::ArenaTeamDisband { team: 2 }),
        Some(ClientCommand::ArenaTeamCommand(
            benilla_protocol::wrath_arena_team::Command::Disband { team_id: 20 }
        ))
    ));
    assert!(matches!(
        send(WrathPopupRequest::RespondInstanceLock { accept: true }),
        Some(ClientCommand::NativeInstanceControl(
            benilla_protocol::wrath_instance_control::Command::LockResponse { raw_accept: 1 }
        ))
    ));
    assert!(matches!(
        send(WrathPopupRequest::RefundItemPurchase {
            bag: 1,
            slot: 4,
            equipped: false
        }),
        Some(ClientCommand::NativeInventory(
            benilla_protocol::wrath_inventory::Command::Refund { item_guid: 104 }
        ))
    ));
    let no_refund = Lookups {
        refund_item: &none2,
        ..look(None)
    };
    assert!(answer(
        &WrathPopupRequest::RefundItemPurchase {
            bag: 1,
            slot: 4,
            equipped: false
        },
        &no_refund
    )
    .is_none());
    assert!(matches!(
        send(WrathPopupRequest::GmResponseResolve),
        Some(ClientCommand::NativeHelp(
            benilla_protocol::wrath_help::Command::ResolveResponse
        ))
    ));
    assert!(matches!(
        send(WrathPopupRequest::RemoveGlyphFromSocket { socket: 3 }),
        Some(ClientCommand::NativeInspect(
            benilla_protocol::wrath_inspect::Command::RemoveGlyph { slot: 2 }
        ))
    ));
    assert!(matches!(
        send(WrathPopupRequest::SetFriendNotes {
            index: Some(2),
            name: None,
            note: "tank".into()
        }),
        Some(ClientCommand::SetContactNotes { guid: 0x42, .. })
    ));
    assert!(send(WrathPopupRequest::SetFriendNotes {
        index: None,
        name: Some("Eve".into()),
        note: String::new()
    })
    .is_none());
    assert!(matches!(
        send(WrathPopupRequest::ComplainChat {
            line_id: Some(9),
            name: None,
            text: None
        }),
        Some(ClientCommand::NativeHelp(
            benilla_protocol::wrath_help::Command::ComplainChat {
                spammer: 0x44,
                language: 7,
                message_type: 1,
                channel_id: 0,
                seconds_since: 3,
                ..
            }
        ))
    ));
    // The purchase is the host's (it may refuse with a line); the deposit names the banker.
    assert!(send(WrathPopupRequest::BuyGuildBankTab).is_none());
    assert!(matches!(
        send(WrathPopupRequest::DepositGuildBankMoney { copper: 250 }),
        Some(ClientCommand::NativeGuildBank(
            benilla_protocol::wrath_guild_bank_commands::Command::DepositMoney {
                banker: 0x66,
                amount: 250
            }
        ))
    ));
    let no_banker = Lookups {
        banker: None,
        ..look(None)
    };
    assert!(answer(
        &WrathPopupRequest::DepositGuildBankMoney { copper: 250 },
        &no_banker
    )
    .is_none());
    assert!(matches!(
        send(WrathPopupRequest::AcceptLevelGrant),
        Some(ClientCommand::NativeReferFriend(
            benilla_protocol::wrath_refer_friend::Command::AcceptLevelGrant { granter: 0x55 }
        ))
    ));
}

/// `UnitIsControlling`: a unit with a charm, else a summon.
#[test]
fn controlling_units_hold_a_charm_or_a_summon() {
    use benilla_protocol::types::{Field, ObjectFields};
    let unit = |slots: &[(Field, u16, u32)]| ObjectStore(ObjectFields::from_field_slots(slots));
    let charmer = unit(&[(Field::UnitCharm, 0, 5)]);
    let summoner = unit(&[(Field::UnitCharm, 0, 0), (Field::UnitSummon, 0, 6)]);
    let neither = unit(&[(Field::UnitCharm, 0, 0)]);
    assert_eq!(
        controlling([(1, &charmer), (2, &summoner), (3, &neither)].into_iter()),
        [1, 2]
    );
}

/// `0x5a6b20`/`0x5a5d30`: the next tab's price by the purchased count; a purchase needs the
/// frame open on a banker and the vault's money plus the player's to cover it.
#[test]
fn guild_bank_tabs_cost_by_count_and_buy_with_the_vault_and_purse() {
    assert_eq!(tab_cost(0), Some(1_000_000));
    assert_eq!(tab_cost(5), Some(50_000_000));
    assert_eq!(tab_cost(6), None);
    assert!(buy_tab(None, (true, 0, 0), u64::MAX).is_none());
    assert!(buy_tab(Some(9), (false, 0, 0), u64::MAX).is_none());
    assert!(buy_tab(Some(9), (true, 6, 0), u64::MAX).is_none());
    assert!(matches!(
        buy_tab(Some(9), (true, 1, 1_000_000), 1_499_999),
        Some(Err("ERR_BANKSLOT_INSUFFICIENT_FUNDS"))
    ));
    assert!(matches!(
        buy_tab(Some(9), (true, 1, 1_000_000), 1_500_000),
        Some(Ok(ClientCommand::NativeGuildBank(
            benilla_protocol::wrath_guild_bank_commands::Command::BuyTab { banker: 9, tab: 1 }
        )))
    ));
}

/// `CannotBeResurrected`: an aura whose set effect bit names aura type 314.
#[test]
fn a_prevent_resurrection_effect_blocks_resurrection() {
    let aura = |spell_id, flags| benilla_protocol::wrath_auras::NativeAura {
        slot: 0,
        spell_id,
        flags,
        level: 80,
        stacks: 0,
        caster_guid: None,
        max_duration_ms: None,
        remaining_ms: None,
    };
    // Spell 9 has 314 in effect 1 only.
    let kind = |spell: u32, k: usize| if spell == 9 && k == 1 { 314 } else { 6 };
    assert!(cannot_be_resurrected(Some([aura(9, 0b010)].iter()), kind));
    assert!(!cannot_be_resurrected(Some([aura(9, 0b001)].iter()), kind));
    assert!(!cannot_be_resurrected(Some([aura(8, 0b111)].iter()), kind));
    assert!(!cannot_be_resurrected(
        None::<std::slice::Iter<'_, _>>,
        kind
    ));
}
