//! Lossless build-12340 guild-bank replies, separate from Vanilla guild state.
//! Reference: AzerothCore 7f12e89 GuildPackets.cpp permissions/remaining writers;
//! native 0x005cb9f0 reads six permission rows regardless of purchased count.
//! Native getter 0x005a44a3 uses signed FILD for the remaining-money word.

use crate::wrath_session::{u32_le, u8_le};
use std::io::{self, Cursor};

pub const MSG_GUILD_PERMISSIONS: u16 = 0x03fd;
pub const MSG_GUILD_BANK_MONEY_WITHDRAWN: u16 = 0x03fe;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabPermission {
    pub rights: u32,
    /// Raw word, including negative/unlimited bit patterns.
    pub remaining_items: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permissions {
    pub rank: u32,
    pub flags: u32,
    /// Raw current-member limit word, not a selected-rank edit value.
    pub money_limit: u32,
    pub purchased_tabs: u8,
    pub tabs: [TabPermission; 6],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Permissions(Permissions),
    RemainingMoney { remaining: i32 },
}

/// Decode a complete reply. No event or state mutation occurs here.
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let packet = match opcode {
        MSG_GUILD_PERMISSIONS => {
            let rank = u32_le(&mut r)?;
            let flags = u32_le(&mut r)?;
            let money_limit = u32_le(&mut r)?;
            let purchased_tabs = u8_le(&mut r)?;
            // Fixed wire rows, not purchased_tabs rows. Preserve unusual counts.
            let mut rows = Vec::with_capacity(6);
            for _ in 0..6 {
                rows.push(TabPermission {
                    rights: u32_le(&mut r)?,
                    remaining_items: u32_le(&mut r)?,
                });
            }
            Packet::Permissions(Permissions {
                rank,
                flags,
                money_limit,
                purchased_tabs,
                tabs: rows.try_into().expect("six fixed permission rows"),
            })
        }
        MSG_GUILD_BANK_MONEY_WITHDRAWN => Packet::RemainingMoney {
            remaining: u32_le(&mut r)? as i32,
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unknown guild-bank reply",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "trailing guild-bank bytes",
        ));
    }
    Ok(packet)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Independent literal wire fixture: three header words, count=1, SIX rows.
    const PERMISSIONS: &[u8] = &[
        2, 0, 0, 0, 0, 0, 4, 128, 255, 255, 255, 255, 1, 1, 0, 0, 128, 255, 255, 255, 255, 2, 0, 0,
        0, 0, 0, 0, 0, 3, 0, 0, 0, 1, 0, 0, 0, 4, 0, 0, 0, 2, 0, 0, 0, 5, 0, 0, 0, 3, 0, 0, 0, 6,
        0, 0, 0, 0, 0, 0, 128,
    ];

    #[test]
    fn permissions_preserve_six_rows_raw_words_and_order() {
        let Packet::Permissions(p) = decode(MSG_GUILD_PERMISSIONS, PERMISSIONS).unwrap() else {
            panic!()
        };
        assert_eq!(
            (p.rank, p.flags, p.money_limit, p.purchased_tabs),
            (2, 0x80040000, u32::MAX, 1)
        );
        assert_eq!(
            p.tabs[0],
            TabPermission {
                rights: 0x80000001,
                remaining_items: u32::MAX
            }
        );
        assert_eq!(p.tabs[1].remaining_items, 0);
        assert_eq!(
            p.tabs[5],
            TabPermission {
                rights: 6,
                remaining_items: 0x80000000
            }
        );
    }

    #[test]
    fn purchased_count_does_not_frame_rows() {
        for count in [0, 6, 255] {
            let mut body = PERMISSIONS.to_vec();
            body[12] = count;
            let Packet::Permissions(p) = decode(MSG_GUILD_PERMISSIONS, &body).unwrap() else {
                panic!()
            };
            assert_eq!(p.purchased_tabs, count);
            assert_eq!(p.tabs.len(), 6);
        }
    }

    #[test]
    fn signed_remaining_money_keeps_zero_and_negative_sentinels() {
        for (body, remaining) in [
            ([0, 0, 0, 0], 0),
            ([255, 255, 255, 255], -1),
            ([0, 0, 0, 128], i32::MIN),
            ([255, 255, 255, 127], i32::MAX),
        ] {
            assert_eq!(
                decode(MSG_GUILD_BANK_MONEY_WITHDRAWN, &body).unwrap(),
                Packet::RemainingMoney { remaining }
            );
        }
    }

    #[test]
    fn all_truncations_and_trailing_bytes_fail() {
        for (opcode, body) in [
            (MSG_GUILD_PERMISSIONS, PERMISSIONS),
            (MSG_GUILD_BANK_MONEY_WITHDRAWN, &[255, 255, 255, 255][..]),
        ] {
            for end in 0..body.len() {
                assert!(decode(opcode, &body[..end]).is_err(), "{opcode:x}/{end}");
            }
            let mut extra = body.to_vec();
            extra.push(0);
            assert!(decode(opcode, &extra).is_err());
        }
        assert!(decode(0x03fc, &[]).is_err());
        assert!(decode(0x03fc, PERMISSIONS).is_err());
    }
}
