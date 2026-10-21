//! The combat log wire, inbound only: spell damage, periodic ticks, heals, power gains, damage
//! shields, environmental damage, misses, kills, dispels, enchants and spell effect logs.

/// `SMSG_SPELLNONMELEEDAMAGELOG`, spell damage dealt (`Server/Packets/Spell.cpp:124-140`);
/// `hit_info` bit `0x2` is a crit (`SpellDefines.h:179`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellDamageLog {
    pub target: u64,
    pub attacker: u64,
    pub spell_id: u32,
    pub damage: u32,
    pub school: u8,
    pub absorb: u32,
    pub resist: i32,
    pub periodic: bool,
    pub blocked: u32,
    pub hit_info: u32,
    /// Build 12340's overkill word (AC `Unit::SendSpellNonMeleeDamageLog`); 0 on 1.12.
    pub overkill: u32,
}

/// One `SMSG_PERIODICAURALOG` tick, shaped by its aura type (vmangos `SpellAuraDefines.h`).
/// `overkill`, `overheal`, heal `absorb` and `critical` are build 12340's (AC
/// `Unit::SendPeriodicAuraLog`); 1.12 leaves them 0 and false.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PeriodicTick {
    Damage {
        amount: u32,
        /// The `SPELL_SCHOOL<n>_NAME` index.
        school: u32,
        absorb: u32,
        resist: i32,
        overkill: u32,
        critical: bool,
    },
    Heal {
        amount: u32,
        overheal: u32,
        absorb: u32,
        critical: bool,
    },
    Energize {
        power: u32,
        amount: u32,
    },
    ManaLeech {
        power: u32,
        amount: u32,
        multiplier: f32,
    },
}

/// `SMSG_PERIODICAURALOG`, periodic aura ticks (vmangos `Unit.cpp:4395-4443`).
#[derive(Debug, Clone, PartialEq)]
pub struct PeriodicAuraLog {
    pub target: u64,
    pub caster: u64,
    pub spell_id: u32,
    pub ticks: Vec<PeriodicTick>,
}

/// `SMSG_SPELLHEALLOG`, a direct heal landing (`Server/Packets/Spell.cpp:105-112`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellHealLog {
    pub target: u64,
    pub healer: u64,
    pub spell_id: u32,
    pub amount: u32,
    pub critical: bool,
    /// Build 12340's overheal and heal-absorb words (AC `Unit::SendHealSpellLog`); 0 on 1.12.
    pub overheal: u32,
    pub absorb: u32,
}

/// `SMSG_SPELLENERGIZELOG`, an instant power gain (`Server/Packets/Spell.cpp:114-121`); `power` is
/// 0 mana, 1 rage, 2 focus, 3 energy, 4 happiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellEnergizeLog {
    pub target: u64,
    pub caster: u64,
    pub spell_id: u32,
    pub power: u32,
    pub amount: u32,
}

/// `SMSG_SPELLDAMAGESHIELD`, a Thorns-style return hit (`Server/Packets/Combat.cpp:73-79`):
/// `victim` bears the shield, `attacker` struck it and takes this damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageShield {
    pub victim: u64,
    pub attacker: u64,
    pub damage: u32,
    /// The `SPELL_SCHOOL<n>_NAME` index.
    pub school: u32,
    /// Build 12340's shield spell and overkill (AC `Unit::DealMeleeDamage`); 0 on 1.12.
    pub spell_id: u32,
    pub overkill: u32,
}

/// `SMSG_ENVIRONMENTALDAMAGELOG`, environmental damage taken (`Server/Packets/Combat.cpp:58-67`).
/// `damage_type` is 0 exhausted, 1 drowning, 2 fall, 3 lava, 4 slime, 5 fire (`Player.h:590`), the
/// row of `EnvironmentalDamage.dbc` whose visual kit plays (fall's is the landing dust puff).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvironmentalDamageLog {
    pub victim: u64,
    pub damage_type: u8,
    pub damage: u32,
    pub absorb: u32,
    pub resist: i32,
}

/// `SMSG_SPELLLOGMISS`, a cast's per-target miss list (`Server/Packets/Spell.cpp:68-86`). Each
/// `u8` is a `SpellMissInfo` (`SpellDefines.h:160-174`): 1 miss, 2 resist, 3 dodge, 4 parry,
/// 5 block, 6 evade, 7 and 8 immune, 9 deflect, 10 absorb, 11 reflect.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellLogMiss {
    pub spell_id: u32,
    pub caster: u64,
    pub misses: Vec<(u64, u8)>,
}

/// `SMSG_PARTYKILLLOG`, the killing blow, sent to the killer's party (`Combat.cpp:52-56`): the
/// only source of "You have slain %s!"; "%s dies." comes from the client's own death handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartyKillLog {
    pub killer: u64,
    pub victim: u64,
}

/// `SMSG_SPELLINSTAKILLLOG`, an instant kill (vmangos `Spells/SpellEffects.cpp:274-279`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellInstaKillLog {
    pub victim: u64,
    pub spell_id: u32,
    /// Build 12340 names the caster first (AC `Spell::EffectInstaKill`); 0 on 1.12.
    pub caster: u64,
}

/// `SMSG_PROCRESIST` or `SMSG_SPELLORDAMAGE_IMMUNE`, which share one body
/// (`Server/Packets/Spell.cpp:88-102`). The 1.12 client reads `log_format` as the "is periodic"
/// flag that lets `CombatLogPeriodicSpells` gate the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellOutcomeLog {
    pub caster: u64,
    pub target: u64,
    pub spell_id: u32,
    pub log_format: u8,
}

/// `SMSG_SPELLDISPELLOG`, the auras a dispel removed (vmangos `SpellEffects.cpp:2524-2539`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellDispelLog {
    /// The unit the auras were on.
    pub victim: u64,
    /// The dispeller; the `AURADISPEL*` line never names them.
    pub caster: u64,
    /// Build 12340's dispelling spell and its header byte (AC writes 0, "not used"); 0 on 1.12.
    pub spell_id: u32,
    pub flag: u8,
    pub auras: Vec<DispelledAura>,
}

/// One removed aura. `flag` is build 12340's per-aura byte (AC: 0 dispelled, non-zero cleansed;
/// on `SMSG_SPELLSTEALLOG` 0 stolen, non-zero transferred); 0 on 1.12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispelledAura {
    pub spell_id: u32,
    pub flag: u8,
}

/// `SMSG_DISPEL_FAILED`, the auras a dispel failed to remove (`SpellEffects.cpp:2549-2555`). No
/// count: the spell ids run to the end of the body, as the 1.12 client reads them (`0x628c20`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispelFailed {
    pub caster: u64,
    pub victim: u64,
    /// Build 12340's dispelling spell, ahead of the list (AC `Spell::EffectDispel`); 0 on 1.12.
    pub spell_id: u32,
    pub spell_ids: Vec<u32>,
}

/// `SMSG_ENCHANTMENTLOG`, an enchant landing on or fading from an item (AzerothCore
/// `WorldPackets::Item::EnchantmentLog`, sent by `Item::SetEnchantment`): packed target and caster,
/// `u32 item, u32 enchant`. A zero `caster` is the removal of the item's previous enchant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantmentLog {
    /// The item's owner.
    pub target: u64,
    pub caster: u64,
    pub item_entry: u32,
    /// A `SpellItemEnchantment.dbc` id, not a spell.
    pub enchant_id: u32,
}

/// One `SMSG_SPELLLOGEXECUTE` row, shaped by its effect id as vmangos writes it
/// (`Spells/Spell.cpp:4694-4771`) and the 1.12 client reads it (`0x5e8074`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecuteLog {
    /// Effect 8 `POWER_DRAIN`: amount, then power (the 1.12 client, `0x5e807b` and `0x62793c`).
    PowerDrain {
        target: u64,
        amount: u32,
        power: u32,
        multiplier: f32,
    },
    /// Effects 10 `HEAL` / 62 `HEAL_MAX_HEALTH`.
    Heal {
        target: u64,
        amount: u32,
        critical: bool,
    },
    /// Effect 30 `ENERGIZE`.
    Energize {
        target: u64,
        amount: u32,
        power: u32,
    },
    /// Effect 19 `ADD_EXTRA_ATTACKS`: "You gain %d extra attacks through %s."
    ExtraAttacks { target: u64, count: u32 },
    /// Effect 24 `CREATE_ITEM`, the tradeskill line: no target guid, only the item entry.
    CreateItem { item_entry: u32 },
    /// Effect 68 `INTERRUPT_CAST`: the interrupted target and spell.
    InterruptCast { target: u64, spell_id: u32 },
    /// Effect 101 `FEED_PET`: an item entry alone.
    FeedPet { item_entry: u32 },
    /// Effect 111 `DURABILITY_DAMAGE`, both fields signed; `-1`, `-1` is the all-items form
    /// (`SPELLDURABILITYDAMAGEALL*`). The 1.12 client reads vmangos's `unk` as the item slot.
    DurabilityDamage {
        target: u64,
        item_entry: i32,
        slot: i32,
    },
    /// Every other effect vmangos logs: a bare target guid.
    Target { target: u64 },
}

/// `SMSG_SPELLLOGEXECUTE`, what a cast's effects did (`Spells/Spell.cpp:4662-4778`): one group of
/// rows per logging effect, keyed by the effect id that picks the combat-log line.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellLogExecute {
    pub caster: u64,
    pub spell_id: u32,
    /// `(effect id, the rows that effect logged)`, in wire order.
    pub effects: Vec<(u32, Vec<ExecuteLog>)>,
}

// vmangos `SpellEffects.h` effect ids, named for the arms that read a payload wider than a guid.
