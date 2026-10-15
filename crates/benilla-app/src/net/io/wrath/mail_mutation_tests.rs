use super::*;
use benilla_protocol::wrath_mail_mutations::{Command, SendMail};

#[test]
fn native_mutations_route_with_complete_wire_fields() {
    let mailbox = 0x0102030405060708;
    let mail_id = 0x12345678;
    for (command, opcode, tail) in [
        (Command::TakeMoney { mailbox, mail_id }, 0x245, vec![]),
        (
            Command::TakeItem {
                mailbox,
                mail_id,
                item_low_guid: u32::MAX,
            },
            0x246,
            vec![255; 4],
        ),
        (Command::MarkRead { mailbox, mail_id }, 0x247, vec![]),
        (
            Command::Return {
                mailbox,
                mail_id,
                sender: u64::MAX,
            },
            0x248,
            vec![255; 8],
        ),
        (
            Command::Delete {
                mailbox,
                mail_id,
                template: u32::MAX,
            },
            0x249,
            vec![255; 4],
        ),
        (Command::CreateTextItem { mailbox, mail_id }, 0x24a, vec![]),
    ] {
        assert_eq!(
            route(&ClientCommand::NativeMailMutation(command.clone())),
            WrathRoute::NativeMailMutation
        );
        let mut expected = vec![8, 7, 6, 5, 4, 3, 2, 1, 0x78, 0x56, 0x34, 0x12];
        expected.extend(tail);
        assert_eq!(native_mail_packet(&command).unwrap(), (opcode, expected));
    }
    for legacy in [
        ClientCommand::MailTakeItem { mailbox, mail_id },
        ClientCommand::MailReturnToSender { mailbox, mail_id },
        ClientCommand::MailTakeMoney { mailbox, mail_id },
        ClientCommand::MailDelete { mailbox, mail_id },
        ClientCommand::MailMarkAsRead { mailbox, mail_id },
        ClientCommand::MailCreateTextItem { mailbox, mail_id },
    ] {
        assert_eq!(route(&legacy), WrathRoute::DisabledGameAction);
    }
}

#[test]
fn native_send_validates_before_a_writer_can_receive_body() {
    let mut mail = SendMail {
        mailbox: 0,
        receiver: vec![],
        subject: vec![],
        body: vec![],
        stationery: 0,
        unknown_word: 0,
        attachments: vec![],
        money: 0,
        cod: 0,
        unknown_guid: 0,
        unknown_byte: 0,
    };
    let command = Command::Send(mail.clone());
    assert_eq!(
        route(&ClientCommand::NativeMailMutation(command.clone())),
        WrathRoute::NativeMailMutation
    );
    assert_eq!(native_mail_packet(&command).unwrap(), (0x238, vec![0; 37]));
    mail.receiver.push(0);
    assert!(native_mail_packet(&Command::Send(mail)).is_err());
}
