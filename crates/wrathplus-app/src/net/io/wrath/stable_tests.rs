use super::*;
#[test]
fn five_existing_stable_commands_use_native_transport() {
    for (command, opcode, pet) in [
        (
            ClientCommand::ListStabledPets {
                npc: 0x0102030405060708,
            },
            0x26f,
            None,
        ),
        (
            ClientCommand::StablePet {
                npc: 0x0102030405060708,
            },
            0x270,
            None,
        ),
        (
            ClientCommand::UnstablePet {
                npc: 0x0102030405060708,
                pet_number: u32::MAX,
            },
            0x271,
            Some(u32::MAX),
        ),
        (
            ClientCommand::BuyStableSlot {
                npc: 0x0102030405060708,
            },
            0x272,
            None,
        ),
        (
            ClientCommand::StableSwapPet {
                npc: 0x0102030405060708,
                pet_number: 0,
            },
            0x275,
            Some(0),
        ),
    ] {
        assert_eq!(route(&command), WrathRoute::NativeSession);
        let mut expected = vec![8, 7, 6, 5, 4, 3, 2, 1];
        if let Some(pet) = pet {
            expected.extend(pet.to_le_bytes());
        }
        assert_eq!(session_packet(command), (opcode, expected));
    }
}
