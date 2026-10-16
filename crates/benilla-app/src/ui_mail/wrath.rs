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
                    kind: row.kind,
                    sender_guid: row.sender,
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
    let enchants = &attachment.enchantments;
    Some(crate::items::wrath_item_link(
        attachment.entry,
        &name,
        template.quality,
        [
            enchants[0][0],
            enchants[2][0],
            enchants[3][0],
            enchants[4][0],
        ],
        attachment.random_property,
        attachment.suffix_factor,
        level,
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

/// An inbox action a Lua row pick asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    /// The 1-based attachment slot `TakeInboxItem` named; `None` takes the first remaining one.
    TakeItem(Option<u32>),
    TakeMoney,
    Delete,
    Return,
    CreateTextItem,
}

/// Build-12340 inbox actions by native inbox row (1-based): the wire wants the attachment's low
/// GUID, the sender and the template the legacy commands lack (AC `MailHandler.cpp`). A take names
/// the attachment in the slot `TakeInboxItem` gave (12340 `0x5711c0`/`0x56d920`, 1-based in Lua,
/// 0-based on the wire), or the row's first remaining one; an empty named slot sends nothing.
pub(super) fn native_action(
    inbox: &benilla_protocol::wrath_mail::Inbox,
    mailbox: u64,
    index_1based: u32,
    action: Action,
) -> Option<ClientCommand> {
    use benilla_protocol::wrath_mail_mutations::Command as M;
    let row = inbox.mails.get(index_1based.checked_sub(1)? as usize)?;
    let mail_id = row.id;
    Some(ClientCommand::NativeMailMutation(match action {
        Action::TakeItem(slot) => M::TakeItem {
            mailbox,
            mail_id,
            item_low_guid: match slot {
                Some(slot) => {
                    let slot = u8::try_from(slot.checked_sub(1)?).ok()?;
                    row.attachments.iter().find(|a| a.slot == slot)?.guid_low
                }
                None => row.attachments.iter().min_by_key(|a| a.slot)?.guid_low,
            },
        },
        Action::TakeMoney => M::TakeMoney { mailbox, mail_id },
        Action::Delete => M::Delete {
            mailbox,
            mail_id,
            template: row.template_id,
        },
        Action::Return => M::Return {
            mailbox,
            mail_id,
            sender: row.sender,
        },
        Action::CreateTextItem => M::CreateTextItem { mailbox, mail_id },
    }))
}

/// `CMSG_MAIL_MARK_AS_READ` for an unread native row, marking it read locally: no reply comes.
pub(super) fn mark_read(
    inbox: &mut benilla_protocol::wrath_mail::Inbox,
    mailbox: u64,
    index_1based: u32,
) -> Option<ClientCommand> {
    let row = inbox.mails.get_mut(index_1based.checked_sub(1)? as usize)?;
    if row.checked & CHECKED_READ != 0 {
        return None;
    }
    row.checked |= CHECKED_READ;
    Some(ClientCommand::NativeMailMutation(
        benilla_protocol::wrath_mail_mutations::Command::MarkRead {
            mailbox,
            mail_id: row.id,
        },
    ))
}

/// `CMSG_SEND_MAIL` in this build: attachments are `(slot, GUID)` pairs, the package word and the
/// two trailing constants are 0 (AC reads and ignores them).
pub(super) fn send(
    mailbox: u64,
    receiver: String,
    subject: String,
    body: String,
    stationery: u32,
    item_guid: Option<u64>,
    money: u32,
    cod: u32,
) -> ClientCommand {
    use benilla_protocol::wrath_mail_mutations::{Attachment, Command, SendMail};
    ClientCommand::NativeMailMutation(Command::Send(SendMail {
        mailbox,
        receiver: receiver.into_bytes(),
        subject: subject.into_bytes(),
        body: body.into_bytes(),
        stationery,
        unknown_word: 0,
        attachments: item_guid
            .map(|item| vec![Attachment { slot: 0, item }])
            .unwrap_or_default(),
        money,
        cod,
        unknown_guid: 0,
        unknown_byte: 0,
    }))
}

#[cfg(test)]
mod action_tests {
    use super::*;
    use benilla_protocol::wrath_mail::{Attachment, Inbox, Mail};
    use benilla_protocol::wrath_mail_mutations::Command as M;

    fn inbox() -> Inbox {
        Inbox {
            total_count: 1,
            mails: vec![Mail {
                id: 5,
                kind: 0,
                sender: 0x42,
                cod: 0,
                unknown: 0,
                stationery: 41,
                money: 10,
                checked: 0,
                expire_days: 30.0,
                template_id: 3,
                subject: "s".into(),
                body: "b".into(),
                attachments: vec![Attachment {
                    slot: 4,
                    guid_low: 0x99,
                    entry: 2589,
                    enchantments: [[0; 3]; 7],
                    random_property: 0,
                    suffix_factor: 0,
                    count: 1,
                    charges: 0,
                    max_durability: 0,
                    durability: 0,
                    unknown: 0,
                }],
            }],
        }
    }

    #[test]
    fn row_picks_carry_the_native_ids_the_wire_wants() {
        let inbox = inbox();
        let native = |action| match native_action(&inbox, 7, 1, action) {
            Some(ClientCommand::NativeMailMutation(command)) => command,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            native(Action::TakeItem(None)),
            M::TakeItem {
                mailbox: 7,
                mail_id: 5,
                item_low_guid: 0x99
            }
        );
        assert_eq!(
            native(Action::TakeItem(Some(5))),
            M::TakeItem {
                mailbox: 7,
                mail_id: 5,
                item_low_guid: 0x99
            },
            "Lua slot 5 is wire slot 4"
        );
        assert!(
            native_action(&inbox, 7, 1, Action::TakeItem(Some(1))).is_none(),
            "an empty named slot sends nothing"
        );
        assert_eq!(
            native(Action::Delete),
            M::Delete {
                mailbox: 7,
                mail_id: 5,
                template: 3
            }
        );
        assert_eq!(
            native(Action::Return),
            M::Return {
                mailbox: 7,
                mail_id: 5,
                sender: 0x42
            }
        );
        assert_eq!(
            native(Action::TakeMoney),
            M::TakeMoney {
                mailbox: 7,
                mail_id: 5
            }
        );
        assert!(native_action(&inbox, 7, 2, Action::TakeMoney).is_none());
        assert!(native_action(&inbox, 7, 0, Action::TakeMoney).is_none());
        let mut empty = inbox.clone();
        empty.mails[0].attachments.clear();
        assert!(native_action(&empty, 7, 1, Action::TakeItem(None)).is_none());
    }

    #[test]
    fn an_open_marks_read_once() {
        let mut inbox = inbox();
        assert!(matches!(
            mark_read(&mut inbox, 7, 1),
            Some(ClientCommand::NativeMailMutation(M::MarkRead {
                mailbox: 7,
                mail_id: 5
            }))
        ));
        assert!(mark_read(&mut inbox, 7, 1).is_none());
    }
}
