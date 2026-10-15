use super::*;

#[test]
fn next_mail_query_is_native_and_has_no_vanilla_payload() {
    assert_eq!(
        route(&ClientCommand::QueryNextMailTime),
        WrathRoute::NativeSession
    );
    assert_eq!(
        session_packet(ClientCommand::QueryNextMailTime),
        (0x284, vec![])
    );
    assert_eq!(
        route(&ClientCommand::BattlefieldStatusRequest),
        WrathRoute::DeferredReadOnlyQuery
    );
    // Do not accidentally enable mailbox mutations until their native layouts are supported.
    assert_eq!(
        route(&ClientCommand::MailTakeItem {
            mailbox: 42,
            mail_id: 3
        }),
        WrathRoute::DisabledGameAction
    );
}
