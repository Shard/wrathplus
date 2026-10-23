use super::*;

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

#[test]
fn result_preserves_raw_codes_and_conditional_tails() {
    for (action, result, extra, tail) in [
        (0xfedcba98, 0xffffffff, vec![], ResultTail::None),
        (
            2,
            1,
            vec![0x87654321],
            ResultTail::EquipError { error: 0x87654321 },
        ),
        (
            2,
            0,
            vec![0x87654321, 0xffffffff],
            ResultTail::ItemTaken {
                item_low_guid: 0x87654321,
                count: 0xffffffff,
            },
        ),
        // The server sends item words for non-equipment failures too.
        (
            2,
            3,
            vec![0, 0],
            ResultTail::ItemTaken {
                item_low_guid: 0,
                count: 0,
            },
        ),
    ] {
        let mut body = words(&[0xffffffff, action, result]);
        body.extend(words(&extra));
        assert_eq!(
            decode(0x239, &body).unwrap(),
            Packet::Result(MailResult {
                mail_id: u32::MAX,
                action,
                result,
                tail
            })
        );
        for end in 0..body.len() {
            assert!(decode(0x239, &body[..end]).is_err(), "prefix {end}");
        }
        body.push(0);
        assert!(decode(0x239, &body).is_err());
    }
    let mut guid = vec![8, 7, 6, 5, 4, 3, 2, 1];
    assert_eq!(
        decode(0x297, &guid).unwrap(),
        Packet::ShowMailbox {
            mailbox: 0x0102030405060708
        }
    );
    for end in 0..8 {
        assert!(decode(0x297, &guid[..end]).is_err());
    }
    guid.push(0);
    assert!(decode(0x297, &guid).is_err());
    assert!(decode(0x238, &[]).is_err());
}

#[test]
fn mutation_commands_have_native_extra_words_and_guids() {
    let mailbox = 0x0102030405060708;
    let mail_id = 0x12345678;
    let base = vec![8, 7, 6, 5, 4, 3, 2, 1, 0x78, 0x56, 0x34, 0x12];
    for (command, opcode, tail) in [
        (Command::TakeMoney { mailbox, mail_id }, 0x245, vec![]),
        (
            Command::TakeItem {
                mailbox,
                mail_id,
                item_low_guid: 0xffffffff,
            },
            0x246,
            vec![255; 4],
        ),
        (Command::MarkRead { mailbox, mail_id }, 0x247, vec![]),
        (
            Command::Return {
                mailbox,
                mail_id,
                sender: 0xffeeddccbbaa9988,
            },
            0x248,
            vec![0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff],
        ),
        (
            Command::Delete {
                mailbox,
                mail_id,
                template: 0xdeadbeef,
            },
            0x249,
            vec![0xef, 0xbe, 0xad, 0xde],
        ),
        (Command::CreateTextItem { mailbox, mail_id }, 0x24a, vec![]),
    ] {
        assert_eq!(command.opcode(), opcode);
        let mut expected = base.clone();
        expected.extend(tail);
        assert_eq!(command.encode().unwrap(), expected);
    }
}

fn mail() -> SendMail {
    SendMail {
        mailbox: 0x0102030405060708,
        receiver: vec![0xff, b'A'],
        subject: vec![],
        body: b"Text".to_vec(),
        stationery: 0x12345678,
        unknown_word: 0xffffffff,
        attachments: vec![
            Attachment {
                slot: 11,
                item: 0x1122334455667788,
            },
            Attachment { slot: 0, item: 0 },
        ],
        money: 0xabcdef01,
        cod: 0x76543210,
        unknown_guid: 0xffeeddccbbaa9988,
        unknown_byte: 0xa5,
    }
}

#[test]
fn send_mail_independent_fixture_keeps_order_and_all_raw_fields() {
    let command = Command::Send(mail());
    assert_eq!(command.opcode(), 0x238);
    assert_eq!(
        command.encode().unwrap(),
        vec![
            8, 7, 6, 5, 4, 3, 2, 1, 255, 65, 0, 0, 84, 101, 120, 116, 0, 0x78, 0x56, 0x34, 0x12,
            255, 255, 255, 255, 2, 11, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 1, 0xef, 0xcd, 0xab, 0x10, 0x32, 0x54, 0x76, 0x88, 0x99, 0xaa, 0xbb,
            0xcc, 0xdd, 0xee, 0xff, 0xa5,
        ]
    );
}

#[test]
fn send_rejects_nul_and_overflow_without_truncating() {
    for field in 0..3 {
        let mut m = mail();
        match field {
            0 => m.receiver.push(0),
            1 => m.subject.push(0),
            _ => m.body.push(0),
        }
        assert!(Command::Send(m).encode().is_err());
    }
    let mut m = mail();
    m.attachments = vec![
        Attachment {
            slot: 0,
            item: u64::MAX
        };
        12
    ];
    assert!(Command::Send(m.clone()).encode().is_ok());
    m.attachments.push(Attachment { slot: 12, item: 0 });
    assert!(Command::Send(m.clone()).encode().is_err());
    m.attachments.clear();
    assert!(Command::Send(m).encode().is_ok());
}
