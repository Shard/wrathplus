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
    player_level: Option<u32>,
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
                            link: template.and_then(|t| attachment_link(a, t, rolls, player_level)),
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

// Native 0x61e290; see docs/WRATH-INBOX-LINK.md. Never produce a plausible but
// incomplete link while instance-name metadata or the live player level is unavailable.
fn attachment_link(
    attachment: &benilla_protocol::wrath_mail::Attachment,
    template: &benilla_protocol::messages::ItemInfo,
    rolls: crate::items::RollCatalogs,
    player_level: Option<u32>,
) -> Option<String> {
    let level = player_level?;
    if attachment.entry == 0 {
        return None;
    }
    if attachment.random_property != 0 {
        rolls.props?.0.get(attachment.random_property)?;
    }
    let name = rolls.name(&template.name, attachment.random_property as u32);
    // Native table 0x00ad2a84: heirlooms (7) share artifacts' (6) color.
    let color = match template.quality {
        0 => "ff9d9d9d",
        2 => "ff1eff00",
        3 => "ff0070dd",
        4 => "ffa335ee",
        5 => "ffff8000",
        6 | 7 => "ffe6cc80",
        _ => "ffffffff",
    };
    let enchants = &attachment.enchantments;
    Some(format!(
        "|c{color}|Hitem:{}:{}:{}:{}:{}:0:{}:{}:{}|h[{name}]|h|r",
        attachment.entry as i32,
        enchants[0][0] as i32,
        enchants[2][0] as i32,
        enchants[3][0] as i32,
        enchants[4][0] as i32,
        attachment.random_property,
        attachment.suffix_factor as i32,
        level as i32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_links_wait_for_level_and_metadata_and_refresh_without_a_new_packet() {
        use benilla_protocol::wrath_mail::{Attachment, Inbox, Mail};
        let (tx, _rx) = crossbeam_channel::unbounded();
        let commands = NetCommands(tx);
        let names = NameCache::default();
        let mut items = Items::default();
        let mut mail = MailOpen::default();
        mail.mailbox = Some(42);
        let mut enchants = [[0; 3]; 7];
        for (i, row) in enchants.iter_mut().enumerate() {
            *row = [100 + i as u32, 200 + i as u32, 300 + i as u32];
        }
        enchants[0][0] = u32::MAX;
        let attachment = Attachment {
            slot: 15,
            guid_low: 1,
            entry: 2589,
            enchantments: enchants,
            random_property: 0,
            suffix_factor: u32::MAX,
            count: 1,
            charges: 0,
            max_durability: 0,
            durability: 0,
            unknown: 0,
        };
        mail.wrath_inbox = Some(Inbox {
            total_count: 1,
            mails: vec![Mail {
                id: 1,
                kind: 0,
                sender: 1,
                cod: 0,
                unknown: 0,
                stationery: 41,
                money: 0,
                checked: 0,
                expire_days: 1.0,
                template_id: 0,
                subject: String::new(),
                body: String::new(),
                attachments: vec![attachment.clone()],
            }],
        });
        let view = |mail: &MailOpen, items: &Items, level| {
            snapshot(
                mail,
                items,
                None,
                &names,
                &commands,
                crate::items::RollCatalogs::NONE,
                level,
            )
        };
        assert!(
            view(&mail, &items, Some(80)).unwrap().rows[0].attachments[0]
                .link
                .is_none()
        );
        let mut template = crate::items::test_template("Test");
        template.quality = 7;
        items.insert_template(2589, Some(template.clone()));
        assert!(view(&mail, &items, None).unwrap().rows[0].attachments[0]
            .link
            .is_none());
        assert_eq!(
            view(&mail, &items, Some(80)).unwrap().rows[0].attachments[0]
                .link
                .as_deref(),
            Some("|cffe6cc80|Hitem:2589:-1:102:103:104:0:0:-1:80|h[Test]|h|r")
        );
        let before = view(&mail, &items, Some(80));
        assert_ne!(before, view(&mail, &items, Some(81)));
        template.quality = 8;
        assert!(attachment_link(
            &attachment,
            &template,
            crate::items::RollCatalogs::NONE,
            Some(80)
        )
        .unwrap()
        .starts_with("|cffffffff"));
        let mut rolled = attachment.clone();
        rolled.random_property = -17;
        assert!(attachment_link(
            &rolled,
            &template,
            crate::items::RollCatalogs::NONE,
            Some(80)
        )
        .is_none());
        rolled.entry = 0;
        assert!(attachment_link(
            &rolled,
            &template,
            crate::items::RollCatalogs::NONE,
            Some(80)
        )
        .is_none());
        mail.clear();
        assert!(view(&mail, &items, Some(80)).is_none());
    }

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
            Some(80),
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
            crate::items::RollCatalogs::NONE,
            Some(80),
        )
        .is_none());
    }
}
