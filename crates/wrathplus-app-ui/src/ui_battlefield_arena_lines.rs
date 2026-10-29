//! The arena-team system lines: `SMSG_ARENA_TEAM_COMMAND_RESULT` (0349), `SMSG_ARENA_TEAM_EVENT`
//! (0357) and `SMSG_ARENA_ERROR` (0376), drained from [`crate::ui_battlefield::NativeBattlegroundRest`]
//! after Experience's reducer takes its own. Each becomes an `ERR_ARENA_*` catalog line, whose row
//! picks the surface and the sound. The three are 12340's handlers: the command result `0x6ccb20`,
//! the event `0x6cc980` (its line, then every slot's roster marked stale,
//! [`crate::ui_arena_team::ArenaRosterStale`]) and the error `0x6cce90`.
//! `SMSG_ARENA_UNIT_DESTROYED` (04C7) stays in the rest FIFO: no arena-opponent model takes it.
use crate::app_core::errors::FillArg;
use crate::app_core::errors::UiError;
use bevy::prelude::*;
use wrathplus_protocol::wrath_battleground::Packet;

pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        drain_arena_lines
            .in_set(crate::app_core::schedule::UiFeed)
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

/// The line one `SMSG_ARENA_TEAM_COMMAND_RESULT` prints, if any, as 12340's `0x6ccb20`: on error 0
/// the action picks it, otherwise the error code does. Each `_S` row takes one fixed field, the
/// player or the team, even when the server left that one empty.
fn command_line(action: u32, team: &str, player: &str, error: u32) -> Option<UiError> {
    let player_s = |key: &'static str| Some(UiError::s(key, player));
    let team_s = |key: &'static str| Some(UiError::s(key, team));
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
        0x03 => player_s("ERR_ALREADY_IN_ARENA_TEAM_S"),
        0x04 => Some(UiError::key("ERR_INVITED_TO_ARENA_TEAM")),
        0x05 => player_s("ERR_ALREADY_INVITED_TO_ARENA_TEAM_S"),
        0x06 => Some(UiError::key("ERR_ARENA_TEAM_NAME_INVALID")),
        0x07 => team_s("ERR_ARENA_TEAM_NAME_EXISTS_S"),
        // `0x08` is the captain's leave refusal under `QUIT`, a permission refusal otherwise.
        0x08 if action == action::QUIT => team_s("ERR_ARENA_TEAM_LEADER_LEAVE_S"),
        0x08 => Some(UiError::key("ERR_ARENA_TEAM_PERMISSIONS")),
        0x09 => Some(UiError::key("ERR_ARENA_TEAM_PLAYER_NOT_IN_TEAM")),
        0x0a => Some(UiError::strings(
            "ERR_ARENA_TEAM_PLAYER_NOT_IN_TEAM_SS",
            &[player, team],
        )),
        0x0b => player_s("ERR_ARENA_TEAM_PLAYER_NOT_FOUND_S"),
        0x0c => Some(UiError::key("ERR_ARENA_TEAM_NOT_ALLIED")),
        0x13 => player_s("ERR_IGNORING_YOU_S"),
        0x15 => player_s("ERR_ARENA_TEAM_TARGET_TOO_LOW_S"),
        0x16 => player_s("ERR_ARENA_TEAM_TARGET_TOO_HIGH_S"),
        0x17 => team_s("ERR_ARENA_TEAM_TOO_MANY_MEMBERS_S"),
        0x1b => Some(UiError::key("ERR_ARENA_TEAM_NOT_FOUND")),
        0x1e => Some(UiError::key("ERR_ARENA_TEAMS_LOCKED")),
        _ => None,
    }
}

/// The line one `SMSG_ARENA_TEAM_EVENT` prints, its strings as they arrived (`0x6cc980`'s switch,
/// message ids `0x216`-`0x21c`). A join naming us first is "You have joined %s." with the team;
/// an unknown event prints row `0x72`, `ERR_GUILD_INTERNAL`. Labelled: the own-name test
/// (`0x76ea40`) is read as a case-blind compare, and its copy (`0x76ed20`) as team over name.
fn event_line(event: u8, strings: &[String], me: Option<&str>) -> UiError {
    let args: Vec<&str> = strings.iter().map(String::as_str).collect();
    let key = match event {
        3 if me.is_some_and(|me| args.first().is_some_and(|n| n.eq_ignore_ascii_case(me))) => {
            return UiError::s(
                "ERR_ARENA_TEAM_YOU_JOIN_S",
                args.get(1).copied().unwrap_or(""),
            );
        }
        3 => "ERR_ARENA_TEAM_JOIN_SS",
        4 => "ERR_ARENA_TEAM_LEAVE_SS",
        5 => "ERR_ARENA_TEAM_REMOVE_SSS",
        6 => "ERR_ARENA_TEAM_LEADER_IS_SS",
        7 => "ERR_ARENA_TEAM_LEADER_CHANGED_SSS",
        8 => "ERR_ARENA_TEAM_DISBANDED_S",
        _ => "ERR_GUILD_INTERNAL",
    };
    UiError::strings(key, &args)
}

/// `SMSG_ARENA_ERROR` (`0x6cce90`): a leading 0 carries the team type, "You are not in a %dv%d
/// arena team."; 1 and 2 are rows `0x253` and `0x2c5`; anything else prints nothing.
fn error_line(raw_word: u32, team_type: Option<u8>) -> Option<UiError> {
    match raw_word {
        0 => {
            let size = team_type?;
            Some(UiError::args(
                "ERR_ARENA_NO_TEAM_II",
                vec![FillArg::D(i64::from(size)), FillArg::D(i64::from(size))],
            ))
        }
        1 => Some(UiError::key("ERR_ARENA_EXPIRED_CAIS")),
        2 => Some(UiError::key("ERR_LFG_CANT_USE_BATTLEGROUND")),
        _ => None,
    }
}

fn drain_arena_lines(
    mut rest: ResMut<crate::ui_battlefield::NativeBattlegroundRest>,
    mut errors: ResMut<crate::app_core::errors::UiErrorKeys>,
    mut stale: ResMut<crate::ui_arena_team::ArenaRosterStale>,
    self_guid: Res<crate::app_core::session::SelfGuid>,
    names: Res<crate::names::NameCache>,
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
            // Every event, known or not, then marks the three slots stale.
            Packet::ArenaEvent { event, strings, .. } => {
                stale.0 += 1;
                let me = self_guid.0.and_then(|g| names.answered_player_name(g));
                Some(event_line(event, &strings, me))
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
#[path = "ui_battlefield_arena_lines_tests.rs"]
mod tests;
