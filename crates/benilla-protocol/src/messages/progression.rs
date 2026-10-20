//! Character-progression messages: XP, exploration, level-up, talent learning and talent wipe.

/// `SMSG_LOG_XPGAIN`: an XP award; `victim` is 0 for non-kill XP (vmangos `Misc.cpp:512-522`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XpGain {
    pub victim: u64,
    pub total: u32,
    /// The award before the rested bonus; only a kill carries it, else it equals `total`.
    /// `total - base` is the chat line's "(+N exp Rested bonus)".
    pub base: u32,
    pub kill: bool,
}

/// `SMSG_EXPLORATION_EXPERIENCE`: a first visit to an area, sent even for 0 XP
/// (`Player.cpp:6228-6341`). The XP also arrives as a non-kill [`XpGain`]; this names the area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplorationXp {
    /// The `AreaTable.dbc` row id, not the explore-flag bit.
    pub area_id: u32,
    /// The XP awarded; 0 at max level or for an area without a level.
    pub xp: u32,
}

/// Inbound `MSG_TALENT_WIPE_CONFIRM`: a class trainer asks whether to unlearn every talent
/// (vmangos `Player.cpp:8414`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TalentWipeConfirm {
    /// The asking trainer, echoed in the answer and required to be a trainer in range there.
    /// Zero means "you have no talents to reset" (vmangos `SkillHandler.cpp:52`).
    pub trainer: u64,
    /// Cost in copper, climbing with every reset (`Player::GetResetTalentsCost`).
    pub cost: u32,
}

/// Body of the outbound `MSG_TALENT_WIPE_CONFIRM`, the `CONFIRM_TALENT_WIPE` Accept: the trainer
/// guid alone (vmangos `Skill.cpp:14-17`), as the reference latches it at `0xc4d7a0`.
pub fn talent_wipe_confirm(trainer_guid: u64) -> Vec<u8> {
    trainer_guid.to_le_bytes().to_vec()
}

/// `SMSG_LEVELUP_INFO`: our own level-up, sent only to us with no guid (`Player.cpp:3210-3218`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelUpInfo {
    /// The level just reached.
    pub level: u32,
    /// Hit points gained.
    pub health: u32,
    /// Power gains: mana, rage, focus, energy, happiness. The server fills only mana, which is
    /// the reference chat line's mana argument.
    pub powers: [u32; 5],
    /// Stat gains in `SPELL_STAT0..4` order (str, agi, stam, int, spirit), one chat line per
    /// positive entry.
    pub stats: [u32; 5],
}
