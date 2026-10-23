//! Sole lossless native chat capture owner: every `SMSG_MESSAGECHAT`/`SMSG_GM_MESSAGECHAT`, addon
//! traffic included, with raw bytes, sender GUID and chat type. It is the only input of the native
//! chat feed (`ui_chat::native_message`); the wire emits no shared `SessionEvent::Chat` beside it.
//! This FIFO does not infer permissions, emit UI events, or initiate queries.
use crate::net::NetHandlerApp;
use wrathplus_protocol::{wrath_chat::Message, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeChatPackets(pub(crate) VecDeque<Message>);

pub struct UiNativeChatPlugin;
impl Plugin for UiNativeChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NativeChatPackets>()
            .net_handler(SessionEventKind::WrathChat, receive)
            .net_handler(SessionEventKind::Disconnected, clear);
    }
}
fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeChatPackets>) {
    if let SessionEvent::WrathChat { message } = event {
        packets.0.push_back(message);
    }
}
fn clear(In(_): In<SessionEvent>, mut packets: ResMut<NativeChatPackets>) {
    packets.0.clear();
}

#[cfg(test)]
#[path = "ui_native_chat_tests.rs"]
mod tests;
