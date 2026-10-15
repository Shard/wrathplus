//! Build-12340 guild-bank read requests. These encode transport only: the App owner
//! supplies interaction authority and decides when queries are appropriate.
//! Native 0x005a5800 and AzerothCore GuildPackets.cpp/GuildPackets.h.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    Permissions,
    RemainingMoney,
    Tab {
        banker: u64,
        tab: u8,
        /// Preserve the caller's wire byte; no cache policy is inferred here.
        full_update: u8,
    },
}

impl Query {
    pub fn opcode(&self) -> u32 {
        match self {
            Self::Permissions => 0x03fd,
            Self::RemainingMoney => 0x03fe,
            Self::Tab { .. } => 0x03e7,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Permissions | Self::RemainingMoney => Vec::new(),
            Self::Tab {
                banker,
                tab,
                full_update,
            } => {
                let mut body = banker.to_le_bytes().to_vec();
                body.extend([*tab, *full_update]);
                body
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_use_native_opcodes_and_exact_bodies() {
        for (query, opcode, body) in [
            (Query::Permissions, 0x03fd, vec![]),
            (Query::RemainingMoney, 0x03fe, vec![]),
            (
                Query::Tab {
                    banker: 0x0102030405060708,
                    tab: 5,
                    full_update: 0xff,
                },
                0x03e7,
                vec![8, 7, 6, 5, 4, 3, 2, 1, 5, 255],
            ),
            (
                Query::Tab {
                    banker: 0,
                    tab: 0,
                    full_update: 0,
                },
                0x03e7,
                vec![0; 10],
            ),
        ] {
            assert_eq!(query.opcode(), opcode);
            assert_eq!(query.encode(), body);
        }
    }
}
