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
    assert_eq!(
        route(&ClientCommand::GuildRank {
            rank_id: 0,
            rights: 0,
            name: "x".into()
        }),
        WrathRoute::DisabledGameAction
    );
}
