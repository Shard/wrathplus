use super::*;

#[test]
fn flightmaster_commands_use_native_layout_without_vanilla_express_fare() {
    let guid = 0xf130_0012_3400_5678u64;
    let pair = [
        guid.to_le_bytes().as_slice(),
        2u32.to_le_bytes().as_slice(),
        300u32.to_le_bytes().as_slice(),
    ]
    .concat();
    let express = [
        guid.to_le_bytes().as_slice(),
        3u32.to_le_bytes().as_slice(),
        2u32.to_le_bytes().as_slice(),
        300u32.to_le_bytes().as_slice(),
        448u32.to_le_bytes().as_slice(),
    ]
    .concat();
    for (command, opcode, body) in [
        (
            ClientCommand::TaxiNodeStatusQuery { guid },
            0x1aa,
            guid.to_le_bytes().to_vec(),
        ),
        (
            ClientCommand::TaxiQueryNodes { guid },
            0x1ac,
            guid.to_le_bytes().to_vec(),
        ),
        (
            ClientCommand::ActivateTaxi {
                guid,
                source_node: 2,
                dest_node: 300,
            },
            0x1ad,
            pair,
        ),
        (
            ClientCommand::ActivateTaxiExpress {
                guid,
                nodes: vec![2, 300, 448],
            },
            0x312,
            express,
        ),
    ] {
        assert_eq!(route(&command), WrathRoute::NativeSession);
        assert_eq!(session_packet(command), (opcode, body));
    }
}
