//! Admission census for every implemented Wrath command variant. Packet codecs retain
//! their own validation; representative valid values exercise route/admission coherence.
use super::*;

#[test]
fn every_wrath_writer_route_is_admitted() {
    let (sender, _receiver) = crossbeam_channel::unbounded();
    let commands = crate::net::NetCommands(sender);
    let rows = vec![
        ("NativeMirrorImageQuery", ClientCommand::NativeMirrorImageQuery { guid: u64::MAX }),
        ("NativeUiTimerQuery", ClientCommand::NativeUiTimerQuery),
        (
            "GuildQuery",
            ClientCommand::GuildQuery {
                guild_id: Default::default(),
            },
        ),
        ("GuildInfoRequest", ClientCommand::GuildInfoRequest),
        ("GuildRosterRequest", ClientCommand::GuildRosterRequest),
        (
            "GuildInvite",
            ClientCommand::GuildInvite {
                name: Default::default(),
            },
        ),
        ("GuildAccept", ClientCommand::GuildAccept),
        ("GuildDecline", ClientCommand::GuildDecline),
        (
            "GuildPromote",
            ClientCommand::GuildPromote {
                name: Default::default(),
            },
        ),
        (
            "GuildDemote",
            ClientCommand::GuildDemote {
                name: Default::default(),
            },
        ),
        (
            "GuildRemove",
            ClientCommand::GuildRemove {
                name: Default::default(),
            },
        ),
        (
            "GuildMotd",
            ClientCommand::GuildMotd {
                motd: Default::default(),
            },
        ),
        (
            "NativeQuestsCompletedQuery",
            ClientCommand::NativeQuestsCompletedQuery,
        ),
        (
            "NativeMailMutation",
            ClientCommand::NativeMailMutation(
                benilla_protocol::wrath_mail_mutations::Command::TakeMoney {
                    mailbox: 1,
                    mail_id: 2,
                },
            ),
        ),
        (
            "PetNameQuery",
            ClientCommand::PetNameQuery {
                pet_number: Default::default(),
                guid: Default::default(),
            },
        ),
        (
            "NativeChatCommand",
            ClientCommand::NativeChatCommand(benilla_protocol::wrath_chat::Command {
                chat_type: 1,
                language: 0,
                target: None,
                text: b"test".to_vec(),
            }),
        ),
        (
            "NativeWorldportAck",
            ClientCommand::NativeWorldportAck(benilla_protocol::wrath_transfer::Token {
                epoch: 1,
                sequence: 2,
            }),
        ),
        (
            "ArenaTeamCommand",
            ClientCommand::ArenaTeamCommand(benilla_protocol::wrath_arena_team::Command::Accept),
        ),
        (
            "LfgCommand",
            ClientCommand::LfgCommand(benilla_protocol::wrath_lfg::commands::Command::GetStatus),
        ),
        (
            "LootRoll",
            ClientCommand::LootRoll {
                looted_target: Default::default(),
                item_slot: Default::default(),
                roll_type: Default::default(),
            },
        ),
        (
            "SetActiveMover",
            ClientCommand::SetActiveMover {
                guid: Default::default(),
            },
        ),
        ("Logout", ClientCommand::Logout),
        ("ForceLogout", ClientCommand::ForceLogout),
        ("LogoutCancel", ClientCommand::LogoutCancel),
        (
            "Move",
            ClientCommand::Move {
                kind: crate::net::MoveKind::Heartbeat,
                flags: Default::default(),
                pos: Default::default(),
                orientation: Default::default(),
                pitch: Default::default(),
                fall_time: Default::default(),
                jump: Default::default(),
                transport: Default::default(),
            },
        ),
        (
            "ItemQuery",
            ClientCommand::ItemQuery {
                entry: Default::default(),
                guid: Default::default(),
            },
        ),
        (
            "Loot",
            ClientCommand::Loot {
                guid: Default::default(),
            },
        ),
        (
            "AutostoreLootItem",
            ClientCommand::AutostoreLootItem {
                slot: Default::default(),
            },
        ),
        ("LootMoney", ClientCommand::LootMoney),
        (
            "LootRelease",
            ClientCommand::LootRelease {
                guid: Default::default(),
            },
        ),
        (
            "SetSelection",
            ClientCommand::SetSelection {
                guid: Default::default(),
            },
        ),
        (
            "AttackSwing",
            ClientCommand::AttackSwing {
                guid: Default::default(),
            },
        ),
        ("AttackStop", ClientCommand::AttackStop),
        (
            "StandStateChange",
            ClientCommand::StandStateChange {
                state: Default::default(),
            },
        ),
        (
            "SetSheathed",
            ClientCommand::SetSheathed {
                state: Default::default(),
            },
        ),
        (
            "GroupInvite",
            ClientCommand::GroupInvite {
                name: Default::default(),
            },
        ),
        ("GroupAccept", ClientCommand::GroupAccept),
        ("GroupDecline", ClientCommand::GroupDecline),
        (
            "PartyAssignment",
            ClientCommand::PartyAssignment {
                assignment: Default::default(),
                apply: Default::default(),
                guid: Default::default(),
            },
        ),
        ("GroupLeave", ClientCommand::GroupLeave),
        ("GroupRaidConvert", ClientCommand::GroupRaidConvert),
        (
            "LootMethod",
            ClientCommand::LootMethod {
                method: Default::default(),
                master: Default::default(),
                threshold: Default::default(),
            },
        ),
        (
            "SetRaidTarget",
            ClientCommand::SetRaidTarget {
                icon: Default::default(),
                guid: Default::default(),
            },
        ),
        (
            "GroupChangeSubGroup",
            ClientCommand::GroupChangeSubGroup {
                name: Default::default(),
                group: Default::default(),
            },
        ),
        (
            "GroupSwapSubGroup",
            ClientCommand::GroupSwapSubGroup {
                name: Default::default(),
                other: Default::default(),
            },
        ),
        (
            "GroupAssistantLeader",
            ClientCommand::GroupAssistantLeader {
                guid: Default::default(),
                grant: Default::default(),
            },
        ),
        ("ReadyCheckStart", ClientCommand::ReadyCheckStart),
        (
            "ReadyCheckAnswer",
            ClientCommand::ReadyCheckAnswer {
                ready: Default::default(),
            },
        ),
        (
            "RequestPartyMemberStats",
            ClientCommand::RequestPartyMemberStats {
                guid: Default::default(),
            },
        ),
        (
            "NameQuery",
            ClientCommand::NameQuery {
                guid: Default::default(),
            },
        ),
        (
            "CreatureQuery",
            ClientCommand::CreatureQuery {
                entry: Default::default(),
                guid: Default::default(),
            },
        ),
        (
            "GameObjectQuery",
            ClientCommand::GameObjectQuery {
                entry: Default::default(),
                guid: Default::default(),
            },
        ),
        (
            "SetActionButton",
            ClientCommand::SetActionButton {
                button: Default::default(),
                packed: Default::default(),
            },
        ),
        (
            "CastSpell",
            ClientCommand::CastSpell {
                spell_id: Default::default(),
                target: Default::default(),
            },
        ),
        (
            "CastSpellAtDest",
            ClientCommand::CastSpellAtDest {
                spell_id: Default::default(),
                dest: Default::default(),
            },
        ),
        (
            "CastSpellAtSource",
            ClientCommand::CastSpellAtSource {
                spell_id: Default::default(),
                src: Default::default(),
            },
        ),
        (
            "CastSpellGameObject",
            ClientCommand::CastSpellGameObject {
                spell_id: Default::default(),
                go_guid: Default::default(),
            },
        ),
        (
            "CastSpellItem",
            ClientCommand::CastSpellItem {
                spell_id: Default::default(),
                item_guid: Default::default(),
            },
        ),
        (
            "CancelCast",
            ClientCommand::CancelCast {
                spell_id: Default::default(),
            },
        ),
        (
            "CancelAura",
            ClientCommand::CancelAura {
                spell_id: Default::default(),
            },
        ),
        (
            "GossipHello",
            ClientCommand::GossipHello {
                guid: Default::default(),
            },
        ),
        (
            "GossipSelectOption",
            ClientCommand::GossipSelectOption {
                guid: Default::default(),
                option: Default::default(),
            },
        ),
        (
            "NpcTextQuery",
            ClientCommand::NpcTextQuery {
                text_id: Default::default(),
                guid: Default::default(),
            },
        ),
        (
            "QuestgiverStatusQuery",
            ClientCommand::QuestgiverStatusQuery {
                npc: Default::default(),
            },
        ),
        (
            "QuestgiverHello",
            ClientCommand::QuestgiverHello {
                npc: Default::default(),
            },
        ),
        (
            "QuestgiverQuery",
            ClientCommand::QuestgiverQuery {
                npc: Default::default(),
                quest: Default::default(),
            },
        ),
        (
            "QuestgiverAccept",
            ClientCommand::QuestgiverAccept {
                npc: Default::default(),
                quest: Default::default(),
            },
        ),
        (
            "QuestgiverComplete",
            ClientCommand::QuestgiverComplete {
                npc: Default::default(),
                quest: Default::default(),
            },
        ),
        (
            "QuestgiverRequestReward",
            ClientCommand::QuestgiverRequestReward {
                npc: Default::default(),
                quest: Default::default(),
            },
        ),
        (
            "QuestgiverChooseReward",
            ClientCommand::QuestgiverChooseReward {
                npc: Default::default(),
                quest: Default::default(),
                choice: Default::default(),
            },
        ),
        (
            "QuestQuery",
            ClientCommand::QuestQuery {
                quest: Default::default(),
            },
        ),
        (
            "QuestlogRemove",
            ClientCommand::QuestlogRemove {
                slot: Default::default(),
            },
        ),
        (
            "PushQuestToParty",
            ClientCommand::PushQuestToParty {
                quest: Default::default(),
            },
        ),
        (
            "QuestConfirmAccept",
            ClientCommand::QuestConfirmAccept {
                quest: Default::default(),
            },
        ),
        (
            "ListInventory",
            ClientCommand::ListInventory {
                guid: Default::default(),
            },
        ),
        (
            "BuyItem",
            ClientCommand::BuyItem {
                vendor: Default::default(),
                entry: Default::default(),
                count: Default::default(),
            },
        ),
        (
            "BuyItemInSlot",
            ClientCommand::BuyItemInSlot {
                vendor: Default::default(),
                entry: Default::default(),
                bag_guid: Default::default(),
                bag_slot: Default::default(),
                count: Default::default(),
            },
        ),
        (
            "SellItem",
            ClientCommand::SellItem {
                vendor: Default::default(),
                item_guid: Default::default(),
                count: Default::default(),
            },
        ),
        (
            "BuybackItem",
            ClientCommand::BuybackItem {
                vendor: Default::default(),
                slot: Default::default(),
            },
        ),
        (
            "RepairItem",
            ClientCommand::RepairItem {
                vendor: Default::default(),
                item_guid: Default::default(),
            },
        ),
        (
            "TaxiNodeStatusQuery",
            ClientCommand::TaxiNodeStatusQuery {
                guid: Default::default(),
            },
        ),
        (
            "TaxiQueryNodes",
            ClientCommand::TaxiQueryNodes {
                guid: Default::default(),
            },
        ),
        (
            "ActivateTaxi",
            ClientCommand::ActivateTaxi {
                guid: Default::default(),
                source_node: Default::default(),
                dest_node: Default::default(),
            },
        ),
        (
            "ActivateTaxiExpress",
            ClientCommand::ActivateTaxiExpress {
                guid: Default::default(),
                total_cost: Default::default(),
                nodes: Default::default(),
            },
        ),
        (
            "BinderActivate",
            ClientCommand::BinderActivate {
                binder: Default::default(),
            },
        ),
        (
            "TrainerList",
            ClientCommand::TrainerList {
                trainer: Default::default(),
            },
        ),
        (
            "ListStabledPets",
            ClientCommand::ListStabledPets {
                npc: Default::default(),
            },
        ),
        (
            "StablePet",
            ClientCommand::StablePet {
                npc: Default::default(),
            },
        ),
        (
            "UnstablePet",
            ClientCommand::UnstablePet {
                npc: Default::default(),
                pet_number: Default::default(),
            },
        ),
        (
            "StableSwapPet",
            ClientCommand::StableSwapPet {
                npc: Default::default(),
                pet_number: Default::default(),
            },
        ),
        (
            "BuyStableSlot",
            ClientCommand::BuyStableSlot {
                npc: Default::default(),
            },
        ),
        (
            "TrainerBuySpell",
            ClientCommand::TrainerBuySpell {
                trainer: Default::default(),
                spell_id: Default::default(),
            },
        ),
        (
            "GetMailList",
            ClientCommand::GetMailList {
                mailbox: Default::default(),
            },
        ),
        (
            "SetChannelWatch",
            ClientCommand::SetChannelWatch {
                name: Default::default(),
            },
        ),
        ("RequestRaidInfo", ClientCommand::RequestRaidInfo),
        (
            "GuildBankQuery",
            ClientCommand::GuildBankQuery(
                benilla_protocol::wrath_guild_bank_query::Query::Permissions,
            ),
        ),
        (
            "LfgRequestPlayerLockInfo",
            ClientCommand::LfgRequestPlayerLockInfo,
        ),
        (
            "LfgRequestPartyLockInfo",
            ClientCommand::LfgRequestPartyLockInfo,
        ),
        (
            "ChannelDisplayList",
            ClientCommand::ChannelDisplayList {
                name: Default::default(),
            },
        ),
        ("QueryNextMailTime", ClientCommand::QueryNextMailTime),
        (
            "MoveSplineDone",
            ClientCommand::MoveSplineDone {
                flags: Default::default(),
                pos: Default::default(),
                orientation: Default::default(),
                spline_id: Default::default(),
            },
        ),
    ];
    let mut covered = std::collections::BTreeSet::new();
    for (name, command) in rows {
        assert!(
            has_writer_route(&command),
            "fixture {name} lost its writer route"
        );
        assert!(
            commands.supports(crate::net::SessionDialect::Wrath12340, &command),
            "writer route rejected by admission: {name}"
        );
        covered.insert(name);
    }
    // Deliberate exclusions: deferred status has no wire effect; legacy un-tokened
    // WorldportAck has no native route. NativeWorldportAck above is admitted, while
    // the writer still requires a matching live transfer capability/epoch.
    for command in [
        ClientCommand::BattlefieldStatusRequest,
        ClientCommand::WorldportAck,
    ] {
        assert!(!has_writer_route(&command));
        assert!(!commands.supports(crate::net::SessionDialect::Wrath12340, &command));
    }
    // These variants have routes, but invalid field values must remain inadmissible.
    for command in [
        ClientCommand::AutostoreLootItem { slot: 18 },
        ClientCommand::SetSheathed { state: 3 },
        ClientCommand::StandStateChange { state: 2 },
        ClientCommand::Move {
            kind: crate::net::MoveKind::Heartbeat,
            flags: 0,
            pos: [0.; 3],
            orientation: 0.,
            pitch: 1.,
            fall_time: 0,
            jump: None,
            transport: None,
        },
    ] {
        assert!(has_writer_route(&command));
        assert!(!commands.supports(crate::net::SessionDialect::Wrath12340, &command));
    }
    // Keep the table exhaustive when a new ClientCommand arm is added to route().
    // This is a structural census, not a codec test: failures request a real fixture.
    let source = include_str!("wrath.rs");
    let route_body = source
        .split("fn route(command:")
        .nth(1)
        .unwrap()
        .split("/// Only implemented wire routes")
        .next()
        .unwrap();
    let disabled = [
        "BattlefieldStatusRequest",
        "MoveTimeSkipped",
        "ForceSpeedAck",
        "NotActiveMover",
        "TeleportAck",
        "WorldportAck",
        "MoveModeAck",
        "KnockBackAck",
    ];
    for tail in route_body.split("ClientCommand::").skip(1) {
        let name = tail
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap();
        assert!(
            covered.contains(name) || disabled.contains(&name),
            "add a valid admission fixture for {name}, or explicitly document its exclusion"
        );
    }
}
