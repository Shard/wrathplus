use super::*;
use wrathplus_protocol::types::{mail_message_type, MailListEntry};

#[test]
fn next_mail_query_uses_the_native_writer_and_updates_pending() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut pending = MailPending::default();
    pending.apply_query_reply(0.0);
    let _ = pending.take_notify();
    let commands = NetCommands(tx);
    query_next_mail_time(&commands, &mut pending);
    assert!(!pending.has_new_mail());
    match rx.try_recv() {
        Ok(ClientCommand::QueryNextMailTime) => {}
        result => panic!("unexpected producer result: {result:?}"),
    }
}

/// `GetPackageInfo`'s icon is the basename under `Interface\Icons` (`0x4ae4c6`, `0x4ae4eb`).
#[test]
fn a_package_row_answers_its_icon_under_the_icon_folder() {
    let view = package_view(&wrathplus_formats::PackageRow {
        id: 2,
        icon: "INV_BOX_04".into(),
        cost: 10,
        name: "Test Package".into(),
    });
    assert_eq!(view.icon, "Interface\\Icons\\INV_BOX_04");
    assert_eq!(
        (view.id, view.cost, view.name.as_str()),
        (2, 10, "Test Package")
    );
}

/// The 1.12 result table at `0x4ad17c` (indexed at `0x4ad1a0`), asserted as build 12340's ids
/// (`0xac8af0`) for the rows its arms push; the 12340 arms are untraced.
#[test]
fn the_mail_result_table_is_the_references_own() {
    let id = |key: &str| {
        wrathplus_ui::messages::by_key(key)
            .unwrap_or_else(|| panic!("{key} is not a catalog row"))
            .id
    };
    for (code, want) in [
        (mail_error::CANNOT_SEND_TO_SELF, 0x17bu16),
        (mail_error::NOT_ENOUGH_MONEY, 0x28),
        (mail_error::RECIPIENT_NOT_FOUND, 0x17c),
        (mail_error::NOT_YOUR_TEAM, 0x10d),
        (mail_error::INTERNAL_ERROR, 0x17d),
        (mail_error::TRIAL_ACCOUNT, 0x1e8),
        (mail_error::TOO_MANY_ATTACHMENTS, 0x1f1),
    ] {
        assert_eq!(id(mail_refusal(code)), want, "mail error {code}");
    }
    // `ja 0x4ad147` and the index table's `08` run: 7..13 and past 15 share the database error.
    for code in [7, 8, 13, 16, 99, u32::MAX] {
        assert_eq!(id(mail_refusal(code)), 0x17d, "mail error {code}");
    }
    // A success shows on the yellow info line.
    assert_eq!(id(MAIL_SENT_KEY), 0x17f);
    assert_eq!(
        wrathplus_ui::messages::by_key(MAIL_SENT_KEY)
            .expect("row")
            .kind,
        wrathplus_ui::messages::MsgKind::Info
    );
}

fn entry(
    message_id: u32,
    sender_guid: Option<u64>,
    checked: u32,
    expire_days: f32,
) -> MailListEntry {
    MailListEntry {
        message_id,
        message_type: mail_message_type::NORMAL,
        sender_guid,
        sender_id: None,
        subject: "s".into(),
        item_text_id: 0,
        stationery: 41,
        item: None,
        money: 0,
        cod: 0,
        checked,
        expire_days,
        mail_template_id: 0,
    }
}

#[test]
fn click_opens_and_reclick_keeps_rows() {
    let mut m = MailOpen::default();
    m.click(0x100);
    assert_eq!(m.mailbox, Some(0x100));
    assert!(m.show_requested);
    m.mails.push(entry(1, Some(0xA), 0, 30.0));
    m.last_list_query = Some(5.0);
    m.show_requested = false;
    // The same mailbox keeps the rows and throttle, and re-requests the show.
    m.click(0x100);
    assert_eq!(m.mails.len(), 1);
    assert_eq!(m.last_list_query, Some(5.0));
    assert!(m.show_requested);
    // A different mailbox resets the session.
    m.click(0x200);
    assert!(m.mails.is_empty());
    assert_eq!(m.last_list_query, None);
}

#[test]
fn clear_closes_the_window() {
    let mut m = MailOpen::default();
    m.click(0x1);
    m.mails.push(entry(1, Some(0xA), 0, 30.0));
    m.bodies.insert(7, Some("hi".into()));
    m.clear();
    assert!(m.mailbox.is_none());
    assert!(m.mails.is_empty());
    assert!(m.bodies.is_empty());
}
