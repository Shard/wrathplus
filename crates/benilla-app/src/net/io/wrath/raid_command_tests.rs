//! Independent opcode/body fixtures for the native raid management routes.
use super::*;

#[test]
fn native_raid_commands_route_and_encode_without_optimistic_state() {
    for (command, opcode, body) in [
        (ClientCommand::GroupRaidConvert, 0x28e, vec![]),
        (
            ClientCommand::LootMethod {
                method: 2,
                master: 0x0807060504030201,
                threshold: 4,
            },
            0x7a,
            vec![2, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 4, 0, 0, 0],
        ),
        (
            ClientCommand::SetRaidTarget {
                icon: 7,
                guid: 0x0807060504030201,
            },
            0x321,
            vec![7, 1, 2, 3, 4, 5, 6, 7, 8],
        ),
        (
            ClientCommand::SetRaidTarget {
                icon: 0xff,
                guid: 99,
            },
            0x321,
            vec![0xff],
        ),
        (
            ClientCommand::SetRaidTarget { icon: 0, guid: 0 },
            0x321,
            vec![0; 9],
        ),
        (
            ClientCommand::GroupChangeSubGroup {
                name: "Ally".into(),
                group: 7,
            },
            0x27e,
            b"Ally\0\x07".to_vec(),
        ),
        (
            ClientCommand::GroupSwapSubGroup {
                name: "Ally".into(),
                other: "Other".into(),
            },
            0x280,
            b"Ally\0Other\0".to_vec(),
        ),
        (
            ClientCommand::GroupAssistantLeader {
                guid: 7,
                grant: true,
            },
            0x28f,
            vec![7, 0, 0, 0, 0, 0, 0, 0, 1],
        ),
        (
            ClientCommand::GroupAssistantLeader {
                guid: 7,
                grant: false,
            },
            0x28f,
            vec![7, 0, 0, 0, 0, 0, 0, 0, 0],
        ),
        (ClientCommand::ReadyCheckStart, 0x322, vec![]),
        (
            ClientCommand::ReadyCheckAnswer { ready: true },
            0x322,
            vec![1],
        ),
        (
            ClientCommand::ReadyCheckAnswer { ready: false },
            0x322,
            vec![0],
        ),
    ] {
        assert_eq!(route(&command), WrathRoute::NativeSession);
        assert_eq!(session_packet(command), (opcode, body));
    }
}
