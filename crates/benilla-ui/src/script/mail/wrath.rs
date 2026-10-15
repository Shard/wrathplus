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

#[cfg(test)]
mod tests {
    use super::*;
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
