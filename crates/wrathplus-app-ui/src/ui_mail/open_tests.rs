//! Server open → real dispatcher → feed → Lua → list/read producers, without a socket.
use super::*;
use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use wrathplus_protocol::{SessionEnd, SessionEvent};

fn fixture() -> (App, crossbeam_channel::Receiver<ClientCommand>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.init_resource::<MailOpen>()
        .init_resource::<MailPending>()
        .init_resource::<crate::ui_items::EquipErrors>()
        .init_resource::<crate::net::GuidIndex>()
        .init_resource::<Items>()
        .init_resource::<NameCache>()
        .init_resource::<crate::app_core::chat::ChatLog>()
        .init_resource::<crate::sound::MessageSounds>()
        .insert_resource(Time::<()>::default())
        .insert_resource(NetCommands(tx))
        .add_systems(Update, (feed_mail, drain_mail).chain());
    net::register(&mut app);
    let script = UiScript::new().unwrap();
    script
        .run(
            r#"
        shows, closes, updates = 0, 0, 0
        local f = CreateFrame("Frame")
        f:RegisterEvent("MAIL_SHOW")
        f:RegisterEvent("MAIL_CLOSED")
        f:RegisterEvent("MAIL_INBOX_UPDATE")
        f:SetScript("OnEvent", function(_, event)
            if event == "MAIL_SHOW" then shows = shows + 1; CheckInbox()
            elseif event == "MAIL_CLOSED" then closes = closes + 1
            else updates = updates + 1 end
        end)
    "#,
        )
        .unwrap();
    app.insert_non_send_resource(script);
    (app, rx)
}

fn show(app: &mut App, guid: u64) {
    let result = WrathEventAdapter::new()
        .dispatch(0x297, &guid.to_le_bytes(), 0)
        .unwrap();
    let WrathDispatch::Handled { session_events, .. } = result else {
        panic!("show not decoded")
    };
    crate::net::handlers::dispatch(app.world_mut(), session_events);
    app.update();
}

fn number(app: &mut App, expression: &str) -> i64 {
    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .eval(expression)
        .unwrap()
}

#[test]
fn server_mailbox_open_dispatches_through_feed_list_read_and_close() {
    const MAILBOX: u64 = 0xF130_0070_2A00_0042;
    let (mut app, rx) = fixture();
    show(&mut app, MAILBOX);
    assert_eq!(number(&mut app, "return shows"), 1);
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientCommand::GetMailList { mailbox: MAILBOX })
    ));
    assert!(rx.is_empty());
    let inbox = wrathplus_protocol::wrath_mail::Inbox {
        total_count: 1,
        mails: vec![wrathplus_protocol::wrath_mail::Mail {
            id: 77,
            kind: 0,
            sender: 0,
            cod: 0,
            unknown: 0,
            stationery: 41,
            money: 0,
            checked: 0,
            expire_days: 30.0,
            template_id: 0,
            subject: "subject".into(),
            body: "inline letter".into(),
            attachments: vec![],
        }],
    };
    crate::net::handlers::dispatch(app.world_mut(), vec![SessionEvent::WrathMailList { inbox }]);
    app.update();
    assert_eq!(number(&mut app, "return updates"), 1);
    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .run(r#"assert(GetInboxNumItems() == 1); assert(GetInboxText(1) == "inline letter")"#)
        .unwrap();
    app.update();
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientCommand::NativeMailMutation(
            wrathplus_protocol::wrath_mail_mutations::Command::MarkRead {
                mailbox: MAILBOX,
                mail_id: 77
            }
        ))
    ));
    // Re-show retains rows and respects CheckInbox's throttle.
    show(&mut app, MAILBOX);
    assert_eq!(number(&mut app, "return shows"), 2);
    assert_eq!(number(&mut app, "return GetInboxNumItems()"), 1);
    assert!(rx.is_empty());
    // Switching mailbox drops the prior inbox and throttle, then lists the new GUID.
    show(&mut app, MAILBOX + 1);
    assert!(
        matches!(rx.try_recv(), Ok(ClientCommand::GetMailList { mailbox }) if mailbox == MAILBOX + 1)
    );
    assert_eq!(number(&mut app, "return GetInboxNumItems()"), 0);
    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .run("CloseMail()")
        .unwrap();
    app.update();
    app.update();
    assert_eq!(number(&mut app, "return closes"), 1);
    assert!(app.world().resource::<MailOpen>().mailbox.is_none());
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: SessionEnd::Lost,
        }],
    );
    assert!(app.world().resource::<MailOpen>().wrath_inbox.is_none());
}
