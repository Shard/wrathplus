//! Build 12340 talent, loot-owner and achievement progress notices. Layouts from AzerothCore
//! Player::BuildPlayerTalentsInfoData/BuildPetTalentsInfoData, Group::SendLooter and
//! AchievementMgr::SendCriteriaUpdate.

use std::io::{self, Cursor};

use crate::events::{SessionEvent, TalentGroup};
use crate::wire::read_packed_guid;
use crate::wrath_session::{u16_le, u32_le, u64_le, u8_le};

pub const SMSG_LOOT_LIST: u16 = 0x03F9;
pub const SMSG_CRITERIA_UPDATE: u16 = 0x046A;
pub const SMSG_TALENTS_INFO: u16 = 0x04C0;

pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_LOOT_LIST | SMSG_CRITERIA_UPDATE | SMSG_TALENTS_INFO
    )
}

fn talents(r: &mut Cursor<&[u8]>) -> io::Result<Vec<(u32, u8)>> {
    let count = u8_le(r)?;
    let mut entries = Vec::new();
    for _ in 0..count {
        entries.push((u32_le(r)?, u8_le(r)?));
    }
    Ok(entries)
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mut r = Cursor::new(body);
    let event = match opcode {
        SMSG_LOOT_LIST => SessionEvent::LootOwners {
            guid: u64_le(&mut r)?,
            master: read_packed_guid(&mut r)?,
            looter: read_packed_guid(&mut r)?,
        },
        SMSG_CRITERIA_UPDATE => SessionEvent::CriteriaUpdate {
            criteria: u32_le(&mut r)?,
            counter: read_packed_guid(&mut r)?,
            player: read_packed_guid(&mut r)?,
            flags: u32_le(&mut r)?,
            packed_time: u32_le(&mut r)?,
            elapsed: u32_le(&mut r)?,
            creation_elapsed: u32_le(&mut r)?,
        },
        SMSG_TALENTS_INFO => {
            let pet = match u8_le(&mut r)? {
                0 => false,
                1 => true,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid talent owner",
                    ))
                }
            };
            let unspent = u32_le(&mut r)?;
            let mut groups = Vec::new();
            let active;
            if pet {
                active = 0;
                groups.push(TalentGroup {
                    talents: talents(&mut r)?,
                    glyphs: Vec::new(),
                });
            } else {
                let count = u8_le(&mut r)?;
                active = u8_le(&mut r)?;
                if count > 2 || (count > 0 && active >= count) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid talent group",
                    ));
                }
                for _ in 0..count {
                    let talents = talents(&mut r)?;
                    let glyph_count = u8_le(&mut r)?;
                    let mut glyphs = Vec::new();
                    for _ in 0..glyph_count {
                        glyphs.push(u16_le(&mut r)?);
                    }
                    groups.push(TalentGroup { talents, glyphs });
                }
            }
            SessionEvent::TalentsInfo {
                pet,
                unspent,
                active,
                groups,
            }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a Wrath progress opcode",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Wrath progress trailing bytes",
        ));
    }
    Ok(vec![event])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::write_packed_guid;

    fn check(opcode: u16, b: &[u8]) {
        for end in 0..b.len() {
            assert!(decode(opcode, &b[..end]).is_err(), "prefix {end}");
        }
        let mut extra = b.to_vec();
        extra.push(0);
        assert!(decode(opcode, &extra).is_err());
    }

    #[test]
    fn player_talents_preserve_both_specs_and_glyphs() {
        let mut b = vec![0];
        b.extend(3u32.to_le_bytes());
        b.extend([2, 1]);
        for rank in [0, 4] {
            b.push(1);
            b.extend(123u32.to_le_bytes());
            b.push(rank);
            b.push(2);
            b.extend(42u16.to_le_bytes());
            b.extend(0u16.to_le_bytes());
        }
        let events = decode(SMSG_TALENTS_INFO, &b).unwrap();
        let [SessionEvent::TalentsInfo {
            pet,
            unspent,
            active,
            groups,
        }] = &events[..]
        else {
            panic!()
        };
        assert!(!pet);
        assert_eq!((*unspent, *active, groups.len()), (3, 1, 2));
        assert_eq!(groups[1].talents, [(123, 4)]);
        assert_eq!(groups[0].glyphs, [42, 0]);
        check(SMSG_TALENTS_INFO, &b);
    }

    #[test]
    fn pet_talents_have_no_spec_or_glyph_header() {
        let mut b = vec![1];
        b.extend(2u32.to_le_bytes());
        b.push(1);
        b.extend(456u32.to_le_bytes());
        b.push(2);
        let events = decode(SMSG_TALENTS_INFO, &b).unwrap();
        let [SessionEvent::TalentsInfo { pet, groups, .. }] = &events[..] else {
            panic!()
        };
        assert!(*pet);
        assert_eq!(groups[0].talents, [(456, 2)]);
        assert!(groups[0].glyphs.is_empty());
        check(SMSG_TALENTS_INFO, &b);
        assert!(decode(SMSG_TALENTS_INFO, &[0, 0, 0, 0, 0, 3, 0]).is_err());
    }

    #[test]
    fn loot_owners_use_full_creature_and_packed_player_guids() {
        let mut b = 0xF130000000001234u64.to_le_bytes().to_vec();
        write_packed_guid(0x123, &mut b).unwrap();
        write_packed_guid(0, &mut b).unwrap();
        assert!(matches!(
            decode(SMSG_LOOT_LIST, &b).unwrap()[0],
            SessionEvent::LootOwners {
                guid: 0xF130000000001234,
                master: 0x123,
                looter: 0
            }
        ));
        check(SMSG_LOOT_LIST, &b);
    }

    #[test]
    fn criteria_counter_is_a_packed_u64_not_a_guid_identity() {
        let mut b = 123u32.to_le_bytes().to_vec();
        write_packed_guid(0x100000001, &mut b).unwrap();
        write_packed_guid(0x42, &mut b).unwrap();
        for v in [1u32, 0xABCDEF01, 30, 60] {
            b.extend(v.to_le_bytes());
        }
        assert!(matches!(
            decode(SMSG_CRITERIA_UPDATE, &b).unwrap()[0],
            SessionEvent::CriteriaUpdate {
                criteria: 123,
                counter: 0x100000001,
                player: 0x42,
                flags: 1,
                packed_time: 0xABCDEF01,
                elapsed: 30,
                creation_elapsed: 60
            }
        ));
        check(SMSG_CRITERIA_UPDATE, &b);
    }
}
