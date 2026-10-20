//! Melee auto-attack messages: the attack start and stop edges, the per-swing report, and the
//! creature-aggro notice, which vmangos sends from `Unit::Attack`.

/// `SMSG_ATTACKERSTATEUPDATE`: one melee swing (`Unit.cpp:4572-4605`), sent once per weapon-timer
/// cycle per hand; the reference plays one swing animation per packet. `hit_info` bit `0x4` marks
/// an offhand swing, bit `0x10000` suppresses the swing animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackerState {
    pub attacker: u64,
    pub victim: u64,
    pub hit_info: u32,
    /// `TotalDamage`: the whole swing, before the sub-damage split.
    pub damage: u32,
    /// `TargetState` (vmangos `VictimState`): 1 hit, 2 dodge, 3 parry, 4 interrupt, 5 blocks, …
    pub victim_state: u32,
    /// The sub-damage blocks' `absorb`, summed (vmangos writes one block).
    pub absorb: u32,
    /// The sub-damage blocks' `resist`, summed: the partial-resist amount.
    pub resist: i32,
    /// `BlockedAmount`, the trailing blocked-damage word.
    pub blocked: u32,
    /// `meleeSpellId` (`rec+0xac`), nonzero for an on-next-swing ability such as Heroic Strike; the
    /// reference only ever compares it to zero (`0x625e40`, `0x6246ca`).
    pub melee_spell_id: u32,
    /// The first sub-damage block's school (0 physical to 6 arcane, `GetFirstSchoolInMask`), 0
    /// with no blocks; a non-physical swing takes the combat log's `…SCHOOL` wording.
    pub school: u8,
}

impl AttackerState {
    /// `0x625e40`: whether the swing gets a combat-log line (`0x629b10`) and a floating number
    /// (`0x6243e0`). A dodged or parried Heroic Strike gets neither, yet still animates and sounds.
    #[must_use]
    pub fn displayed(&self) -> bool {
        self.melee_spell_id == 0 || self.victim_state == 1
    }
}

/// The server's refusal of `CMSG_ATTACKSWING`; every body is empty. Four opcodes, three variants:
/// the reference (`0x6255b0`) handles `0x148` and `0x149` in one shared arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackSwingError {
    /// `SMSG_ATTACKSWING_NOTINRANGE` (`0x145`): the reference latches code 1 (`0x625a8a`).
    NotInRange,
    /// `SMSG_ATTACKSWING_BADFACING` (`0x146`): the reference latches code 2 (`0x625aa1`).
    BadFacing,
    /// `SMSG_ATTACKSWING_DEADTARGET` (`0x148`) or `SMSG_ATTACKSWING_CANT_ATTACK` (`0x149`): no
    /// message, the reference only stops the attack (`0x625ab8`).
    DeadOrUnattackable,
}
