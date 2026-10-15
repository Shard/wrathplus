//! Build 12340 group loot rolls. AzerothCore Group.cpp::SendLootStartRoll/SendLootRoll/
//! SendLootRollWon/SendLootAllPassed. Native fields must not be narrowed into the 1.12 roll model.
use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_LOOT_ALL_PASSED: u16 = 0x29e;
pub const SMSG_LOOT_ROLL_WON: u16 = 0x29f;
pub const CMSG_LOOT_ROLL: u32 = 0x2a0;
pub const SMSG_LOOT_START_ROLL: u16 = 0x2a1;
pub const SMSG_LOOT_ROLL: u16 = 0x2a2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub guid: u64,
    pub slot: u32,
    pub entry: u32,
    pub suffix_factor: u32,
    pub random_property: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Roll {
    Start {
        item: Item,
        map: u32,
        count: u32,
        countdown_ms: u32,
        vote_mask: u8,
    },
    Vote {
        item: Item,
        player: u64,
        number: u8,
        kind: u8,
        auto_pass: bool,
    },
    Won {
        item: Item,
        player: u64,
        number: u8,
        kind: u8,
    },
    AllPassed {
        item: Item,
    },
}

pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_LOOT_START_ROLL | SMSG_LOOT_ROLL | SMSG_LOOT_ROLL_WON | SMSG_LOOT_ALL_PASSED
    )
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Wrath roll layout")
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Roll> {
    if !handles(opcode) {
        return Err(invalid());
    }
    let mut r = Cursor::new(body);
    let guid = u64_le(&mut r)?;
    let map = if opcode == SMSG_LOOT_START_ROLL {
        u32_le(&mut r)?
    } else {
        0
    };
    let slot = u32_le(&mut r)?;
    let player = if opcode == SMSG_LOOT_ROLL {
        u64_le(&mut r)?
    } else {
        0
    };
    let entry = u32_le(&mut r)?;
    let first = u32_le(&mut r)?;
    let second = u32_le(&mut r)?;
    // ALL_PASSED uniquely puts the property before the suffix.
    let (suffix_factor, random_property) = if opcode == SMSG_LOOT_ALL_PASSED {
        (second, first as i32)
    } else {
        (first, second as i32)
    };
    let item = Item {
        guid,
        slot,
        entry,
        suffix_factor,
        random_property,
    };
    let roll = match opcode {
        SMSG_LOOT_START_ROLL => Roll::Start {
            item,
            map,
            count: u32_le(&mut r)?,
            countdown_ms: u32_le(&mut r)?,
            vote_mask: u8_le(&mut r)?,
        },
        SMSG_LOOT_ROLL => {
            let number = u8_le(&mut r)?;
            let kind = u8_le(&mut r)?;
            let auto_pass = match u8_le(&mut r)? {
                0 => false,
                1 => true,
                _ => return Err(invalid()),
            };
            Roll::Vote {
                item,
                player,
                number,
                kind,
                auto_pass,
            }
        }
        SMSG_LOOT_ROLL_WON => Roll::Won {
            item,
            player: u64_le(&mut r)?,
            number: u8_le(&mut r)?,
            kind: u8_le(&mut r)?,
        },
        SMSG_LOOT_ALL_PASSED => Roll::AllPassed { item },
        _ => unreachable!(),
    };
    if r.position() as usize != body.len() {
        return Err(invalid());
    }
    Ok(roll)
}

/// LootHandler.cpp::HandleLootRoll reads guid, u32 slot, u8 vote (pass/need/greed/disenchant).
pub fn encode_vote(guid: u64, slot: u32, vote: u8) -> io::Result<Vec<u8>> {
    if vote > 3 {
        return Err(invalid());
    }
    let mut body = guid.to_le_bytes().to_vec();
    body.extend_from_slice(&slot.to_le_bytes());
    body.push(vote);
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_retains_native_map_count_mask_signed_property_and_suffix() {
        let mut b = 0x1234u64.to_le_bytes().to_vec();
        for v in [571u32, 8, 2589, 12345, (-17i32) as u32, 1000, 60000] {
            b.extend(v.to_le_bytes());
        }
        b.push(15);
        assert_eq!(
            decode(SMSG_LOOT_START_ROLL, &b).unwrap(),
            Roll::Start {
                item: Item {
                    guid: 0x1234,
                    slot: 8,
                    entry: 2589,
                    suffix_factor: 12345,
                    random_property: -17
                },
                map: 571,
                count: 1000,
                countdown_ms: 60000,
                vote_mask: 15,
            }
        );
        for n in 0..b.len() {
            assert!(decode(SMSG_LOOT_START_ROLL, &b[..n]).is_err());
        }
        b.push(0);
        assert!(decode(SMSG_LOOT_START_ROLL, &b).is_err());
    }
    #[test]
    fn vote_and_winner_preserve_native_order_and_autopass() {
        let mut prefix = 42u64.to_le_bytes().to_vec();
        prefix.extend(7u32.to_le_bytes());
        let mut fields = Vec::new();
        for v in [2589u32, 876, (-17i32) as u32] {
            fields.extend(v.to_le_bytes());
        }
        let item = Item {
            guid: 42,
            slot: 7,
            entry: 2589,
            suffix_factor: 876,
            random_property: -17,
        };
        let mut vote = prefix.clone();
        vote.extend(88u64.to_le_bytes());
        vote.extend(&fields);
        vote.extend([128, 3, 1]);
        assert_eq!(
            decode(SMSG_LOOT_ROLL, &vote).unwrap(),
            Roll::Vote {
                item: item.clone(),
                player: 88,
                number: 128,
                kind: 3,
                auto_pass: true
            }
        );
        *vote.last_mut().unwrap() = 2;
        assert!(decode(SMSG_LOOT_ROLL, &vote).is_err());
        let mut won = prefix;
        won.extend(fields);
        won.extend(88u64.to_le_bytes());
        won.extend([100, 3]);
        assert_eq!(
            decode(SMSG_LOOT_ROLL_WON, &won).unwrap(),
            Roll::Won {
                item,
                player: 88,
                number: 100,
                kind: 3
            }
        );
        for n in 0..won.len() {
            assert!(decode(SMSG_LOOT_ROLL_WON, &won[..n]).is_err());
        }
    }
    #[test]
    fn all_passed_reverses_property_and_suffix_and_vote_is_strict() {
        let mut b = 22u64.to_le_bytes().to_vec();
        for v in [3u32, 2589, (-99i32) as u32, 765] {
            b.extend(v.to_le_bytes());
        }
        assert_eq!(
            decode(SMSG_LOOT_ALL_PASSED, &b).unwrap(),
            Roll::AllPassed {
                item: Item {
                    guid: 22,
                    slot: 3,
                    entry: 2589,
                    suffix_factor: 765,
                    random_property: -99
                }
            }
        );
        let mut expected = 22u64.to_le_bytes().to_vec();
        expected.extend(3u32.to_le_bytes());
        expected.push(3);
        assert_eq!(encode_vote(22, 3, 3).unwrap(), expected);
        assert!(encode_vote(22, 3, 4).is_err());
        assert!(decode(0xffff, &b).is_err());
    }
}
