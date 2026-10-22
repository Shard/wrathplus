//! Build-12340 mirror-image query/reply, capture only; no rendering inference.
//! AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03 SpellHandler.cpp
//! HandleMirrorImageDataRequest writes a fixed 68-byte reply, including zero creature tail.
//! Wow.exe SHA256 c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c:
//! 00716f10 sends 0401 + unpacked GUID through 0047b100; 00742220 registers
//! 0402 to 007324b0 (GUID reader 0047b400), then 00730290 reads display u32,
//! race u8, and for nonzero race seven more bytes, guild u32 and eleven displays.
//! Its race-zero path does not interpret the tail; retain the server's raw fields.
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub guid: u64,
    pub display_id: u32,
    pub race: u8,
    pub gender: u8,
    pub class: u8,
    /// Skin, face, hair style, hair color, facial hair in wire order.
    pub appearance: [u8; 5],
    pub guild_id: u32,
    /// Head, shoulders, body, chest, waist, legs, feet, wrists, hands, back, tabard.
    pub equipment_display_ids: [u32; 11],
}

pub fn encode_query(guid: u64) -> (u32, Vec<u8>) {
    (0x0401, guid.to_le_bytes().to_vec())
}

pub fn decode(body: &[u8]) -> io::Result<Packet> {
    let bytes: &[u8; 68] = body.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "mirror image requires exactly 68 bytes",
        )
    })?;
    Ok(Packet {
        guid: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
        display_id: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        race: bytes[12],
        gender: bytes[13],
        class: bytes[14],
        appearance: bytes[15..20].try_into().unwrap(),
        guild_id: u32::from_le_bytes(bytes[20..24].try_into().unwrap()),
        equipment_display_ids: std::array::from_fn(|i| {
            u32::from_le_bytes(bytes[24 + i * 4..28 + i * 4].try_into().unwrap())
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_fixture_preserves_each_raw_field_and_display_order() {
        let body = [
            8, 7, 6, 5, 4, 3, 2, 1, 0x78, 0x56, 0x34, 0x12, 255, 254, 253, 252, 251, 250, 249, 248,
            4, 3, 2, 1, 255, 255, 255, 255, 0, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0,
            0, 0, 6, 0, 0, 0, 7, 0, 0, 0, 8, 0, 0, 0, 9, 0, 0, 0, 10, 0, 0, 0,
        ];
        assert_eq!(
            decode(&body).unwrap(),
            Packet {
                guid: 0x0102030405060708,
                display_id: 0x12345678,
                race: 255,
                gender: 254,
                class: 253,
                appearance: [252, 251, 250, 249, 248],
                guild_id: 0x01020304,
                equipment_display_ids: [u32::MAX, 0, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            }
        );
        assert_eq!(
            encode_query(0x0102030405060708),
            (0x401, vec![8, 7, 6, 5, 4, 3, 2, 1])
        );
        assert_eq!(encode_query(0), (0x401, vec![0; 8]));
        assert_eq!(encode_query(u64::MAX), (0x401, vec![255; 8]));
    }

    #[test]
    fn race_zero_does_not_discard_tail_or_fabricate_appearance() {
        let mut body = [255; 68];
        body[12] = 0;
        let packet = decode(&body).unwrap();
        assert_eq!(packet.race, 0);
        assert_eq!(packet.appearance, [255; 5]);
        assert_eq!(packet.equipment_display_ids, [u32::MAX; 11]);
        assert_eq!(decode(&[0; 68]).unwrap().equipment_display_ids, [0; 11]);
    }

    #[test]
    fn rejects_every_truncation_and_trailing_bytes() {
        for len in 0..68 {
            assert!(decode(&vec![0; len]).is_err());
        }
        for len in [69, 72, 136] {
            assert!(decode(&vec![0; len]).is_err());
        }
    }
}
