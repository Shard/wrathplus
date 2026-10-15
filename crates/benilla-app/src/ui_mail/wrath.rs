//! Native mailbox snapshot: inline text and attachment slots are never narrowed to Vanilla mail.
use super::*;
use benilla_ui::script::{WrathMailAttachment, WrathMailInbox, WrathMailRow};

pub(super) fn snapshot(
    mail: &MailOpen,
    items: &Items,
    icons: Option<&ItemDisplays>,
    names: &NameCache,
    commands: &NetCommands,
    rolls: crate::items::RollCatalogs,
) -> Option<WrathMailInbox> {
    mail.mailbox?;
    let inbox = mail.wrath_inbox.as_ref()?;
    Some(WrathMailInbox {
        total_count: inbox.total_count,
        rows: inbox
            .mails
            .iter()
            .map(|row| {
                let attachments: Vec<_> = row
                    .attachments
                    .iter()
                    .map(|a| {
                        let template = items.template(a.entry, 0, commands);
                        let texture = template
                            .and_then(|t| icons.and_then(|i| i.catalog.get(t.display_info_id)))
                            .and_then(|d| d.icon.clone());
                        WrathMailAttachment {
                            slot: a.slot,
                            guid_low: a.guid_low,
                            entry: a.entry,
                            random_property: a.random_property,
                            suffix_factor: a.suffix_factor,
                            count: a.count,
                            name: template.map(|t| rolls.name(&t.name, a.random_property as u32)),
                            texture,
                            quality: template.map(|t| t.quality),
                            link: None,
                        }
                    })
                    .collect();
                let returned = row.checked & CHECKED_RETURNED != 0;
                let header = MailInboxRow {
                    package_icon: attachments.first().and_then(|a| a.texture.clone()),
                    stationery_icon: Some(
                        if row.stationery == STATIONERY_GM {
                            STATIONERY_ICON_GM
                        } else {
                            STATIONERY_ICON_NORMAL
                        }
                        .into(),
                    ),
                    sender: (row.kind == 0)
                        .then(|| names.resolve(row.sender, commands).map(str::to_owned))
                        .flatten(),
                    subject: row.subject.clone(),
                    money: row.money,
                    cod: row.cod,
                    days_left: row.expire_days,
                    item_count: attachments.len() as u32,
                    was_read: row.checked & CHECKED_READ != 0,
                    was_returned: returned,
                    text_created: row.checked & CHECKED_COPIED != 0,
                    can_reply: row.kind == 0 && !returned,
                    is_gm: row.stationery == STATIONERY_GM,
                    body: Some(row.body.clone()),
                    has_body: !row.body.is_empty(),
                    stationery_texture: benilla_formats::StationeryCatalog::DEFAULT_TEXTURE.into(),
                    can_delete: row.money == 0 && attachments.is_empty(),
                    ..Default::default()
                };
                WrathMailRow {
                    header,
                    attachments,
                }
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_snapshot_keeps_inline_body_total_slots_and_full_stacks() {
        use benilla_protocol::wrath_mail::{Attachment, Inbox, Mail};
        let (tx, _rx) = crossbeam_channel::unbounded();
        let commands = NetCommands(tx);
        let mut mail = MailOpen::default();
        mail.mailbox = Some(42);
        mail.wrath_inbox = Some(Inbox {
            total_count: 70,
            mails: vec![Mail {
                id: 10,
                kind: 0,
                sender: 12,
                cod: 0,
                unknown: 0,
                stationery: 41,
                money: 500,
                checked: CHECKED_READ,
                expire_days: 10.0,
                template_id: 0,
                subject: "Native".into(),
                body: "Inline body".into(),
                attachments: (0..12)
                    .map(|slot| Attachment {
                        slot,
                        guid_low: 100 + u32::from(slot),
                        entry: 2589,
                        enchantments: [[0; 3]; 7],
                        random_property: -17,
                        suffix_factor: 30,
                        count: 1000,
                        charges: 0,
                        max_durability: 0,
                        durability: 0,
                        unknown: 0,
                    })
                    .collect(),
            }],
        });
        let view = snapshot(
            &mail,
            &Items::default(),
            None,
            &NameCache::default(),
            &commands,
            crate::items::RollCatalogs::NONE,
        )
        .unwrap();
        assert_eq!(view.total_count, 70);
        assert_eq!(view.rows[0].header.body.as_deref(), Some("Inline body"));
        assert_eq!(view.rows[0].header.item_count, 12);
        assert_eq!(view.rows[0].attachments[11].slot, 11);
        assert_eq!(view.rows[0].attachments[11].guid_low, 111);
        assert_eq!(view.rows[0].attachments[11].count, 1000);
        assert_eq!(view.rows[0].attachments[11].random_property, -17);
        assert_eq!(view.rows[0].header.item_id, 0); // no single-item projection
        mail.clear();
        assert!(snapshot(
            &mail,
            &Items::default(),
            None,
            &NameCache::default(),
            &commands,
            crate::items::RollCatalogs::NONE
        )
        .is_none());
    }
}
