//! Shared Wrath guild-control rank save, including all six bank permission rows.
//! Native 0x005cb170 and AzerothCore 7f12e89 GuildSetRankPermissions::Read agree
//! on the complete 0x0231 body. There are no individual bank-setter packets.
use std::io;

pub const CMSG_GUILD_RANK: u32 = 0x0231;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabPermission {
    pub rights: u32,
    /// Raw rank limit, not current-member withdrawals remaining.
    pub item_limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRank {
    pub rank: u32,
    /// Entire composed word; preserve bits not changed by rank checkboxes.
    pub flags: u32,
    /// Wire bytes excluding terminator. Name validation/normalization is upstream.
    pub name: Vec<u8>,
    /// Already-converted wire word. Native multiplies retained money by 10000
    /// before writing; this codec does not repeat conversion or normalize sentinels.
    pub money_limit: u32,
    pub tabs: [TabPermission; 6],
}

impl SaveRank {
    pub const fn opcode(&self) -> u32 {
        CMSG_GUILD_RANK
    }

    pub fn encode(&self) -> io::Result<Vec<u8>> {
        if self.name.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "embedded rank name NUL",
            ));
        }
        let mut body = self.rank.to_le_bytes().to_vec();
        body.extend(self.flags.to_le_bytes());
        body.extend(&self.name);
        body.push(0);
        body.extend(self.money_limit.to_le_bytes());
        for tab in &self.tabs {
            body.extend(tab.rights.to_le_bytes());
            body.extend(tab.item_limit.to_le_bytes());
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> SaveRank {
        SaveRank {
            rank: 0x01020304,
            flags: 0x80000041,
            name: vec![b'R', 255],
            money_limit: u32::MAX,
            tabs: [
                TabPermission {
                    rights: 1,
                    item_limit: 2,
                },
                TabPermission {
                    rights: 3,
                    item_limit: 4,
                },
                TabPermission {
                    rights: 5,
                    item_limit: 6,
                },
                TabPermission {
                    rights: 7,
                    item_limit: 8,
                },
                TabPermission {
                    rights: 9,
                    item_limit: 10,
                },
                TabPermission {
                    rights: 0x80000000,
                    item_limit: u32::MAX,
                },
            ],
        }
    }

    #[test]
    fn save_is_full_shared_rank_body_with_six_ordered_rows() {
        let rank = fixture();
        assert_eq!(rank.opcode(), 0x0231);
        assert_eq!(
            rank.encode().unwrap(),
            [
                4, 3, 2, 1, 0x41, 0, 0, 128, b'R', 255, 0, 255, 255, 255, 255, 1, 0, 0, 0, 2, 0, 0,
                0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0, 6, 0, 0, 0, 7, 0, 0, 0, 8, 0, 0, 0, 9, 0, 0,
                0, 10, 0, 0, 0, 0, 0, 0, 128, 255, 255, 255, 255,
            ]
        );
    }

    #[test]
    fn raw_wire_money_is_not_rescaled_and_nul_is_rejected() {
        let mut rank = fixture();
        rank.name.clear(); // codec framing, not native Lua name eligibility
        for money in [0, 1, 10000, 0x80000000, u32::MAX] {
            rank.money_limit = money;
            let body = rank.encode().unwrap();
            assert_eq!(body.len(), 61);
            assert_eq!(&body[9..13], &money.to_le_bytes());
            assert_eq!(&body[13..], &fixture().encode().unwrap()[15..]);
        }
        for name in [vec![0], vec![b'A', 0, b'B']] {
            rank.name = name;
            assert_eq!(
                rank.encode().unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }
}
