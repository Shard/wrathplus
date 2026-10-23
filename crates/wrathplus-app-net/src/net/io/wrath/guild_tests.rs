use super::*;
#[test]
fn core_guild_routes_encode_native_bodies_and_fail_closed_on_nuls() {
    for (command, op, body) in [
        (
            ClientCommand::GuildQuery {
                guild_id: 0x01020304,
            },
            0x54,
            vec![4, 3, 2, 1],
        ),
        (ClientCommand::GuildInfoRequest, 0x87, vec![]),
        (ClientCommand::GuildRosterRequest, 0x89, vec![]),
        (
            ClientCommand::GuildInvite { name: "é".into() },
            0x82,
            "é\0".as_bytes().to_vec(),
        ),
        (ClientCommand::GuildAccept, 0x84, vec![]),
        (ClientCommand::GuildDecline, 0x85, vec![]),
        (
            ClientCommand::GuildPromote {
                name: "name".into(),
            },
            0x8b,
            b"name\0".to_vec(),
        ),
        (
            ClientCommand::GuildDemote {
                name: "name".into(),
            },
            0x8c,
            b"name\0".to_vec(),
        ),
        (
            ClientCommand::GuildRemove {
                name: "name".into(),
            },
            0x8e,
            b"name\0".to_vec(),
        ),
        (ClientCommand::GuildMotd { motd: "".into() }, 0x91, vec![0]),
        (ClientCommand::GuildLeave, 0x8d, vec![]),
        (ClientCommand::GuildDisband, 0x8f, vec![]),
        (ClientCommand::GuildLeader { name: "Bob".into() }, 0x90, b"Bob\0".to_vec()),
        (ClientCommand::GuildAddRank { name: "Alt".into() }, 0x232, b"Alt\0".to_vec()),
        (ClientCommand::GuildDelRank, 0x233, vec![]),
        (
            ClientCommand::GuildSetOfficerNote { name: "Bob".into(), note: "x".into() },
            0x235,
            b"Bob\0x\0".to_vec(),
        ),
        (ClientCommand::GuildInfoText { text: "hi".into() }, 0x2fc, b"hi\0".to_vec()),
        (ClientCommand::TabardVendorActivate { npc: 9 }, 0x1f2, 9u64.to_le_bytes().to_vec()),
        (ClientCommand::GuildEventLogQuery, 0x3ff, vec![]),
        (
            ClientCommand::NativeGuildRank(wrathplus_protocol::wrath_guild_rank::SaveRank {
                rank: 1,
                flags: 2,
                name: b"R".to_vec(),
                money_limit: 3,
                tabs: std::array::from_fn(|i| wrathplus_protocol::wrath_guild_rank::TabPermission {
                    rights: i as u32,
                    item_limit: 0,
                }),
            }),
            0x231,
            [1u32.to_le_bytes().to_vec(), 2u32.to_le_bytes().to_vec(), b"R\0".to_vec(), 3u32.to_le_bytes().to_vec(), (0..6u32).flat_map(|i| [i.to_le_bytes(), 0u32.to_le_bytes()].concat()).collect()].concat(),
        ),
    ] {
        assert_eq!(route(&command), WrathRoute::NativeGuild);
        assert_eq!(guild_packet(command).unwrap(), (op, body));
    }
    for command in [
        ClientCommand::GuildInvite {
            name: "a\0b".into(),
        },
        ClientCommand::GuildPromote { name: "\0".into() },
        ClientCommand::GuildDemote { name: "\0".into() },
        ClientCommand::GuildRemove { name: "\0".into() },
        ClientCommand::GuildMotd { motd: "\0".into() },
    ] {
        assert!(guild_packet(command).is_err());
    }
}
