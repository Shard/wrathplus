//! Native NPC service flags. Bits stay in their wire dialect; only predicates are portable.
use super::{FieldLayout, ObjectFields};

/// Services shared by both client dialects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpcService {
    Gossip,
    QuestGiver,
    Vendor,
    FlightMaster,
    Trainer,
    SpiritHealer,
    SpiritGuide,
    Innkeeper,
    Banker,
    Petitioner,
    TabardDesigner,
    BattleMaster,
    Auctioneer,
    StableMaster,
    Repair,
}

/// A native `UNIT_NPC_FLAGS` word with its dialect, including unknown bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NpcFlags {
    layout: FieldLayout,
    bits: u32,
}

impl NpcFlags {
    pub const fn from_bits(layout: FieldLayout, bits: u32) -> Self {
        Self { layout, bits }
    }
    pub const fn bits(self) -> u32 {
        self.bits
    }
    pub const fn layout(self) -> FieldLayout {
        self.layout
    }

    /// Semantic service test; vendor/trainer specialization bits count as their service.
    /// vmangos UnitDefines.h and AzerothCore SharedDefines.h (`enum NPCFlags`).
    pub fn has(self, service: NpcService) -> bool {
        use NpcService::*;
        let (vanilla, wrath) = match service {
            Gossip => (0x1, 0x1),
            QuestGiver => (0x2, 0x2),
            Vendor => (0x4, 0xf80),
            FlightMaster => (0x8, 0x2000),
            Trainer => (0x10, 0x70),
            SpiritHealer => (0x20, 0x4000),
            SpiritGuide => (0x40, 0x8000),
            Innkeeper => (0x80, 0x10000),
            Banker => (0x100, 0x20000),
            Petitioner => (0x200, 0x40000),
            TabardDesigner => (0x400, 0x80000),
            BattleMaster => (0x800, 0x100000),
            Auctioneer => (0x1000, 0x200000),
            StableMaster => (0x2000, 0x400000),
            Repair => (0x4000, 0x1000),
        };
        self.bits
            & match self.layout {
                FieldLayout::Vanilla5875 => vanilla,
                FieldLayout::Wrath12340 => wrath,
            }
            != 0
    }
    pub fn is_vendor(self) -> bool {
        self.has(NpcService::Vendor)
    }
    pub fn is_trainer(self) -> bool {
        self.has(NpcService::Trainer)
    }
    pub fn is_quest_giver(self) -> bool {
        self.has(NpcService::QuestGiver)
    }
    pub fn can_repair(self) -> bool {
        self.has(NpcService::Repair)
    }
}

impl ObjectFields {
    /// Native flags and portable service predicates; no Vanilla bit renumbering.
    pub fn unit_npc_services(&self) -> NpcFlags {
        NpcFlags::from_bits(self.layout(), self.unit_npc_flags())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::Field;

    #[test]
    fn services_match_both_native_dialects_without_renumbering() {
        use NpcService::*;
        for (service, vanilla, wrath) in [
            (Gossip, 1, 1),
            (QuestGiver, 2, 2),
            (Vendor, 4, 0x80),
            (FlightMaster, 8, 0x2000),
            (Trainer, 0x10, 0x10),
            (SpiritHealer, 0x20, 0x4000),
            (SpiritGuide, 0x40, 0x8000),
            (Innkeeper, 0x80, 0x10000),
            (Banker, 0x100, 0x20000),
            (Petitioner, 0x200, 0x40000),
            (TabardDesigner, 0x400, 0x80000),
            (BattleMaster, 0x800, 0x100000),
            (Auctioneer, 0x1000, 0x200000),
            (StableMaster, 0x2000, 0x400000),
            (Repair, 0x4000, 0x1000),
        ] {
            for (layout, bits) in [
                (FieldLayout::Vanilla5875, vanilla),
                (FieldLayout::Wrath12340, wrath),
            ] {
                let span = layout.resolve(Field::UnitNpcFlags).unwrap();
                let fields =
                    ObjectFields::from_native_pairs(layout, &[(span.index, bits | 0x80000000)]);
                let flags = fields.unit_npc_services();
                assert_eq!(flags.bits(), bits | 0x80000000);
                assert_eq!(flags.layout(), layout);
                assert!(flags.has(service));
                assert!(!NpcFlags::from_bits(layout, 0).has(service));
            }
        }
        for bit in [0x80, 0x100, 0x200, 0x400, 0x800] {
            assert!(NpcFlags::from_bits(FieldLayout::Wrath12340, bit).is_vendor());
        }
        for bit in [0x10, 0x20, 0x40] {
            assert!(NpcFlags::from_bits(FieldLayout::Wrath12340, bit).is_trainer());
        }
        let wrath = NpcFlags::from_bits(FieldLayout::Wrath12340, 0x1000);
        assert!(wrath.can_repair());
        assert!(!wrath.has(Auctioneer));
        let vanilla = NpcFlags::from_bits(FieldLayout::Vanilla5875, 0x1000);
        assert!(!vanilla.can_repair());
        assert!(vanilla.has(Auctioneer));
    }
}
