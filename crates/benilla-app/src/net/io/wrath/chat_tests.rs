use super::*;
use benilla_protocol::wrath_chat::{encode, Command};
#[test]
fn native_chat_routes_without_shared_type_mapping() {
    for kind in [1, 6, 7, 0x11, 0x33] {
        let command = Command {
            chat_type: kind,
            language: u32::MAX,
            target: if matches!(kind, 7 | 0x11) {
                Some(vec![255])
            } else {
                None
            },
            text: vec![254],
        };
        assert_eq!(
            route(&ClientCommand::NativeChatCommand(command.clone())),
            WrathRoute::NativeChat
        );
        let mut expected = kind.to_le_bytes().to_vec();
        expected.extend([255; 4]);
        if matches!(kind, 7 | 0x11) {
            expected.extend([255, 0]);
        }
        expected.extend([254, 0]);
        assert_eq!(encode(&command).unwrap(), (0x95, expected));
    }
    assert!(encode(&Command {
        chat_type: 7,
        language: 0,
        target: None,
        text: vec![]
    })
    .is_err());
    assert!(encode(&Command {
        chat_type: 1,
        language: 0,
        target: None,
        text: vec![0]
    })
    .is_err());
}
