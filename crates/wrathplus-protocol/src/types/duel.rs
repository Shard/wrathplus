//! The duel family: challenge, countdown, bounds, completion and the winner line. The 1.12
//! client's handlers are registered at `0x4d4710`; the two bounds messages have empty bodies.

/// `SMSG_DUEL_REQUESTED`, a duel challenge sent to both parties (vmangos `EffectDuel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuelRequested {
    /// The duel flag game object (entry 21680) between the players: the bounds anchor, the
    /// `PLAYER_DUEL_ARBITER` value, and the guid echoed on accept and cancel.
    pub arbiter: u64,
    /// Who challenged; when it is us, the 1.12 client shows "You have requested a duel." and
    /// auto-accepts (`0x4d49d0`).
    pub challenger: u64,
}

/// `SMSG_DUEL_WINNER`, the outcome line, broadcast around the loser (`Player::DuelComplete`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuelWinner {
    /// True when the loser fled or forfeited (`DUEL_WINNER_RETREAT`), false for a knockout
    /// (`DUEL_WINNER_KNOCKOUT`); the 1.12 client picks the line by it (`0x4d4bd9`).
    pub fled: bool,
    /// The winner's name, `%1$s`.
    pub winner: String,
    /// The loser's name, `%2$s`.
    pub loser: String,
}
