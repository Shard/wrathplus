//! Build-12340 appearance traffic: the worn-display toggles, the barber chair and titles earned.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03 `CharacterPackets.cpp`
//! (`ShowingHelm`/`ShowingCloak::Read`, a `bool`), `CharacterHandler.cpp` (`HandleAlterAppearance`,
//! four `uint32`; `SMSG_BARBER_SHOP_RESULT` `uint32` 0 done, 1 no money, 2 refused),
//! `GameObject.cpp` (`SMSG_ENABLE_BARBER_SHOP`, empty) and `Player.cpp` (`SetTitle`:
//! `SMSG_TITLE_EARNED` `uint32` bit index, `uint32` 1 earned / 0 lost). Strict framing.
use std::io;

pub const OPCODES: [u16; 3] = [0x0373, 0x0427, 0x0428];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// A title's `CharTitles.dbc` bit index, earned or lost.
    TitleEarned {
        bit_index: u32,
        earned: bool,
    },
    /// The barber chair is in use: open the barber window.
    EnableBarberShop,
    BarberShopResult {
        result: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CMSG_SHOWING_HELM`: show (true) or hide the helm.
    ShowingHelm { show: bool },
    /// `CMSG_SHOWING_CLOAK`.
    ShowingCloak { show: bool },
    /// `CMSG_ALTER_APPEARANCE`: `BarberShopStyle.dbc` ids; skin 0 leaves it.
    AlterAppearance {
        hair: u32,
        hair_color: u32,
        facial_hair: u32,
        skin_color: u32,
    },
}

fn word(body: &[u8], at: usize) -> io::Result<u32> {
    body.get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().expect("four bytes")))
        .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let (packet, len) = match opcode {
        0x0373 => {
            let earned = match word(body, 4)? {
                0 => false,
                1 => true,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "title earned flag is 0 or 1",
                    ))
                }
            };
            (
                Packet::TitleEarned {
                    bit_index: word(body, 0)?,
                    earned,
                },
                8,
            )
        }
        0x0427 => (Packet::EnableBarberShop, 0),
        0x0428 => (
            Packet::BarberShopResult {
                result: word(body, 0)?,
            },
            4,
        ),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "appearance opcode",
            ))
        }
    };
    if body.len() != len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "appearance packet extent",
        ));
    }
    Ok(packet)
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    match command {
        Command::ShowingHelm { show } => (0x02b9, vec![u8::from(*show)]),
        Command::ShowingCloak { show } => (0x02ba, vec![u8::from(*show)]),
        Command::AlterAppearance {
            hair,
            hair_color,
            facial_hair,
            skin_color,
        } => (
            0x0426,
            [hair, hair_color, facial_hair, skin_color]
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_decode_strictly() {
        let title = [7u32.to_le_bytes(), 1u32.to_le_bytes()].concat();
        assert_eq!(
            decode(0x373, &title).unwrap(),
            Packet::TitleEarned {
                bit_index: 7,
                earned: true
            }
        );
        let lost = [7u32.to_le_bytes(), 0u32.to_le_bytes()].concat();
        assert!(matches!(
            decode(0x373, &lost).unwrap(),
            Packet::TitleEarned { earned: false, .. }
        ));
        let odd = [7u32.to_le_bytes(), 2u32.to_le_bytes()].concat();
        assert!(decode(0x373, &odd).is_err());
        for end in 0..title.len() {
            assert!(decode(0x373, &title[..end]).is_err());
        }
        assert!(decode(0x373, &[title.clone(), vec![0]].concat()).is_err());
        assert_eq!(decode(0x427, &[]).unwrap(), Packet::EnableBarberShop);
        assert!(decode(0x427, &[0]).is_err());
        assert_eq!(
            decode(0x428, &1u32.to_le_bytes()).unwrap(),
            Packet::BarberShopResult { result: 1 }
        );
        assert!(decode(0x428, &[1, 0, 0]).is_err());
    }

    #[test]
    fn commands_encode_ac_read_order() {
        assert_eq!(
            encode(&Command::ShowingHelm { show: true }),
            (0x2b9, vec![1])
        );
        assert_eq!(
            encode(&Command::ShowingCloak { show: false }),
            (0x2ba, vec![0])
        );
        let (op, body) = encode(&Command::AlterAppearance {
            hair: 1,
            hair_color: 2,
            facial_hair: 3,
            skin_color: 0,
        });
        assert_eq!(op, 0x426);
        assert_eq!(
            body,
            [1u32, 2, 3, 0]
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect::<Vec<_>>()
        );
    }
}
