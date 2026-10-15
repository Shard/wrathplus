use super::*;
use benilla_protocol::wrath_arena_team::Command;
#[test]
fn native_arena_commands_route_and_encode_without_ui_producers() {
    for (command, opcode, body) in [
        (
            Command::Query {
                team_id: 0x01020304,
            },
            0x34b,
            vec![4, 3, 2, 1],
        ),
        (Command::Roster { team_id: u32::MAX }, 0x34d, vec![255; 4]),
        (
            Command::Invite {
                team_id: 1,
                name: "é".as_bytes().to_vec(),
            },
            0x34f,
            vec![1, 0, 0, 0, 0xc3, 0xa9, 0],
        ),
        (Command::Accept, 0x351, vec![]),
        (Command::Decline, 0x352, vec![]),
    ] {
        assert_eq!(
            route(&ClientCommand::ArenaTeamCommand(command.clone())),
            WrathRoute::NativeArenaTeam
        );
        assert_eq!(arena_team_packet(&command).unwrap(), (opcode, body));
    }
    assert!(arena_team_packet(&Command::Invite {
        team_id: 1,
        name: b"a\0b".to_vec()
    })
    .is_err());
}
