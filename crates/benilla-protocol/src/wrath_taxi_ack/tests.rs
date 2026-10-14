use super::*;
#[test]
fn completion_has_one_packed_guid_native_info_then_counter() {
    let body = encode(0x0500_0001, [1.0, 2.0, 3.0], 0.5, 99, 77).unwrap();
    let counter = body.len() - 4;
    assert_eq!(&body[counter..], &77u32.to_le_bytes());
    let decoded = OutboundMovement::from_bytes(&body[..counter]).unwrap();
    assert_eq!(decoded.mover_guid, 0x0500_0001);
    assert_eq!(decoded.movement.flags, 0);
    assert_eq!(decoded.movement.flags2, 0);
    assert_eq!(decoded.movement.timestamp, 99);
    assert_eq!(decoded.movement.position.z, 3.0);
    assert_eq!(decoded.movement.position.orientation, 0.5);
    assert!(encode(1, [f32::NAN, 0.0, 0.0], 0.0, 0, 0).is_err());
}
