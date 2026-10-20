//! Trainer window wire (opcodes 432-436). The window opens off the gossip trainer option
//! (`GOSSIP_OPTION_TRAINER`), so there is no open verb, only a list refresh and a buy.

/// One 38-byte `SMSG_TRAINER_LIST` service (`NPCHandler.cpp:97-139`); the server already filters
/// the list to the player's class and race.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerSpell {
    /// The service spell id, which `CMSG_TRAINER_BUY_SPELL` names to buy it.
    pub spell: u32,
    /// A [`trainer_spell_state`]: the client renders the colour as sent, never recomputing it.
    pub state: u8,
    /// In copper, already reputation-discounted by the server.
    pub cost: u32,
    /// Sent as `first_rank && can_learn`; the client enables Learn only when it equals
    /// `is_primary_prof_first_rank`, which greys a third primary profession.
    pub can_learn_primary_prof: bool,
    /// Taking it spends a profession slot, so the client asks for confirmation.
    pub is_primary_prof_first_rank: bool,
    /// Native Wrath talent/profession point costs, not Vanilla profession eligibility flags.
    pub wrath_point_cost: Option<[u32; 2]>,
    pub req_level: u8,
    /// A `SkillLine.dbc` id, 0 for none.
    pub req_skill: u32,
    pub req_skill_value: u32,
    /// The `SpellChainNode` req and prev spells, then a slot vmangos always sends as 0.
    pub req_spells: [u32; 3],
}

/// `TrainerSpellState` (`Player.h:119-122`); `GREEN_DISABLED` (10) is sent as `GREEN`.
pub mod trainer_spell_state {
    /// Learnable now.
    pub const GREEN: u8 = 0;
    /// Level, skill or prerequisite unmet.
    pub const RED: u8 = 1;
    /// Already known.
    pub const GRAY: u8 = 2;
}

/// `SMSG_TRAINER_BUY_FAILED` error codes (vmangos `SharedDefines.h:1120-1122` `TRAIN_FAIL_*`).
pub mod train_fail {
    /// Not your trainer, out of line of sight, or not a listed service.
    pub const UNAVAILABLE: u32 = 0;
    pub const NOT_ENOUGH_MONEY: u32 = 1;
    pub const NOT_ENOUGH_SKILL: u32 = 2;
}

/// `CMSG_TRAINER_LIST`: re-requests the list, as after a purchase to turn the service gray.
pub fn trainer_list(trainer_guid: u64) -> Vec<u8> {
    trainer_guid.to_le_bytes().to_vec()
}
