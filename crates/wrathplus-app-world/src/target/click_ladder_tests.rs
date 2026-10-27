//! The NPC service ladder (`0x6ddbb0`): its bit order, its arms' sends and the spell-click rung.

use super::*;

/// Every arm of `0x5f0130`'s walk over `UNIT_NPC_FLAGS`, in the reference's order.
#[test]
fn the_service_ladder_walks_the_reference_bit_order() {
    use cursor_mode::npc_flags as f;
    let has = Some(wrathplus_protocol::types::dialog_status::AVAILABLE);
    for (flags, arm) in [
        (f::GOSSIP, ServiceArm::Gossip),
        (f::VENDOR, ServiceArm::Vendor),
        (f::FLIGHTMASTER, ServiceArm::FlightMaster),
        (f::TRAINER, ServiceArm::Trainer),
        (f::SPIRITHEALER, ServiceArm::SpiritHealer),
        (f::SPIRITGUIDE, ServiceArm::SpiritGuide),
        (f::INNKEEPER, ServiceArm::Innkeeper),
        (f::BANKER, ServiceArm::Banker),
        (f::PETITIONER, ServiceArm::Petitioner),
        (f::TABARDDESIGNER, ServiceArm::TabardDesigner),
        (f::BATTLEMASTER, ServiceArm::Battlemaster),
        (f::AUCTIONEER, ServiceArm::Auctioneer),
        (f::STABLEMASTER, ServiceArm::StableMaster),
    ] {
        assert_eq!(service_arm(flags, None), Some(arm), "flags {flags:#x}");
    }
    // Bit 1 is the one arm with a second conjunct: the target's cached questgiver status.
    assert_eq!(
        service_arm(f::QUESTGIVER, has),
        Some(ServiceArm::Questgiver)
    );
    assert_eq!(service_arm(f::QUESTGIVER, None), None);
    // REPAIR (`0x1000` on 12340) has no arm, and an empty field matches nothing.
    assert_eq!(service_arm(0x1000, None), None);
    assert_eq!(service_arm(0, None), None);
}

#[test]
fn the_service_ladder_is_first_match_wins() {
    use cursor_mode::npc_flags as f;
    // Anything gossip-flagged keeps its menu.
    for other in [
        f::VENDOR,
        f::TRAINER,
        f::INNKEEPER,
        f::STABLEMASTER,
        f::BANKER,
    ] {
        assert_eq!(
            service_arm(f::GOSSIP | other, None),
            Some(ServiceArm::Gossip)
        );
    }
    // Banker (bit 8) before auctioneer (bit 12), both Buy(3) to the cursor.
    assert_eq!(
        service_arm(f::BANKER | f::AUCTIONEER, None),
        Some(ServiceArm::Banker)
    );
    // Trainer (bit 4) before innkeeper (bit 7).
    assert_eq!(
        service_arm(f::TRAINER | f::INNKEEPER, None),
        Some(ServiceArm::Trainer)
    );
}

#[test]
fn the_service_arms_send_what_the_reference_sends() {
    let sent = |arm, ghost| match service_action(arm, 0x42, ghost, None) {
        ServiceAction::Send(cmd) => format!("{cmd:?}"),
        ServiceAction::SellFromCursor(cmd) => format!("sell {cmd:?}"),
        ServiceAction::AskBinder => "ask-binder".to_string(),
        ServiceAction::AskSpiritHealer => "ask-spirit-healer".to_string(),
        ServiceAction::AcquireSpiritGuide => "acquire-spirit-guide".to_string(),
        ServiceAction::Silent(_) => "silent".to_string(),
    };
    assert!(matches!(
        service_action(ServiceArm::Questgiver, 0x42, false, None),
        ServiceAction::Send(ClientCommand::QuestgiverHello { npc: 0x42 })
    ));
    assert!(matches!(
        service_action(ServiceArm::Trainer, 0x42, false, None),
        ServiceAction::Send(ClientCommand::TrainerList { trainer: 0x42 })
    ));
    assert!(matches!(
        service_action(ServiceArm::Petitioner, 0x42, false, None),
        ServiceAction::Send(ClientCommand::PetitionShowList { npc: 0x42 })
    ));
    assert!(matches!(
        service_action(ServiceArm::Vendor, 0x42, false, None),
        ServiceAction::Send(ClientCommand::ListInventory { guid: 0x42 })
    ));
    assert!(matches!(
        service_action(ServiceArm::FlightMaster, 0x42, false, None),
        ServiceAction::Send(ClientCommand::TaxiQueryNodes { guid: 0x42 })
    ));
    assert!(matches!(
        service_action(ServiceArm::Banker, 0x42, false, None),
        ServiceAction::Send(ClientCommand::BankerActivate { guid: 0x42 })
    ));
    assert!(matches!(
        service_action(ServiceArm::Auctioneer, 0x42, false, None),
        ServiceAction::Send(ClientCommand::AuctionHello { auctioneer: 0x42 })
    ));
    assert!(matches!(
        service_action(ServiceArm::StableMaster, 0x42, false, None),
        ServiceAction::Send(ClientCommand::ListStabledPets { npc: 0x42 })
    ));
    // The innkeeper asks and sends nothing.
    assert_eq!(sent(ServiceArm::Innkeeper, false), "ask-binder");
    // The two ghost-gated arms give a living player nothing, as in the reference.
    assert_eq!(sent(ServiceArm::SpiritHealer, false), "silent");
    assert_eq!(sent(ServiceArm::SpiritGuide, false), "silent");
    assert_eq!(sent(ServiceArm::SpiritHealer, true), "ask-spirit-healer");
    assert!(matches!(
        service_action(ServiceArm::TabardDesigner, 0x42, false, None),
        ServiceAction::Send(ClientCommand::TabardVendorActivate { npc: 0x42 })
    ));
    assert!(matches!(
        service_action(ServiceArm::Battlemaster, 0x42, false, None),
        ServiceAction::Send(ClientCommand::BattlemasterHello { npc: 0x42 })
    ));
}

/// The sell fork lives in the vendor handler alone (`0x5df5d0`), so no other arm may change
/// with the cursor.
#[test]
fn only_the_vendor_arm_reads_the_cursor() {
    const VENDOR: u64 = 0xF130_0000_0000_0042;
    const ITEM: u64 = 0x4000_0000_0000_0099;

    // Empty cursor: the list opens.
    assert!(matches!(
        service_action(ServiceArm::Vendor, VENDOR, false, None),
        ServiceAction::Send(ClientCommand::ListInventory { guid: VENDOR })
    ));
    // An item held: the sale to the clicked NPC, count 0 (the whole stack).
    assert!(matches!(
        service_action(ServiceArm::Vendor, VENDOR, false, Some(ITEM)),
        ServiceAction::SellFromCursor(ClientCommand::SellItem {
            vendor: VENDOR,
            item_guid: ITEM,
            count: 0,
        })
    ));
    // Every other arm answers the same, held or not.
    for arm in [
        ServiceArm::Gossip,
        ServiceArm::Questgiver,
        ServiceArm::FlightMaster,
        ServiceArm::Trainer,
        ServiceArm::SpiritHealer,
        ServiceArm::SpiritGuide,
        ServiceArm::Innkeeper,
        ServiceArm::Banker,
        ServiceArm::Petitioner,
        ServiceArm::TabardDesigner,
        ServiceArm::Battlemaster,
        ServiceArm::Auctioneer,
        ServiceArm::StableMaster,
    ] {
        for ghost in [false, true] {
            let describe = |a: ServiceAction| match a {
                ServiceAction::Send(cmd) => format!("{cmd:?}"),
                ServiceAction::SellFromCursor(cmd) => format!("SELL {cmd:?}"),
                ServiceAction::AskBinder => "ask-binder".into(),
                ServiceAction::AskSpiritHealer => "ask-xp-loss".into(),
                ServiceAction::AcquireSpiritGuide => "acquire-spirit-guide".into(),
                ServiceAction::Silent(w) => format!("silent {w}"),
            };
            let empty = describe(service_action(arm, VENDOR, ghost, None));
            let held = describe(service_action(arm, VENDOR, ghost, Some(ITEM)));
            assert_eq!(
                empty, held,
                "{arm:?} (ghost={ghost}) changed its answer because an item was on the cursor"
            );
            assert!(
                !held.starts_with("SELL"),
                "{arm:?} (ghost={ghost}) reached the vendor arm's sale"
            );
        }
    }
}

mod spell_click {
    use super::super::tests::{interact_with, store, walking_world, F_HEALTH, F_MAXHEALTH, VENDOR};
    use super::*;
    use wrathplus_protocol::types::{Field, GroupMemberEntry, PartyMemberStatsInfo};
    use wrathplus_protocol::wrath_spell_control::Command as Spell;

    const SPELLCLICK: u32 = 0x0100_0000;

    /// An NPC in reach with `npc_flags` and `flags_2`, interacted with; what went out.
    fn click(
        npc_flags: u32,
        flags_2: u32,
        group: Option<crate::app_core::group::GroupState>,
    ) -> Vec<ClientCommand> {
        let (mut world, npc, rx) = walking_world(Vec3::new(2.5, 0.0, 0.0), false);
        world.entity_mut(npc).insert(store(&[
            (F_HEALTH, 100),
            (F_MAXHEALTH, 100),
            (Field::UnitNpcFlags.index(), npc_flags),
            (Field::UnitFlags2.index(), flags_2),
        ]));
        if let Some(group) = group {
            world.insert_resource(group);
        }
        interact_with(&mut world, npc);
        rx.try_iter().collect()
    }

    fn sent_spell_click(sent: &[ClientCommand]) -> bool {
        matches!(
            sent,
            [ClientCommand::NativeSpellControl(Spell::SpellClick {
                unit: VENDOR
            })]
        )
    }

    /// `0x71c570` → `0x6d2740`: a spell-click NPC sends `CMSG_SPELLCLICK` with its guid;
    /// `UNIT_FLAG2_PREVENT_SPELL_CLICK` stops it, and any earlier rung (gossip) wins.
    #[test]
    fn a_spell_click_npc_sends_its_guid_after_every_service_rung() {
        assert!(sent_spell_click(&click(SPELLCLICK, 0, None)));
        assert!(click(SPELLCLICK, 0x2000, None).is_empty(), "prevented");
        let gossip = click(SPELLCLICK | 1, 0, None);
        assert!(
            matches!(gossip[..], [ClientCommand::GossipHello { guid: VENDOR }]),
            "{gossip:?}"
        );
    }

    /// Under `UNIT_FLAG2_RESTRICT_PARTY_INTERACTION` only the group's unit qualifies: a member, or
    /// a member's pet off its online record when the member is not streamed (`0x52c8c0`).
    #[test]
    fn a_party_restricted_spell_click_needs_the_unit_to_be_the_groups() {
        assert!(click(SPELLCLICK, 0x1000, None).is_empty(), "no group");
        let member = |guid| GroupMemberEntry {
            name: "Alpha".into(),
            guid,
            status: 1,
            flags: 0,
        };
        let mut group = crate::app_core::group::GroupState::default();
        group.members.push(member(0x55));
        assert!(
            click(SPELLCLICK, 0x1000, Some(group)).is_empty(),
            "not ours"
        );
        let mut group = crate::app_core::group::GroupState::default();
        group.members.push(member(0x55));
        group.stats.insert(
            0x55,
            PartyMemberStatsInfo {
                status: Some(1),
                pet_guid: Some(VENDOR),
                ..Default::default()
            },
        );
        assert!(sent_spell_click(&click(SPELLCLICK, 0x1000, Some(group))));
        let mut group = crate::app_core::group::GroupState::default();
        group.members.push(member(VENDOR));
        assert!(sent_spell_click(&click(SPELLCLICK, 0x1000, Some(group))));
    }
}
