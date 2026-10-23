use super::*;

fn packet(kind: u8, n: u8) -> Vec<u8> {
    let mut row = vec![0, 0];
    row.extend(42u32.to_le_bytes());
    row.push(kind);
    if kind == 0 {
        row.extend(0x123456789u64.to_le_bytes());
    } else {
        row.extend(123u32.to_le_bytes());
    }
    for word in [12u32, 99, 41, 55, 16, 5f32.to_bits(), 7] {
        row.extend(word.to_le_bytes());
    }
    row.extend(b"subject\0inline body\0");
    row.push(n);
    for slot in 0..n {
        row.push(slot);
        for word in [1234 + u32::from(slot), 2589] {
            row.extend(word.to_le_bytes());
        }
        for i in 0..21u32 {
            row.extend((100 + i).to_le_bytes());
        }
        for word in [(-73i32) as u32, 66, 1000, 9, 60, 50] {
            row.extend(word.to_le_bytes());
        }
        row.push(8);
    }
    let size = row.len() as u16;
    row[..2].copy_from_slice(&size.to_le_bytes());
    let mut packet = 70u32.to_le_bytes().to_vec();
    packet.push(1);
    packet.extend(row);
    packet
}

#[test]
fn retains_twelve_items_inline_body_and_overflow_total_without_narrowing() {
    let inbox = decode_inbox(&packet(0, 12)).unwrap();
    assert_eq!(inbox.total_count, 70);
    let row = &inbox.mails[0];
    assert_eq!(row.sender, 0x123456789);
    assert_eq!(row.body, "inline body");
    assert_eq!(row.unknown, 99);
    assert_eq!(row.attachments.len(), 12);
    let last = &row.attachments[11];
    assert_eq!(last.slot, 11);
    assert_eq!(last.guid_low, 1245);
    assert_eq!(last.random_property, -73);
    assert_eq!(last.count, 1000);
    assert_eq!(last.enchantments[6], [118, 119, 120]);
    assert_eq!(last.unknown, 8);
    assert!(
        matches!(&decode(SMSG_MAIL_LIST_RESULT, &packet(0,12)).unwrap()[0],
        SessionEvent::WrathMailList { inbox } if inbox.mails[0].attachments.len() == 12)
    );
}

#[test]
fn native_sender_types_and_empty_inbox() {
    for kind in [2, 3, 4, 5] {
        let inbox = decode_inbox(&packet(kind, 0)).unwrap();
        assert_eq!(inbox.mails[0].sender, 123);
        assert!(inbox.mails[0].attachments.is_empty());
    }
    assert_eq!(decode_inbox(&[0, 0, 0, 0, 0]).unwrap().mails.len(), 0);
}

#[test]
fn strict_row_lengths_tails_counts_and_truncation() {
    let good = packet(0, 12);
    for end in 0..good.len() {
        assert!(decode_inbox(&good[..end]).is_err(), "end {end}");
    }
    let mut bad = good.clone();
    bad.push(0);
    assert!(decode_inbox(&bad).is_err());
    for size in [0u16, 2, 35, 65535] {
        let mut bad = good.clone();
        bad[5..7].copy_from_slice(&size.to_le_bytes());
        assert!(decode_inbox(&bad).is_err());
    }
    assert!(decode_inbox(&packet(0, 13)).is_err());
    assert!(decode_inbox(&packet(1, 0)).is_err());
    let mut bad = good.clone();
    bad[..4].copy_from_slice(&0u32.to_le_bytes());
    assert!(decode_inbox(&bad).is_err());
    let mut bad = good.clone();
    bad[4] = 51;
    assert!(decode_inbox(&bad).is_err());
    // Attachment offset: header5 + row47 + subject8 + body12 + count1 = 73.
    let first = 5 + 2 + 4 + 1 + 8 + 28 + 8 + 12 + 1;
    let mut bad = good.clone();
    bad[first] = 12;
    assert!(decode_inbox(&bad).is_err());
    let mut bad = good;
    bad[first + 118] = 0;
    assert!(decode_inbox(&bad).is_err());
}
