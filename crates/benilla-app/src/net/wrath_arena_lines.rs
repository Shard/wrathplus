//! The arena-team system lines: `SMSG_ARENA_TEAM_COMMAND_RESULT` (0349), `SMSG_ARENA_TEAM_EVENT`
//! (0357) and `SMSG_ARENA_ERROR` (0376), drained from [`crate::ui_battlefield::NativeBattlegroundRest`]
//! after Experience's reducer takes its own. Each becomes an `ERR_ARENA_*` catalog line, whose row
//! picks the surface and the sound; an event also re-asks the named team's roster
//! ([`crate::ui_arena_team::ArenaRosterRefresh`], the team name being its last string).
//!
//! Labelled: the client's arena handlers are untraced. The tables mirror the guild command-result
//! and event handlers (`crate::ui_guild` `lines`, `0x5e7520`, `0x5e7180`), whose codes AzerothCore's
//! `ArenaTeamCommandErrors` and `ArenaTeamEvents` copy (`ArenaTeam.h:30-69`). A `_S` result takes
//! the player name, else the team name, as AC fills either field; 0376 is
//! `ERR_ARENA_NO_TEAM_II` with the team type twice. `SMSG_ARENA_UNIT_DESTROYED` (04C7) stays in
//! the rest FIFO: no arena-opponent model takes it.
use crate::ui_action::{FillArg, UiError};
use benilla_protocol::wrath_battleground::Packet;
use bevy::prelude::*;

pub(super) fn register(app: &mut App) {
    app.add_systems(
        Update,
        drain_arena_lines
            .in_set(crate::ui_script::UiFeed)
            .after(crate::ui_battlefield::drain_native),
    );
}

/// `ArenaTeamCommandTypes` (`ArenaTeam.h:30-36`).
mod action {
    pub(super) const CREATE: u32 = 0x00;
    pub(super) const INVITE: u32 = 0x01;
    pub(super) const QUIT: u32 = 0x03;
    pub(super) const FOUNDER: u32 = 0x0e;
}

/// The line one `SMSG_ARENA_TEAM_COMMAND_RESULT` prints, if any: on error 0 the action picks it,
/// otherwise the error code does.
fn command_line(action: u32, team: &str, player: &str, error: u32) -> Option<UiError> {
    let name = if player.is_empty() { team } else { player };
    let named = |key: &'static str| Some(UiError::s(key, name));
    if error == 0 {
        return match action {
            action::CREATE => Some(UiError::s("ERR_ARENA_TEAM_CREATE_S", team)),
            action::INVITE => Some(UiError::strings(
                "ERR_ARENA_TEAM_INVITE_SS",
                &[player, team],
            )),
            action::QUIT => Some(UiError::s("ERR_ARENA_TEAM_QUIT_S", team)),
            action::FOUNDER => Some(UiError::s("ERR_ARENA_TEAM_FOUNDER_S", team)),
            _ => None,
        };
    }
    match error {
        0x01 => Some(UiError::key("ERR_ARENA_TEAM_INTERNAL")),
        0x02 => Some(UiError::key("ERR_ALREADY_IN_ARENA_TEAM")),
        0x03 => named("ERR_ALREADY_IN_ARENA_TEAM_S"),
        0x04 => Some(UiError::key("ERR_INVITED_TO_ARENA_TEAM")),
        0x05 => named("ERR_ALREADY_INVITED_TO_ARENA_TEAM_S"),
        0x06 => Some(UiError::key("ERR_ARENA_TEAM_NAME_INVALID")),
        0x07 => named("ERR_ARENA_TEAM_NAME_EXISTS_S"),
        // `0x08` is the captain's leave refusal under `QUIT`, a permission refusal otherwise.
        0x08 if action == action::QUIT => Some(UiError::key("ERR_ARENA_TEAM_LEADER_LEAVE_S")),
        0x08 => Some(UiError::key("ERR_ARENA_TEAM_PERMISSIONS")),
        0x09 => Some(UiError::key("ERR_ARENA_TEAM_PLAYER_NOT_IN_TEAM")),
        0x0a => Some(UiError::strings(
            "ERR_ARENA_TEAM_PLAYER_NOT_IN_TEAM_SS",
            &[player, team],
        )),
        0x0b => named("ERR_ARENA_TEAM_PLAYER_NOT_FOUND_S"),
        0x0c => Some(UiError::key("ERR_ARENA_TEAM_NOT_ALLIED")),
        0x13 => named("ERR_IGNORING_YOU_S"),
        0x15 => named("ERR_ARENA_TEAM_TARGET_TOO_LOW_S"),
        0x16 => named("ERR_ARENA_TEAM_TARGET_TOO_HIGH_S"),
        0x17 => named("ERR_ARENA_TEAM_TOO_MANY_MEMBERS_S"),
        0x1b => Some(UiError::key("ERR_ARENA_TEAM_NOT_FOUND")),
        0x1e => Some(UiError::key("ERR_ARENA_TEAMS_LOCKED")),
        _ => None,
    }
}

/// The line one `SMSG_ARENA_TEAM_EVENT` prints, its strings as they arrived.
fn event_line(event: u8, strings: &[String]) -> Option<UiError> {
    let args: Vec<&str> = strings.iter().map(String::as_str).collect();
    let key = match event {
        3 => "ERR_ARENA_TEAM_JOIN_SS",
        4 => "ERR_ARENA_TEAM_LEAVE_SS",
        5 => "ERR_ARENA_TEAM_REMOVE_SSS",
        6 => "ERR_ARENA_TEAM_LEADER_IS_SS",
        7 => "ERR_ARENA_TEAM_LEADER_CHANGED_SSS",
        8 => "ERR_ARENA_TEAM_DISBANDED_S",
        _ => return None,
    };
    Some(UiError::strings(key, &args))
}

/// `SMSG_ARENA_ERROR`: a leading 0 carries the team type, "You are not in a %dv%d arena team."
fn error_line(raw_word: u32, team_type: Option<u8>) -> Option<UiError> {
    let size = team_type.filter(|_| raw_word == 0)?;
    Some(UiError::args(
        "ERR_ARENA_NO_TEAM_II",
        vec![FillArg::D(i64::from(size)), FillArg::D(i64::from(size))],
    ))
}

fn drain_arena_lines(
    mut rest: ResMut<crate::ui_battlefield::NativeBattlegroundRest>,
    mut errors: ResMut<crate::ui_action::UiErrorKeys>,
    mut refresh: ResMut<crate::ui_arena_team::ArenaRosterRefresh>,
) {
    if rest.0.is_empty() {
        return;
    }
    let mut kept = std::collections::VecDeque::new();
    for packet in rest.0.drain(..) {
        let line = match packet {
            Packet::ArenaCommandResult {
                action,
                team,
                player,
                error,
            } => command_line(action, &team, &player, error),
            Packet::ArenaEvent { event, strings, .. } => {
                let line = event_line(event, &strings);
                if line.is_some() {
                    refresh.0.extend(strings.last().cloned());
                }
                line
            }
            Packet::ArenaError {
                raw_word,
                team_type,
            } => error_line(raw_word, team_type),
            other => {
                kept.push_back(other);
                None
            }
        };
        errors.0.extend(line);
    }
    rest.0 = kept;
}

#[cfg(test)]
#[path = "wrath_arena_lines_tests.rs"]
mod tests;
