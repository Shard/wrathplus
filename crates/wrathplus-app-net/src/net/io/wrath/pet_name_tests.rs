use super::*;
#[test]
fn existing_query_command_uses_native_number_and_full_guid_route() {
    let command = ClientCommand::PetNameQuery {
        pet_number: 0x12345678,
        guid: 0xf140112233445566,
    };
    assert!(matches!(route(&command), WrathRoute::NativePetNameQuery));
    let (op, body) =
        wrathplus_protocol::wrath_pet_name::encode_query(0x12345678, 0xf140112233445566);
    assert_eq!(op, 0x52);
    assert_eq!(
        body,
        [0x78, 0x56, 0x34, 0x12, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x40, 0xf1]
    );
}
