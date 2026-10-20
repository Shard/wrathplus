//! The host side of Native UI's build-12340 UnitPopup seam (`benilla_ui::script` `wrath_unit_popup`):
//! the menu's verbs become wire sends, and the state the menu reads is published.
//!
//! Sends: `SetPVP` the one-byte `CMSG_TOGGLE_PVP` (AC sets the flag to the byte), the dungeon and
//! raid difficulty messages, `CMSG_OPT_OUT_OF_LOOT`, the refer-a-friend `CMSG_GRANT_LEVEL` and
//! `CMSG_REPORT_PVP_AFK`. Voice silencing is not sent: voice chat is out of scope.
//!
//! State, each labelled where it approximates an untraced native check:
//! - grantable: grouped players carrying `UNIT_DYNFLAG_REFER_A_FRIEND` (0x40, which AzerothCore
//!   sets on a recruit-a-friend partner) below the player's level; the native check `0x6de9e0` is
//!   untraced, so this is AzerothCore's own refusal set (`HandleGrantLevel`) as far as the client
//!   can see it (grantable levels left and faction are not visible).
//! - pet_undismissable: the player's pet (`UNIT_FIELD_SUMMON`) was summoned by a spell whose
//!   SUMMON effect names a `SummonProperties.dbc` row with flag 0x20 (`0x5d3820`).
//! - chat_lines: the native chat feed's held lines with their event line ids.
//! - difficulty_locked_until: never set; the source of native's lock stamp (`DAT_00bd1984`) is
//!   untraced, so the change always reads unlocked.

use bevy::prelude::*;

use benilla_formats::SUMMON_FLAG_UNDISMISSABLE;
use benilla_protocol::messages::Field;
use benilla_ui::script::{UiScript, WrathChatLine, WrathUnitPopupRequest, WrathUnitPopupState};

use crate::net::{ClientCommand, Guid, NetCommands, ObjectStore, SelfPlayer};

/// `UNIT_DYNFLAG_REFER_A_FRIEND` (AzerothCore `SharedDefines.h`).
const UNIT_DYNFLAG_REFER_A_FRIEND: u32 = 0x40;
const SPELL_EFFECT_SUMMON: u32 = 28;

pub(crate) struct UiUnitPopupPlugin;

impl Plugin for UiUnitPopupPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (drain_unit_popup, publish_unit_popup).in_set(crate::ui_unit::UnitFeed),
        );
    }
}

/// The wire command for one menu verb; `None` for voice, which is not sent.
fn command_for(request: WrathUnitPopupRequest) -> Option<ClientCommand> {
    use WrathUnitPopupRequest as R;
    Some(match request {
        R::SetPvp { value } => ClientCommand::NativeSessionMisc(
            benilla_protocol::wrath_session_misc::Command::SetPvp { on: value },
        ),
        R::SetDungeonDifficulty { difficulty } => ClientCommand::NativeInstanceControl(
            benilla_protocol::wrath_instance_control::Command::SetDungeonDifficulty {
                mode: difficulty,
            },
        ),
        R::SetRaidDifficulty { difficulty } => ClientCommand::NativeInstanceControl(
            benilla_protocol::wrath_instance_control::Command::SetRaidDifficulty {
                mode: difficulty,
            },
        ),
        R::SetOptOutOfLoot { on } => {
            ClientCommand::NativeExchange(benilla_protocol::wrath_exchange::Command::OptOutOfLoot {
                pass: u32::from(on),
            })
        }
        R::GrantLevel { guid } => ClientCommand::NativeReferFriend(
            benilla_protocol::wrath_refer_friend::Command::GrantLevel { target: guid },
        ),
        R::ReportPvpAfk { guid } => ClientCommand::NativeBattleground(
            benilla_protocol::wrath_battleground::Command::ReportAfk { player: guid },
        ),
        R::ChannelSilenceVoice { .. } => return None,
    })
}

fn drain_unit_popup(script: Option<NonSendMut<UiScript>>, commands: Res<NetCommands>) {
    let Some(mut script) = script else {
        return;
    };
    for request in script.take_wrath_unit_popup_requests() {
        match command_for(request.clone()) {
            Some(command) => {
                let _ = commands.0.send(command);
            }
            None => debug!("unit popup: {request:?} not sent (voice chat is out of scope)"),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn publish_unit_popup(
    script: Option<NonSendMut<UiScript>>,
    me: Query<&ObjectStore, With<SelfPlayer>>,
    units: Query<(&Guid, &ObjectStore), Without<SelfPlayer>>,
    roster: Res<crate::spell::group_relation::GroupRoster>,
    spells: Option<Res<crate::ui_action::Spells>>,
    slots: Option<Res<crate::ui_companions::SummonSlots>>,
    chat: Option<Res<crate::ui_chat::native_message::NativeChatFeed>>,
    mut memo: Local<crate::ui_script::VmMemo<Option<WrathUnitPopupState>>>,
) {
    let Some(mut script) = script else {
        return;
    };
    let me = me.iter().next();
    let my_level = me.and_then(|s| s.0.get(Field::UnitLevel)).unwrap_or(0);
    let grantable = units
        .iter()
        .filter(|(guid, _)| roster.party.contains(&guid.0) || roster.raid.contains(&guid.0))
        .filter(|(_, store)| {
            store.0.get(Field::UnitDynamicFlags).unwrap_or(0) & UNIT_DYNFLAG_REFER_A_FRIEND != 0
                && store.0.get(Field::UnitLevel).unwrap_or(u32::MAX) < my_level
        })
        .map(|(guid, _)| guid.0)
        .collect();
    let pet_spell = me
        .and_then(|s| s.0.get_field_guid(Field::UnitSummon, 0))
        .filter(|&pet| pet != 0)
        .and_then(|pet| units.iter().find(|(g, _)| g.0 == pet))
        .and_then(|(_, store)| store.0.get(Field::UnitCreatedBySpell));
    let pet_undismissable = match (pet_spell, spells.as_deref(), slots.as_deref()) {
        (Some(spell), Some(spells), Some(slots)) => spells.catalog.get(spell).is_some_and(|s| {
            (0..3).any(|i| {
                s.effects[i] == SPELL_EFFECT_SUMMON
                    && u32::try_from(s.effect_misc_value_b[i])
                        .ok()
                        .and_then(|row| slots.0.flags(row))
                        .is_some_and(|f| f & SUMMON_FLAG_UNDISMISSABLE != 0)
            })
        }),
        _ => false,
    };
    let chat_lines = chat
        .map(|feed| {
            feed.texts()
                .map(|(line, sender, text)| WrathChatLine {
                    line_id: line.id as i32,
                    sender: sender.to_string(),
                    text: text.to_string(),
                    guid: line.sender,
                })
                .collect()
        })
        .unwrap_or_default();
    let state = WrathUnitPopupState {
        difficulty_locked_until: None,
        grantable,
        pet_undismissable,
        chat_lines,
    };
    let last = memo.get(&script);
    if last.as_ref() == Some(&state) {
        return;
    }
    *last = Some(state.clone());
    script.set_wrath_unit_popup_state(state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_verb_but_voice_maps_to_its_native_writer() {
        use WrathUnitPopupRequest as R;
        let sent = [
            R::SetPvp { value: 1 },
            R::SetDungeonDifficulty { difficulty: 1 },
            R::SetRaidDifficulty { difficulty: 3 },
            R::SetOptOutOfLoot { on: true },
            R::GrantLevel { guid: 7 },
            R::ReportPvpAfk { guid: 8 },
        ]
        .map(|r| command_for(r).expect("sent"));
        assert!(matches!(
            sent[0],
            ClientCommand::NativeSessionMisc(
                benilla_protocol::wrath_session_misc::Command::SetPvp { on: 1 }
            )
        ));
        assert!(matches!(
            sent[2],
            ClientCommand::NativeInstanceControl(
                benilla_protocol::wrath_instance_control::Command::SetRaidDifficulty { mode: 3 }
            )
        ));
        assert!(matches!(
            sent[4],
            ClientCommand::NativeReferFriend(
                benilla_protocol::wrath_refer_friend::Command::GrantLevel { target: 7 }
            )
        ));
        let (tx, _rx) = crossbeam_channel::unbounded();
        let net = NetCommands(tx);
        for command in &sent {
            assert!(net.supports(command), "{command:?}");
        }
        assert!(command_for(R::ChannelSilenceVoice {
            channel: "x".into(),
            name: "y".into(),
            silence: true,
            group: false,
        })
        .is_none());
    }
}
