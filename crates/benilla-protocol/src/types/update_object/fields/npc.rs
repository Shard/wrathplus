//! Native NPC service flags. Bits stay in their wire dialect; only predicates are portable.
use super::{FieldLayout, ObjectFields};

/// Native NPC services; dialect-specific services are absent in other layouts.
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
    GuildBanker,
    /// Flag presence only; interaction also requires FLAGS2 and GUID eligibility.
    SpellClick,
    /// Flag presence only; does not establish interaction eligibility.
    Mailbox,
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

    /// Flag-presence test, not permission to interact or send an action. Vendor/trainer
    /// specialization bits count as their service. SpellClick eligibility additionally
    /// depends on FLAGS2 and GUID checks outside this descriptor accessor.
    /// vmangos UnitDefines.h and AzerothCore SharedDefines.h (`enum NPCFlags`).
    pub fn has(self, service: NpcService) -> bool {
        use NpcService::*;
        let mask = match service {
            Gossip => 0x1,
            QuestGiver => 0x2,
            Vendor => 0xf80,
            FlightMaster => 0x2000,
            Trainer => 0x70,
            SpiritHealer => 0x4000,
            SpiritGuide => 0x8000,
            Innkeeper => 0x10000,
            Banker => 0x20000,
            Petitioner => 0x40000,
            TabardDesigner => 0x80000,
            BattleMaster => 0x100000,
            Auctioneer => 0x200000,
            StableMaster => 0x400000,
            Repair => 0x1000,
            // Build 12340 click FUN_006ddbb0 and cursor FUN_004f7a50 test bit 23.
            GuildBanker => 0x800000,
            // Build 12340 FUN_0071c570 and cursor FUN_004f7a50 test bit 24.
            SpellClick => 0x1000000,
            // Build 12340 click FUN_006ddbb0 tests distinct bit 26.
            Mailbox => 0x4000000,
        };
        self.bits & mask != 0
    }
    /// Exact base vendor flag, excluding vendor-specialization flags. Use for
    /// client branches that test the base bit, not the broader service union.
    pub fn has_vendor_base(self) -> bool {
        self.bits & 0x80 != 0
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
    use crate::types::Field;

    #[test]
    fn guild_banker_is_exact_wrath_bit_and_absent_in_vanilla() {
        let layout = FieldLayout::Wrath12340;
        for bit in 0..32 {
            let bits = 1u32 << bit;
            let flags = NpcFlags::from_bits(layout, bits);
            assert_eq!(
                flags.has(NpcService::GuildBanker),
                layout == FieldLayout::Wrath12340 && bits == 0x800000
            );
            assert_eq!(flags.bits(), bits);
        }
        assert!(!NpcFlags::from_bits(layout, 0).has(NpcService::GuildBanker));
        assert_eq!(
            NpcFlags::from_bits(layout, u32::MAX).has(NpcService::GuildBanker),
            layout == FieldLayout::Wrath12340
        );
    }

    #[test]
    fn spellclick_and_mailbox_are_distinct_wrath_only_presence_flags() {
        for (service, mask) in [
            (NpcService::SpellClick, 0x1000000),
            (NpcService::Mailbox, 0x4000000),
        ] {
            let layout = FieldLayout::Wrath12340;
            for bit in 0..32 {
                let bits = 1u32 << bit;
                let flags = NpcFlags::from_bits(layout, bits);
                assert_eq!(
                    flags.has(service),
                    layout == FieldLayout::Wrath12340 && bits == mask
                );
                assert_eq!(flags.bits(), bits);
            }
            assert!(!NpcFlags::from_bits(layout, 0).has(service));
            assert_eq!(
                NpcFlags::from_bits(layout, u32::MAX).has(service),
                layout == FieldLayout::Wrath12340
            );
        }
    }

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
        let layout = FieldLayout::Wrath12340;
        let vendor_bit = 0x80;
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

    #[test]
    fn services_match_their_native_bits() {
        use NpcService::*;
        for (service, wrath) in [
            (Gossip, 1),
            (QuestGiver, 2),
            (Vendor, 0x80),
            (FlightMaster, 0x2000),
            (Trainer, 0x10),
            (SpiritHealer, 0x4000),
            (SpiritGuide, 0x8000),
            (Innkeeper, 0x10000),
            (Banker, 0x20000),
            (Petitioner, 0x40000),
            (TabardDesigner, 0x80000),
            (BattleMaster, 0x100000),
            (Auctioneer, 0x200000),
            (StableMaster, 0x400000),
            (Repair, 0x1000),
        ] {
            let (layout, bits) = (FieldLayout::Wrath12340, wrath);
            let span = layout.resolve(Field::UnitNpcFlags).unwrap();
            let fields =
                ObjectFields::from_native_pairs(layout, &[(span.index, bits | 0x80000000)]);
            let flags = fields.unit_npc_services();
            assert_eq!(flags.bits(), bits | 0x80000000);
            assert_eq!(flags.layout(), layout);
            assert!(flags.has(service));
            assert!(!NpcFlags::from_bits(layout, 0).has(service));
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
    }
}
