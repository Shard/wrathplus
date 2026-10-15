//! Build-12340 inbox view. Keep attachment slots separate from the 1.12 single-item row.
use super::MailInboxRow;

/// Resolved item metadata for one native attachment slot (1-based in Lua).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attachment {
    pub slot: u8,
    pub guid_low: u32,
    pub entry: u32,
    pub random_property: i32,
    pub suffix_factor: u32,
    pub count: u32,
    pub name: Option<String>,
    pub texture: Option<String>,
    pub quality: Option<u32>,
    /// Native nine-field hyperlink, resolved by the app; absent until metadata/level are known.
    pub link: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub header: MailInboxRow,
    pub attachments: Vec<Attachment>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inbox {
    pub total_count: u32,
    pub rows: Vec<Row>,
}

/// Wrath GetInboxItem(index, attachmentIndex): name, texture, count, quality, usable (stock MailFrame.lua:400).
pub(super) fn item(
    lua: &mlua::Lua,
    model: &super::Model,
    index: usize,
    slot: usize,
) -> mlua::Result<mlua::MultiValue> {
    use mlua::Value;
    let attachment = model
        .wrath_mail
        .as_ref()
        .and_then(|m| index.checked_sub(1).and_then(|i| m.rows.get(i)))
        .and_then(|r| {
            slot.checked_sub(1)
                .and_then(|s| r.attachments.iter().find(|a| usize::from(a.slot) == s))
        });
    let Some(a) = attachment else {
        return Ok(mlua::MultiValue::from_vec(vec![Value::Nil]));
    };
    let text = |s: &Option<String>| -> mlua::Result<Value> {
        Ok(match s {
            Some(s) => Value::String(lua.create_string(s)?),
            None => Value::Nil,
        })
    };
    Ok(mlua::MultiValue::from_vec(vec![
        text(&a.name)?,
        text(&a.texture)?,
        Value::Integer(i64::from(a.count)),
        Value::Integer(i64::from(a.quality.unwrap_or(0))),
        super::flag(super::super::item_stats::item_usable_by_id(model, a.entry)),
    ]))
}

/// Build-12340 0x56eca0; stock MailFrame.xml:353 uses the explicit attachment slot.
/// Unlike GetInboxItem, unavailable links return no Lua values (not one nil).
pub(in crate::script) fn install(lua: &mlua::Lua) -> mlua::Result<()> {
    lua.globals().set(
        "GetInboxItemLink",
        lua.create_function(|lua, args: mlua::MultiValue| {
            let index = super::super::binding_abi::number_arg(
                lua,
                args.front().cloned().unwrap_or(mlua::Value::Nil),
                "Usage: GetInboxItemLink(messageIndex, attachIndex)",
            )?
            .wrapping_sub(1) as u32;
            // Native subtracts before truncating the second argument, unlike the first.
            let slot = lua
                .coerce_number(args.get(1).cloned().unwrap_or(mlua::Value::Nil))?
                .map(|n| (n - 1.0) as i64 as i32)
                .unwrap_or(-1);
            let model = lua.app_data_ref::<super::Model>().expect("model");
            let attachment = model
                .wrath_mail
                .as_ref()
                .and_then(|m| m.rows.get(index as usize))
                .and_then(|r| {
                    if slot == -1 {
                        r.attachments
                            .iter()
                            .filter(|a| a.slot < 16 && a.entry != 0)
                            .min_by_key(|a| a.slot)
                    } else if (0..16).contains(&slot) {
                        r.attachments
                            .iter()
                            .find(|a| i32::from(a.slot) == slot && a.entry != 0)
                    } else {
                        None
                    }
                });
            let mut values = mlua::MultiValue::new();
            if let Some(link) = attachment.and_then(|a| a.link.as_ref()) {
                values.push_back(mlua::Value::String(lua.create_string(link)?));
            }
            Ok(values)
        })?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_link_argument_slot_and_zero_return_abi() {
        let mut script =
            crate::script::UiScript::new_with_dialect(crate::script::ScriptDialect::Wrath12340)
                .unwrap();
        script.set_wrath_mail(Some(Inbox {
            rows: vec![Row {
                attachments: vec![
                    Attachment {
                        slot: 11,
                        entry: 2589,
                        link: Some("late".into()),
                        ..Default::default()
                    },
                    Attachment {
                        slot: 0,
                        entry: 0,
                        link: Some("empty".into()),
                        ..Default::default()
                    },
                    Attachment {
                        slot: 2,
                        entry: 2590,
                        link: Some("first".into()),
                        ..Default::default()
                    },
                    Attachment {
                        slot: 4,
                        entry: 2591,
                        ..Default::default()
                    },
                    Attachment {
                        slot: 16,
                        entry: 2592,
                        link: Some("outside".into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }));
        for args in [
            "1",
            "'1.9'",
            "1,nil",
            "1,false",
            "1,{}",
            "1,'junk'",
            "1,0",
            "1,-0.5",
            "1,4294967296",
        ] {
            let link: String = script
                .lua()
                .load(format!("return GetInboxItemLink({args})"))
                .eval()
                .unwrap();
            assert_eq!(link, "first", "{args}");
        }
        for args in ["1,12", "1,'12.9'", "1.9,12", "1,13.0-0.1"] {
            let link: String = script
                .lua()
                .load(format!("return GetInboxItemLink({args})"))
                .eval()
                .unwrap();
            assert_eq!(link, "late", "{args}");
        }
        for args in ["0", "2", "-1", "1,1", "1,5", "1,17", "1,-1", "1,0.5"] {
            let values: mlua::MultiValue = script
                .lua()
                .load(format!("return GetInboxItemLink({args})"))
                .eval()
                .unwrap();
            assert!(values.is_empty(), "{args}");
        }
        for args in ["", "nil", "false", "{}", "'junk'"] {
            let error = script
                .lua()
                .load(format!("return GetInboxItemLink({args})"))
                .eval::<mlua::MultiValue>()
                .unwrap_err();
            assert!(error
                .to_string()
                .contains("Usage: GetInboxItemLink(messageIndex, attachIndex)"));
        }
        script.set_wrath_mail(None);
        assert!(script
            .lua()
            .load("return GetInboxItemLink(1)")
            .eval::<mlua::MultiValue>()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn first_occupied_unresolved_link_does_not_skip_to_resolved_slot() {
        let mut script =
            crate::script::UiScript::new_with_dialect(crate::script::ScriptDialect::Wrath12340)
                .unwrap();
        script.set_wrath_mail(Some(Inbox {
            rows: vec![Row {
                attachments: vec![
                    Attachment {
                        slot: 0,
                        entry: 1,
                        ..Default::default()
                    },
                    Attachment {
                        slot: 1,
                        entry: 2,
                        link: Some("ready".into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }));
        assert!(script
            .lua()
            .load("return GetInboxItemLink(1)")
            .eval::<mlua::MultiValue>()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn native_inbox_exposes_total_and_attachment_slot_without_stack_narrowing() {
        let mut script = crate::script::UiScript::new().unwrap();
        script.model_mut().dialect = crate::script::ScriptDialect::Wrath12340;
        script.set_wrath_mail(Some(Inbox {
            total_count: 70,
            rows: vec![Row {
                header: MailInboxRow {
                    subject: "subject".into(),
                    body: Some("inline".into()),
                    item_count: 1,
                    package_icon: Some("NativeIcon".into()),
                    ..Default::default()
                },
                attachments: vec![Attachment {
                    slot: 11,
                    entry: 2589,
                    count: 1000,
                    name: Some("Linen".into()),
                    ..Default::default()
                }],
            }],
        }));
        let counts: (u32, u32) = script
            .lua()
            .load("return GetInboxNumItems()")
            .eval()
            .unwrap();
        assert_eq!(counts, (1, 70));
        let result: (String, mlua::Value, u32, u32, mlua::Value) = script
            .lua()
            .load("return GetInboxItem(1,12)")
            .eval()
            .unwrap();
        assert_eq!((result.0, result.2), ("Linen".into(), 1000));
        let (icon, quantity): (String, u32) = script.lua().load(
            "local icon,_,_,_,_,_,_,_,_,_,_,_,_,quantity = GetInboxHeaderInfo(1); return icon,quantity"
        ).eval().unwrap();
        assert_eq!((icon, quantity), ("NativeIcon".into(), 1000));
        let body: String = script.lua().load("return GetInboxText(1)").eval().unwrap();
        assert_eq!(body, "inline");
        assert!(script
            .lua()
            .load("return GetInboxItem(1,13)")
            .eval::<mlua::Value>()
            .unwrap()
            .is_nil());
        script.set_wrath_mail(None);
        assert_eq!(
            script
                .lua()
                .load("return GetInboxNumItems()")
                .eval::<(u32, u32)>()
                .unwrap(),
            (0, 0)
        );
    }
}
