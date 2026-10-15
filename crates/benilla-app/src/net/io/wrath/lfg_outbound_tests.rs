//! Raw transport only; App's guarded proposal lifecycle is not enabled here.
use super::*;
use benilla_protocol::wrath_lfg::commands::{Command, Join};

fn join() -> Join {
    Join {
        roles: 0x80ff010f,
        no_partial_clear: true,
        achievements: false,
        slots: vec![0xab123456, 0xab123456],
        needs: [0, 128, 255],
        comment: "é".into(),
    }
}

#[test]
fn l2_l3_raw_commands_route_with_independent_opcode_body_fixtures() {
    for (command, opcode, body) in [
        (Command::Leave, 0x035d, vec![]),
        (Command::GetStatus, 0x0296, vec![]),
        (Command::SetRoles(255), 0x036a, vec![255]),
        (Command::SetComment(String::new()), 0x0366, vec![0]),
        (Command::SetComment("é".into()), 0x0366, vec![195, 169, 0]),
        (
            Command::Join(join()),
            0x035c,
            vec![
                15, 1, 255, 128, 1, 0, 2, 86, 52, 18, 171, 86, 52, 18, 171, 3, 0, 128, 255, 195,
                169, 0,
            ],
        ),
        (
            Command::ProposalResult {
                proposal_id: 0x12345678,
                accept: true,
            },
            0x0362,
            vec![120, 86, 52, 18, 1],
        ),
        (
            Command::ProposalResult {
                proposal_id: 0,
                accept: false,
            },
            0x0362,
            vec![0; 5],
        ),
    ] {
        assert_eq!(
            route(&ClientCommand::LfgCommand(command.clone())),
            WrathRoute::NativeLfg
        );
        assert_eq!(lfg_packet(&command).unwrap(), (opcode, body));
    }
}

#[test]
fn malformed_commands_fail_before_a_wire_packet_exists() {
    let mut nul = join();
    nul.comment = "before\0after".into();
    let mut oversized = join();
    oversized.slots = vec![0xfe000007; 51];
    for command in [
        Command::SetComment("\0".into()),
        Command::Join(nul),
        Command::Join(oversized),
    ] {
        assert_eq!(
            lfg_packet(&command).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
    for roles in 0..=255 {
        assert_eq!(
            lfg_packet(&Command::SetRoles(roles)).unwrap(),
            (0x036a, vec![roles])
        );
    }
}
