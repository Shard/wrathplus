use super::*;

#[test]
fn accepting_innkeeper_confirmation_sends_native_full_guid() {
    let binder = 0xf130_0012_3400_5678u64;
    let command = ClientCommand::BinderActivate { binder };
    assert_eq!(route(&command), WrathRoute::NativeSession);
    assert_eq!(
        session_packet(command),
        (0x1b5, binder.to_le_bytes().to_vec())
    );
}
