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
}

#[test]
fn inbox_request_is_native_guid_without_enabling_mutations() {
    let mailbox = 0xf110_0000_1234_5678;
    assert_eq!(
        route(&ClientCommand::GetMailList { mailbox }),
        WrathRoute::NativeSession
    );
    assert_eq!(
        session_packet(ClientCommand::GetMailList { mailbox }),
        (0x23a, mailbox.to_le_bytes().to_vec())
    );
}
