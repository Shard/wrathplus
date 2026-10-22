//! Build-12340 guild-bank interaction command bodies, without eligibility policy.
//! Layouts: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03,
//! GuildPackets.cpp / GuildPackets.h. Native buy/deposit/withdraw senders:
//! 0x005a5010, 0x005a5b00, 0x005a5ba0 respectively.
//! Close is local-only and deliberately has no packet here. Queries are separate.

pub const CMSG_GUILD_BANKER_ACTIVATE: u32 = 0x03e6;
pub const CMSG_GUILD_BANK_BUY_TAB: u32 = 0x03ea;
pub const CMSG_GUILD_BANK_DEPOSIT_MONEY: u32 = 0x03ec;
pub const CMSG_GUILD_BANK_WITHDRAW_MONEY: u32 = 0x03ed;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Raw full-update byte; interaction selection belongs to the caller.
    Activate {
        banker: u64,
        full_update: u8,
    },
    /// Native sender uses the purchased-tab count, not a Lua one-based index.
    /// This codec preserves the caller's byte without imposing cache capacity.
    BuyTab {
        banker: u64,
        tab: u8,
    },
    /// Amounts are copper u32 on wire, despite the u64 replicated balance.
    /// Conversion and balance/allowance checks belong to App/UI.
    DepositMoney {
        banker: u64,
        amount: u32,
    },
    WithdrawMoney {
        banker: u64,
        amount: u32,
    },
}

impl Command {
    pub const fn opcode(&self) -> u32 {
        match self {
            Self::Activate { .. } => CMSG_GUILD_BANKER_ACTIVATE,
            Self::BuyTab { .. } => CMSG_GUILD_BANK_BUY_TAB,
            Self::DepositMoney { .. } => CMSG_GUILD_BANK_DEPOSIT_MONEY,
            Self::WithdrawMoney { .. } => CMSG_GUILD_BANK_WITHDRAW_MONEY,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        match *self {
            Self::Activate {
                banker,
                full_update,
            } => {
                let mut body = banker.to_le_bytes().to_vec();
                body.push(full_update);
                body
            }
            Self::BuyTab { banker, tab } => {
                let mut body = banker.to_le_bytes().to_vec();
                body.push(tab);
                body
            }
            Self::DepositMoney { banker, amount } | Self::WithdrawMoney { banker, amount } => {
                let mut body = banker.to_le_bytes().to_vec();
                body.extend(amount.to_le_bytes());
                body
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activate_and_buy_are_guid_then_raw_byte() {
        let activate = Command::Activate {
            banker: 0x0102030405060708,
            full_update: 0xa5,
        };
        assert_eq!(activate.opcode(), 0x03e6);
        assert_eq!(activate.encode(), [8, 7, 6, 5, 4, 3, 2, 1, 0xa5]);
        let buy = Command::BuyTab {
            banker: 0x0102030405060708,
            tab: 255,
        };
        assert_eq!(buy.opcode(), 0x03ea);
        assert_eq!(buy.encode(), [8, 7, 6, 5, 4, 3, 2, 1, 255]);
        assert_eq!(
            Command::Activate {
                banker: 0,
                full_update: 0
            }
            .encode(),
            [0; 9]
        );
        assert_eq!(
            Command::BuyTab {
                banker: u64::MAX,
                tab: 0
            }
            .encode(),
            [255, 255, 255, 255, 255, 255, 255, 255, 0]
        );
    }

    #[test]
    fn money_is_exactly_guid_and_u32_not_balance_width() {
        let deposit = Command::DepositMoney {
            banker: 0x0102030405060708,
            amount: 0x89abcdef,
        };
        assert_eq!(deposit.opcode(), 0x03ec);
        assert_eq!(
            deposit.encode(),
            [8, 7, 6, 5, 4, 3, 2, 1, 0xef, 0xcd, 0xab, 0x89]
        );
        let withdraw = Command::WithdrawMoney {
            banker: 0x0102030405060708,
            amount: 0x89abcdef,
        };
        assert_eq!(withdraw.opcode(), 0x03ed);
        assert_eq!(withdraw.encode(), deposit.encode());
        for amount in [0, 1, u32::MAX] {
            let deposit = Command::DepositMoney {
                banker: u64::MAX,
                amount,
            }
            .encode();
            let withdraw = Command::WithdrawMoney {
                banker: u64::MAX,
                amount,
            }
            .encode();
            assert_eq!(deposit.len(), 12);
            assert_eq!(withdraw, deposit);
            assert_eq!(&deposit[..8], &[255; 8]);
            assert_eq!(u32::from_le_bytes(deposit[8..].try_into().unwrap()), amount);
        }
    }
}
