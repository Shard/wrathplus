//! Lossless build-12340 guild-bank replies, separate from Vanilla guild state.
//! Reference: AzerothCore 7f12e89 GuildPackets.cpp permissions/remaining writers;
//! native 0x005cb9f0 reads six permission rows regardless of purchased count.
//! Native getter 0x005a44a3 uses signed FILD for the remaining-money word.

use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_GUILD_BANK_LIST: u16 = 0x03e8;
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

/// Raw contents reply. Full-update is retained, not applied as reducer policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contents {
    pub money: u64,
    pub tab: u8,
    pub remaining_withdrawals: i32,
    pub full_update: u8,
    /// Present only when full_update != 0 and tab == 0, including empty lists.
    pub tab_info: Option<Vec<TabInfo>>,
    pub slots: Vec<Slot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabInfo {
    /// NUL-terminated wire strings without the terminator; no lossy UTF-8 conversion.
    pub name: Vec<u8>,
    pub icon: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub slot: u8,
    pub item_id: u32,
    /// Absent exactly when item_id == 0 (explicit empty slot).
    pub item: Option<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub flags: u32,
    pub random_property: u32,
    pub random_property_seed: Option<u32>,
    pub count: u32,
    pub enchantment: u32,
    pub charges: u8,
    /// Ordered raw index/enchantment pairs, including unknown indices and duplicates.
    pub sockets: Vec<(u8, u32)>,
}

// Native 0x005a7250 and AC GuildBankQueryResults::Write agree on framing.
// Six tabs/98 slots/three sockets are retained-client capacities, not wire bounds.
fn contents(r: &mut Cursor<&[u8]>) -> io::Result<Contents> {
    let money = u64_le(r)?;
    let tab = u8_le(r)?;
    let remaining_withdrawals = u32_le(r)? as i32;
    let full_update = u8_le(r)?;
    let tab_info = if full_update != 0 && tab == 0 {
        let count = u8_le(r)?;
        let mut tabs = Vec::new();
        for _ in 0..count {
            tabs.push(TabInfo {
                name: wire_string(r)?,
                icon: wire_string(r)?,
            });
        }
        Some(tabs)
    } else {
        None
    };
    let count = u8_le(r)?;
    let mut slots = Vec::new();
    for _ in 0..count {
        let slot = u8_le(r)?;
        let item_id = u32_le(r)?;
        let item = if item_id != 0 {
            let flags = u32_le(r)?;
            let random_property = u32_le(r)?;
            let random_property_seed = if random_property != 0 {
                Some(u32_le(r)?)
            } else {
                None
            };
            let count = u32_le(r)?;
            let enchantment = u32_le(r)?;
            let charges = u8_le(r)?;
            let socket_count = u8_le(r)?;
            let mut sockets = Vec::new();
            for _ in 0..socket_count {
                sockets.push((u8_le(r)?, u32_le(r)?));
            }
            Some(Item {
                flags,
                random_property,
                random_property_seed,
                count,
                enchantment,
                charges,
                sockets,
            })
        } else {
            None
        };
        slots.push(Slot {
            slot,
            item_id,
            item,
        });
    }
    Ok(Contents {
        money,
        tab,
        remaining_withdrawals,
        full_update,
        tab_info,
        slots,
    })
}

fn wire_string(r: &mut Cursor<&[u8]>) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let byte = u8_le(r)?;
        if byte == 0 {
            return Ok(bytes);
        }
        bytes.push(byte);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Contents(Contents),
    Log(crate::wrath_guild_bank_log::Log),
    Permissions(Permissions),
    RemainingMoney { remaining: i32 },
}

/// Decode a complete reply. No event or state mutation occurs here.
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    if opcode == crate::wrath_guild_bank_log::MSG_GUILD_BANK_LOG_QUERY {
        return crate::wrath_guild_bank_log::decode(body).map(Packet::Log);
    }
    let mut r = Cursor::new(body);
    let packet = match opcode {
        SMSG_GUILD_BANK_LIST => Packet::Contents(contents(&mut r)?),
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

    // Full tab zero: two metadata rows (including invalid UTF-8), an empty slot,
    // and one item with a negative property, seed and duplicate socket indices.
    const CONTENTS: &[u8] = &[
        8, 7, 6, 5, 4, 3, 2, 1, 0, 255, 255, 255, 255, 2, 2, b'A', 0, b'I', 0, 255, 0, 0, 2, 250,
        0, 0, 0, 0, 250, 4, 3, 2, 1, 0, 0, 0, 128, 255, 255, 255, 255, 9, 8, 7, 6, 5, 0, 0, 0, 6,
        0, 0, 0, 255, 2, 7, 8, 0, 0, 0, 7, 9, 0, 0, 0,
    ];

    fn read_contents(body: &[u8]) -> Contents {
        let Packet::Contents(c) = decode(SMSG_GUILD_BANK_LIST, body).unwrap() else {
            panic!()
        };
        c
    }

    #[test]
    fn contents_preserve_conditional_metadata_item_seed_and_sockets() {
        let c = read_contents(CONTENTS);
        assert_eq!(c.money, 0x0102030405060708);
        assert_eq!((c.tab, c.remaining_withdrawals, c.full_update), (0, -1, 2));
        assert_eq!(
            c.tab_info,
            Some(vec![
                TabInfo {
                    name: b"A".to_vec(),
                    icon: b"I".to_vec()
                },
                TabInfo {
                    name: vec![255],
                    icon: vec![]
                },
            ])
        );
        assert_eq!(
            c.slots[0],
            Slot {
                slot: 250,
                item_id: 0,
                item: None
            }
        );
        assert_eq!(
            c.slots[1],
            Slot {
                slot: 250,
                item_id: 0x01020304,
                item: Some(Item {
                    flags: 0x80000000,
                    random_property: u32::MAX,
                    random_property_seed: Some(0x06070809),
                    count: 5,
                    enchantment: 6,
                    charges: 255,
                    sockets: vec![(7, 8), (7, 9)],
                })
            }
        );
    }

    #[test]
    fn contents_full_nonzero_tab_and_delta_omit_metadata() {
        // Item with property zero omits seed, then a second explicit empty row.
        let body = [
            0, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 128, 1, 2, 98, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0,
            0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 99, 0, 0, 0, 0,
        ];
        for (tab, full) in [(255, 1), (0, 0), (1, 0)] {
            let mut body = body;
            body[8] = tab;
            body[13] = full;
            let c = read_contents(&body);
            assert_eq!(c.tab_info, None);
            assert_eq!(c.remaining_withdrawals, i32::MIN);
            let item = c.slots[0].item.as_ref().unwrap();
            assert_eq!(item.random_property_seed, None);
            assert_eq!((item.count, item.enchantment, item.charges), (3, 4, 5));
            assert!(item.sockets.is_empty());
            assert!(c.slots[1].item.is_none());
        }
        let empty_full = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0];
        assert_eq!(read_contents(&empty_full).tab_info, Some(vec![]));
    }

    #[test]
    fn contents_counted_rows_are_not_native_capacity_limits() {
        let mut body = vec![0; 14];
        body[13] = 1;
        body.push(7);
        body.extend([0; 14]); // seven empty name/icon pairs
        body.push(255);
        for slot in 0..=254 {
            body.extend([slot, 0, 0, 0, 0]);
        }
        let c = read_contents(&body);
        assert_eq!(c.tab_info.unwrap().len(), 7);
        assert_eq!(c.slots.len(), 255);
        // Socket rows likewise remain ordered, not indexed into a 3-element array.
        let mut body = vec![0; 14];
        body.extend([1, 0, 1, 0, 0, 0]);
        body.extend([0; 17]); // flags/property/count/enchantment/charges
        body.push(255);
        for index in 0..=254 {
            body.extend([index, 1, 0, 0, 0]);
        }
        assert_eq!(
            read_contents(&body).slots[0]
                .item
                .as_ref()
                .unwrap()
                .sockets
                .len(),
            255
        );
    }

    #[test]
    fn contents_reject_all_truncations_trailing_and_bad_counts() {
        for end in 0..CONTENTS.len() {
            assert!(
                decode(SMSG_GUILD_BANK_LIST, &CONTENTS[..end]).is_err(),
                "{end}"
            );
        }
        let mut body = CONTENTS.to_vec();
        body.push(0);
        assert!(decode(SMSG_GUILD_BANK_LIST, &body).is_err());
        // Unterminated name, missing icon, missing item row, missing socket row.
        for suffix in [&[1, b'A'][..], &[1, b'A', 0][..], &[0, 1][..]] {
            let mut body = CONTENTS[..14].to_vec();
            body.extend(suffix);
            assert!(decode(SMSG_GUILD_BANK_LIST, &body).is_err());
        }
        let mut body = CONTENTS.to_vec();
        body[14] = 255;
        assert!(decode(SMSG_GUILD_BANK_LIST, &body).is_err());
    }

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
