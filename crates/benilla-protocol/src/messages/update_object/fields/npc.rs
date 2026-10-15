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
    /// Exact base vendor flag, excluding vendor-specialization flags. Use for
    /// client branches that test the base bit, not the broader service union.
    pub fn has_vendor_base(self) -> bool {
        self.bits
            & match self.layout {
                FieldLayout::Vanilla5875 => 0x4,
                FieldLayout::Wrath12340 => 0x80,
            }
            != 0
    }
    /// Exact base trainer flag, excluding class/profession trainer flags.
    pub fn has_trainer_base(self) -> bool {
        self.bits & 0x10 != 0
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
    fn exact_base_predicates_do_not_promote_wrath_specializations() {
        for bit in [0x100, 0x200, 0x400, 0x800] {
            let flags = NpcFlags::from_bits(FieldLayout::Wrath12340, bit | 0x80000000);
            assert!(flags.is_vendor());
            assert!(!flags.has_vendor_base());
            assert!(NpcFlags::from_bits(FieldLayout::Wrath12340, bit | 0x80).has_vendor_base());
        }
        for bit in [0x20, 0x40] {
            let flags = NpcFlags::from_bits(FieldLayout::Wrath12340, bit | 0x80000000);
            assert!(flags.is_trainer());
            assert!(!flags.has_trainer_base());
            assert!(NpcFlags::from_bits(FieldLayout::Wrath12340, bit | 0x10).has_trainer_base());
        }
    }

    #[test]
    fn exact_base_predicates_respect_native_dialect_and_preserve_all_bits() {
        for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
            let vendor_bit = if layout == FieldLayout::Vanilla5875 {
                0x4
            } else {
                0x80
            };
            for bit in 0..32 {
                let bits = 1u32 << bit;
                let flags = NpcFlags::from_bits(layout, bits);
                assert_eq!(flags.has_vendor_base(), bits == vendor_bit);
                assert_eq!(flags.has_trainer_base(), bits == 0x10);
                assert_eq!(flags.bits(), bits);
                assert_eq!(flags.layout(), layout);
            }
            let empty = NpcFlags::from_bits(layout, 0);
            assert!(!empty.has_vendor_base());
            assert!(!empty.has_trainer_base());
            let all = NpcFlags::from_bits(layout, u32::MAX);
            assert!(all.has_vendor_base());
            assert!(all.has_trainer_base());
        }
    }

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
