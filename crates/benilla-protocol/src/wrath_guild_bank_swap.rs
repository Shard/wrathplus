//! Native 0x03e9 sender branches: 0x005a50c0 inventory-to-bank,
//! 0x005a5410 bank-to-bank, 0x005a63f0 bank auto-store.
//! Coordinates and counts are already resolved by the cursor owner. This codec
//! neither changes cursor state nor predicts permissions, items or stack sizes.
pub const CMSG_GUILD_BANK_SWAP_ITEMS: u32 = 0x03e9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BankSlot {
    pub tab: u8,
    pub slot: u8,
    pub item: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Swap {
    pub banker: u64,
    pub transfer: Transfer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transfer {
    /// Native 0x005a5410 writes destination triple first, source second.
    BankToBank {
        destination: BankSlot,
        source: BankSlot,
        count: i32,
    },
    /// Native 0x005a50c0 uses bank-only=0, auto-store=0, to-slot=0.
    InventoryToBank {
        destination: BankSlot,
        container: u8,
        container_slot: u8,
        stack_count: i32,
    },
    /// Native 0x005a63f0 uses bank-only=0, auto-store=1, to-slot=1,
    /// stack-count=0. Count is retained bank-item count, not a guessed split.
    AutoStore { source: BankSlot, count: i32 },
}

impl Swap {
    pub const fn opcode(&self) -> u32 {
        CMSG_GUILD_BANK_SWAP_ITEMS
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut body = self.banker.to_le_bytes().to_vec();
        match self.transfer {
            Transfer::BankToBank {
                destination,
                source,
                count,
            } => {
                body.push(1);
                slot(&mut body, destination);
                slot(&mut body, source);
                body.push(0);
                body.extend(count.to_le_bytes());
            }
            Transfer::InventoryToBank {
                destination,
                container,
                container_slot,
                stack_count,
            } => {
                body.push(0);
                slot(&mut body, destination);
                body.extend([0, container, container_slot, 0]);
                body.extend(stack_count.to_le_bytes());
            }
            Transfer::AutoStore { source, count } => {
                body.push(0);
                slot(&mut body, source);
                body.push(1);
                body.extend(count.to_le_bytes());
                body.push(1);
                body.extend(0i32.to_le_bytes());
            }
        }
        body
    }
}

fn slot(body: &mut Vec<u8>, slot: BankSlot) {
    body.extend([slot.tab, slot.slot]);
    body.extend(slot.item.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    const DEST: BankSlot = BankSlot {
        tab: 5,
        slot: 97,
        item: 0x01020304,
    };
    const SOURCE: BankSlot = BankSlot {
        tab: 0,
        slot: 0,
        item: 0x80000000,
    };

    #[test]
    fn bank_to_bank_is_destination_first_and_preserves_signed_count() {
        let cmd = Swap {
            banker: 0x0102030405060708,
            transfer: Transfer::BankToBank {
                destination: DEST,
                source: SOURCE,
                count: -1,
            },
        };
        assert_eq!(cmd.opcode(), 0x03e9);
        assert_eq!(
            cmd.encode(),
            [
                8, 7, 6, 5, 4, 3, 2, 1, 1, 5, 97, 4, 3, 2, 1, 0, 0, 0, 0, 0, 128, 0, 255, 255, 255,
                255
            ]
        );
    }

    #[test]
    fn inventory_sender_uses_actual_container_bytes_and_to_slot_zero() {
        let cmd = Swap {
            banker: 0,
            transfer: Transfer::InventoryToBank {
                destination: DEST,
                container: 255,
                container_slot: 254,
                stack_count: i32::MIN,
            },
        };
        assert_eq!(
            cmd.encode(),
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 97, 4, 3, 2, 1, 0, 255, 254, 0, 0, 0, 0, 128]
        );
    }

    #[test]
    fn auto_store_retains_count_and_has_native_direction_and_zero_stack() {
        let cmd = Swap {
            banker: u64::MAX,
            transfer: Transfer::AutoStore {
                source: SOURCE,
                count: i32::MAX,
            },
        };
        assert_eq!(
            cmd.encode(),
            [
                255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 128, 1, 255, 255, 255,
                127, 1, 0, 0, 0, 0
            ]
        );
        // Empty destination item, raw unusual coordinate bytes and zero split
        // remain wire values; eligibility is not the encoder's responsibility.
        let cmd = Swap {
            banker: 1,
            transfer: Transfer::BankToBank {
                destination: BankSlot {
                    tab: 255,
                    slot: 254,
                    item: 0,
                },
                source: SOURCE,
                count: 0,
            },
        };
        assert_eq!(
            cmd.encode(),
            [1, 0, 0, 0, 0, 0, 0, 0, 1, 255, 254, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 0, 0]
        );
    }
}
