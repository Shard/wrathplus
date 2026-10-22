//! Shared spline-facing and creature-query types.

/// The final facing a `SMSG_MONSTER_MOVE` dictates (`moveType`), applied as a hard snap, not a
/// turn (`0x6018f0`); `Spot` and `Target` resolve once, from positions at receipt.
#[derive(Debug, Clone, Copy)]
pub enum MonsterMoveFacing {
    /// `moveType` 0 or a stop: the unit faces its travel direction.
    None,
    /// `moveType` 2: face a world point (raw WoW coords).
    Spot([f32; 3]),
    /// `moveType` 3: face a unit by guid, resolved to its position when applied.
    Target(u64),
    /// `moveType` 4: face a raw orientation (radians, WoW convention).
    Angle(f32),
}

/// One creature template's UI-visible head, from `SMSG_CREATURE_QUERY_RESPONSE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatureQueryInfo {
    pub name: String,
    /// The NPC subtitle ("Stable Master"), the unit tooltip's second line.
    pub subname: String,
    /// `CreatureType.dbc` id: the tooltip level line's type word and the TAB-target filter.
    pub creature_type: u32,
    /// `CreatureFamily.dbc` id; `0`, read as nil, for all but tameable beasts and warlock minions.
    pub pet_family: u32,
    /// 0 normal, 1 elite, 2 rare-elite, 3 world boss, 4 rare (tooltip: "", Elite, Elite, Boss, "").
    pub rank: u32,
    /// Bit `0x10` (HIDE_FACTION_TOOLTIP) drops the tooltip's faction line (`0x612610`).
    pub type_flags: u32,
    /// The tooltip's green CIVILIAN line and the dishonorable-kill mark.
    pub civilian: bool,
    /// The tooltip's white LEADER line (`0x6125c0`).
    pub racial_leader: bool,
    /// `creature_template.display_id[0]` (`QueryHandler.cpp:179`), `0` if none, for drawing an
    /// unseen creature; a spawned unit may use any of its four ids, so its own display id wins.
    pub display_id: u32,
    /// Wrath's HealthModifier/PowerModifier template floats as raw bits (`Eq` stays derivable);
    /// `None` where the dialect's packet carries none (1.12).
    pub modifiers: Option<[u32; 2]>,
}
