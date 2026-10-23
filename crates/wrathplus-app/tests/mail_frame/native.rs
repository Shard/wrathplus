//! The mail kit's inbox fixtures, and their native 3.3.5 inbox, which the bindings read.

use wrathplus_ui::script::{MailInboxRow, MailState};

/// A 1.12-shaped fixture as the native inbox the 3.3.5 bindings read: each row's one item becomes
/// its attachment slot 0.
pub fn native(state: MailState) -> wrathplus_ui::script::WrathMailInbox {
    use wrathplus_ui::script::{WrathMailAttachment, WrathMailRow};
    wrathplus_ui::script::WrathMailInbox {
        total_count: state.inbox.len() as u32,
        rows: state
            .inbox
            .into_iter()
            .map(|header| WrathMailRow {
                attachments: (header.item_id != 0)
                    .then(|| WrathMailAttachment {
                        slot: 0,
                        entry: header.item_id,
                        count: header.item_count,
                        name: header.item_name.clone(),
                        texture: header.item_texture.clone(),
                        quality: header.item_quality,
                        ..Default::default()
                    })
                    .into_iter()
                    .collect(),
                header,
                kind: 0,
                sender_guid: 0,
            })
            .collect(),
    }
}

/// One inbox row with the fields the window paints.
pub fn row(sender: &str, subject: &str, was_read: bool, item_id: u32, cod: u32) -> MailInboxRow {
    MailInboxRow {
        package_icon: (item_id != 0).then(|| "Interface\\Icons\\INV_Misc_Bag_08".to_string()),
        stationery_icon: Some("Interface\\Icons\\INV_Misc_Note_01".to_string()),
        sender: Some(sender.to_string()),
        subject: subject.to_string(),
        money: 0,
        cod,
        days_left: 29.0,
        item_count: if item_id != 0 { 2 } else { 0 },
        was_read,
        was_returned: false,
        text_created: false,
        can_reply: true,
        is_gm: false,
        body: Some("body".into()),
        stationery_texture: "STATIONERYTEST".into(),
        is_invoice: false,
        invoice: None,
        has_body: true,
        item_id,
        item_name: (item_id != 0).then(|| "Linen Cloth".to_string()),
        item_texture: (item_id != 0).then(|| "Interface\\Icons\\INV_Fabric_Linen_01".to_string()),
        item_quality: (item_id != 0).then_some(1),
        can_delete: false,
        item_random_property_id: 0,
    }
}

/// A one-page inbox (2 mails).
pub fn small_inbox() -> MailState {
    MailState {
        inbox: vec![
            row("Thrall", "Warchief's orders", false, 2589, 0),
            row("Jaina", "A letter", true, 0, 0),
        ],
    }
}
