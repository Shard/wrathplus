//! The one drain of [`NativeCharServiceReplies`]: every frame, whatever screen is up, each reply
//! becomes a [`CharServiceReply`] tagged with the screen it arrived on, and the screens answer
//! from that message (`delete_result` on select, the paid flow on create, the stock glue host
//! later). Nothing waits in the capture for a screen that is not up.

use bevy::prelude::*;

use wrathplus_protocol::wrath_char_services::Reply;

use super::ClientState;
use crate::net::wrath_char_service_capture::NativeCharServiceReplies;

/// One character-service reply, with the screen that was up when it arrived; a reader answers
/// only its own screen's, so one read on a screen switch's far side cannot answer twice.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub(crate) struct CharServiceReply {
    pub(crate) reply: Reply,
    pub(crate) screen: Option<ClientState>,
}

/// Register the message and the drain, ahead of both screens' chains and after the net drain.
pub(crate) fn register(app: &mut App) {
    app.add_message::<CharServiceReply>().add_systems(
        Update,
        publish_service_replies
            .after(wrathplus_world::schedule::WorldStage::Net)
            .before(crate::glue::GlueScreens::Select)
            .before(crate::glue::GlueScreens::Create),
    );
}

/// Move the captured replies, in order, into [`CharServiceReply`] messages.
pub(crate) fn publish_service_replies(
    replies: Option<ResMut<NativeCharServiceReplies>>,
    state: Option<Res<State<ClientState>>>,
    mut out: MessageWriter<CharServiceReply>,
) {
    let Some(mut replies) = replies else { return };
    let screen = state.map(|s| *s.get());
    for reply in replies.0.drain(..) {
        out.write(CharServiceReply { reply, screen });
    }
}

/// The replies one screen answers, read through its own cursor.
pub(crate) fn replies_for(
    reader: Option<MessageReader<CharServiceReply>>,
    screen: ClientState,
) -> Vec<Reply> {
    reader
        .into_iter()
        .flat_map(|mut r| {
            r.read()
                .filter(|m| m.screen == Some(screen))
                .map(|m| m.reply.clone())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each reply goes out once, tagged with the screen up at the drain, and each screen's reader
    /// takes only its own, even when it first runs a frame after the switch.
    #[test]
    fn one_drain_tags_each_reply_with_its_screen() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(ClientState::CharCreate)
            .init_resource::<NativeCharServiceReplies>()
            .add_message::<CharServiceReply>()
            .add_systems(Update, publish_service_replies);
        let refused = Reply::Rename {
            result: 0x31,
            renamed: None,
        };
        app.world_mut()
            .resource_mut::<NativeCharServiceReplies>()
            .0
            .push_back(refused.clone());
        app.update();
        assert!(app
            .world()
            .resource::<NativeCharServiceReplies>()
            .0
            .is_empty());
        use bevy::ecs::system::RunSystemOnce;
        let seen = |app: &mut App, screen| {
            app.world_mut()
                .run_system_once(move |r: Option<MessageReader<CharServiceReply>>| {
                    replies_for(r, screen)
                })
                .unwrap()
        };
        assert_eq!(seen(&mut app, ClientState::CharCreate), [refused]);
        assert!(
            seen(&mut app, ClientState::CharSelect).is_empty(),
            "not select's"
        );
    }

    /// A refused native rename opens the dialog with its `CHAR_NAME_*` key; an applied one
    /// (result 0, the roster already refreshed) opens nothing.
    #[test]
    fn a_refused_character_service_raises_the_glue_dialog() {
        use crate::glue::dialog::GlueDialog;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<GlueDialog>()
            .init_resource::<NativeCharServiceReplies>()
            .add_plugins(bevy::state::app::StatesPlugin)
            .insert_state(ClientState::CharSelect)
            .add_message::<CharServiceReply>()
            .add_systems(
                Update,
                (publish_service_replies, super::super::delete_result).chain(),
            );
        app.world_mut()
            .resource_mut::<NativeCharServiceReplies>()
            .0
            .push_back(Reply::Rename {
                result: 0,
                renamed: Some((7, "Newname".into())),
            });
        app.update();
        assert!(!app.world().resource::<GlueDialog>().is_open());
        app.world_mut()
            .resource_mut::<NativeCharServiceReplies>()
            .0
            .push_back(Reply::Rename {
                result: 0x5e,
                renamed: None,
            });
        app.update();
        assert!(app.world().resource::<GlueDialog>().is_open());
        assert!(app
            .world()
            .resource::<NativeCharServiceReplies>()
            .0
            .is_empty());
    }

    /// An applied rename takes down its `CHAR_RENAME_IN_PROGRESS` status; the status's Cancel
    /// closes it too.
    #[test]
    fn an_applied_rename_or_cancel_takes_down_the_rename_status() {
        use crate::glue::dialog::{DialogKind, GlueDialog, GlueDialogAnswer};
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<GlueDialogAnswer>()
            .init_resource::<GlueDialog>()
            .init_resource::<NativeCharServiceReplies>()
            .add_plugins(bevy::state::app::StatesPlugin)
            .insert_state(ClientState::CharSelect)
            .add_message::<CharServiceReply>()
            .add_systems(
                Update,
                (publish_service_replies, super::super::delete_result).chain(),
            );
        app.world_mut()
            .resource_mut::<GlueDialog>()
            .open_status("Renaming Character...");
        app.world_mut()
            .resource_mut::<NativeCharServiceReplies>()
            .0
            .push_back(Reply::Rename {
                result: 0,
                renamed: Some((7, "Newname".into())),
            });
        app.update();
        assert!(!app.world().resource::<GlueDialog>().is_open());
        app.world_mut()
            .resource_mut::<GlueDialog>()
            .open_status("Renaming Character...");
        app.world_mut().write_message(GlueDialogAnswer {
            kind: DialogKind::Status,
            button1: true,
            button2: false,
        });
        app.update();
        assert!(!app.world().resource::<GlueDialog>().is_open());
    }
}
