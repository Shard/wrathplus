//! Completion reward, AzerothCore 7f12e89e LFGHandler.cpp::SendLfgPlayerReward.
//! Native handler 0x0055bdc0 confirms unconditional two words, raw done byte,
//! five words, u8 item count and 12-byte rows. There is no five-item cap.
use super::{check_rows, invalid, RewardItem};
use crate::wrath_session::{u32_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_LFG_PLAYER_REWARD: u16 = 0x01ff;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reward {
    pub random_entry: u32,
    pub actual_entry: u32,
    pub done: u8,
    /// Native label "Strangers"; preserve without inferring reward policy.
    pub extra: u32,
    pub money: u32,
    pub xp: u32,
    /// Native labels "VarMoney" and "VarXP"; not assumed zero.
    pub variable_money: u32,
    pub variable_xp: u32,
    pub items: Vec<RewardItem>,
}
pub fn decode(body: &[u8]) -> io::Result<Reward> {
    let mut r = Cursor::new(body);
    let random_entry = u32_le(&mut r)?;
    let actual_entry = u32_le(&mut r)?;
    let done = u8_le(&mut r)?;
    let extra = u32_le(&mut r)?;
    let money = u32_le(&mut r)?;
    let xp = u32_le(&mut r)?;
    let variable_money = u32_le(&mut r)?;
    let variable_xp = u32_le(&mut r)?;
    let count = u8_le(&mut r)? as usize;
    check_rows(&r, count, 12)?;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(RewardItem {
            item: u32_le(&mut r)?,
            display: u32_le(&mut r)?,
            count: u32_le(&mut r)?,
        });
    }
    if r.position() as usize != body.len() {
        return Err(invalid("trailing LFG completion bytes"));
    }
    Ok(Reward {
        random_entry,
        actual_entry,
        done,
        extra,
        money,
        xp,
        variable_money,
        variable_xp,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        vec![
            0x56, 0x34, 0x12, 0xab, 4, 3, 2, 0x81, 0xfe, 0xff, 0xff, 0xff, 0xff, 8, 7, 6, 5, 0, 0,
            0, 0x80, 4, 3, 2, 1, 0x44, 0x33, 0x22, 0x11, 2, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0,
            0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0x80,
        ]
    }
    #[test]
    fn independent_reward_fixture_preserves_all_raw_words_and_items() {
        let expected = Reward {
            random_entry: 0xab123456,
            actual_entry: 0x81020304,
            done: 0xfe,
            extra: u32::MAX,
            money: 0x05060708,
            xp: 0x80000000,
            variable_money: 0x01020304,
            variable_xp: 0x11223344,
            items: vec![
                RewardItem {
                    item: 1,
                    display: 2,
                    count: 3,
                },
                RewardItem {
                    item: u32::MAX,
                    display: 0,
                    count: 0x80000000,
                },
            ],
        };
        assert_eq!(decode(&fixture()).unwrap(), expected);
        assert!(super::super::handles(0x01ff));
        assert_eq!(
            super::super::decode(0x01ff, &fixture()).unwrap(),
            super::super::Packet::Completion(expected)
        );
    }
    #[test]
    fn completion_count_boundaries_preserve_all_rows_and_explicit_zero() {
        for count in [0u8, 6, 255] {
            let mut body = vec![0; 30];
            body[29] = count;
            for row in 0..count {
                body.extend([row, 0, 0, 0x80, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff]);
            }
            let reward = decode(&body).unwrap();
            assert_eq!(reward.items.len(), count as usize);
            assert_eq!(reward.done, 0);
            for (index, item) in reward.items.iter().enumerate() {
                assert_eq!(
                    item,
                    &RewardItem {
                        item: 0x80000000 | index as u32,
                        display: 0,
                        count: u32::MAX
                    }
                );
            }
        }
    }
    #[test]
    fn completion_rejects_every_truncation_trailing_and_forged_counts() {
        let body = fixture();
        for cut in 0..body.len() {
            assert!(
                super::super::decode(0x01ff, &body[..cut]).is_err(),
                "cut {cut}"
            );
        }
        let mut trailing = body.clone();
        trailing.push(0);
        assert!(decode(&trailing).is_err());
        for count in [0, 1, 3, 255] {
            let mut bad = body.clone();
            bad[29] = count;
            assert!(decode(&bad).is_err());
        }
        // done=false never removes the remaining unconditional body.
        let mut inactive = body;
        inactive[8] = 0;
        assert!(decode(&inactive).is_ok());
        assert!(decode(&inactive[..9]).is_err());
    }
}
