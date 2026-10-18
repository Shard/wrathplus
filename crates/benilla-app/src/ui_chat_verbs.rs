//! Build 12340's slash-command sends that read no selection and cast nothing
//! ([`WrathChatVerbRequest`], `/tmp/azb-app-mig/ghidra/chatverbs.log`): the arena-team verbs. The
//! selection, swing and cast verbs apply in call order with the script calls
//! ([`crate::script_calls`]).
//!
//! `ArenaTeamInviteByName` (`0x515cc0`), `ArenaTeamLeave` (`0x515dd0`) and the uninvite and leader
//! bindings send their team slot's id, 0 for a slot out of 1-3, which still goes out. A name must
//! be non-empty and under 49 characters; a longer one is the binding's "Name too long" error,
//! which this drain logs (labelled). Labelled: the team id is read off the descriptor's
//! `ARENA_TEAM_INFO` slot, where native reads its own arena-team table (`0xc0f840`). Pending a
//! writer: `SetCurrentTitle` (CMSG `0x374`) and `SetTaxiBenchmarkMode` (CMSG `0x389`).

use bevy::prelude::*;

use benilla_protocol::wrath_arena_team::Command as ArenaTeam;
use benilla_ui::script::{UiScript, WrathChatVerbRequest};

use crate::net::{ClientCommand, NetCommands, ObjectStore};

/// `0x76ee30`'s character count must stay under `0x31`.
const MAX_NAME_CHARS: usize = 48;

/// One arena verb's command, or `None` for a name the binding refuses.
pub(crate) fn arena_command(
    request: &WrathChatVerbRequest,
    team_id: impl Fn(u8) -> u32,
) -> Option<ArenaTeam> {
    let named = |name: &str| {
        let chars = name.chars().count();
        (chars > 0 && chars <= MAX_NAME_CHARS).then(|| name.as_bytes().to_vec())
    };
    Some(match request {
        WrathChatVerbRequest::ArenaTeamInvite { team, name } => ArenaTeam::Invite {
            team_id: team_id(*team),
            name: named(name)?,
        },
        WrathChatVerbRequest::ArenaTeamLeave { team } => ArenaTeam::Leave {
            team_id: team_id(*team),
        },
        WrathChatVerbRequest::ArenaTeamUninvite { team, name } => ArenaTeam::Remove {
            team_id: team_id(*team),
            name: named(name)?,
        },
        WrathChatVerbRequest::ArenaTeamSetLeader { team, name } => ArenaTeam::Leader {
            team_id: team_id(*team),
            name: named(name)?,
        },
        _ => return None,
    })
}

/// Drain the frame's sends. Folded into [`crate::ui_talent`]'s learn drain, which reads the same
/// VM, self store and commands, so it adds no schedule edge of its own.
pub(crate) fn drain(script: &mut UiScript, store: Option<&ObjectStore>, commands: &NetCommands) {
    let requests = script.take_wrath_chat_verb_requests();
    if requests.is_empty() {
        return;
    }
    // The 1-based slot's id; 0 out of range, as native sends it.
    let team_id = |team: u8| {
        team.checked_sub(1)
            .filter(|&slot| slot < 3)
            .and_then(|slot| store.and_then(|s| s.0.player_arena_team_id(slot)))
            .unwrap_or(0)
    };
    for request in requests {
        match arena_command(&request, team_id) {
            Some(command) => {
                let _ = commands.0.send(ClientCommand::ArenaTeamCommand(command));
            }
            None => debug!("chat verb {request:?}: not sent (refused name or no writer yet)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_verbs_send_their_slots_team_id_and_refuse_bad_names() {
        let id = |team: u8| u32::from(team) * 100;
        let name = |s: &str| s.to_string();
        assert_eq!(
            arena_command(
                &WrathChatVerbRequest::ArenaTeamInvite {
                    team: 2,
                    name: name("Bob")
                },
                id
            ),
            Some(ArenaTeam::Invite {
                team_id: 200,
                name: b"Bob".to_vec()
            })
        );
        assert_eq!(
            arena_command(&WrathChatVerbRequest::ArenaTeamLeave { team: 0 }, |_| 0),
            Some(ArenaTeam::Leave { team_id: 0 })
        );
        assert_eq!(
            arena_command(
                &WrathChatVerbRequest::ArenaTeamSetLeader {
                    team: 1,
                    name: name("Ann")
                },
                id
            ),
            Some(ArenaTeam::Leader {
                team_id: 100,
                name: b"Ann".to_vec()
            })
        );
        for bad in ["", &"x".repeat(49)] {
            assert_eq!(
                arena_command(
                    &WrathChatVerbRequest::ArenaTeamUninvite {
                        team: 1,
                        name: name(bad)
                    },
                    id
                ),
                None
            );
        }
        assert_eq!(
            arena_command(
                &WrathChatVerbRequest::ArenaTeamUninvite {
                    team: 1,
                    name: "é".repeat(48)
                },
                id
            ),
            Some(ArenaTeam::Remove {
                team_id: 100,
                name: "é".repeat(48).into_bytes()
            })
        );
    }
}
