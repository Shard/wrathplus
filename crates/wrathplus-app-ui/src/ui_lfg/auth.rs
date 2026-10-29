//! Transport-only authenticated expansion capture; App owns catalog-context presentation.
use crate::net::NetHandlerApp;
use bevy::prelude::*;
use wrathplus_protocol::{SessionEvent, SessionEventKind};

#[derive(Resource, Default)]
pub(crate) struct NativeAccountExpansion(pub(crate) Option<u8>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeAccountExpansion>()
        .net_handler(SessionEventKind::WrathAccountExpansion, receive)
        .net_handler(SessionEventKind::Disconnected, clear);
}
fn receive(In(event): In<SessionEvent>, mut value: ResMut<NativeAccountExpansion>) {
    if let SessionEvent::WrathAccountExpansion { expansion } = event {
        value.0 = expansion;
    }
}
fn clear(In(_): In<SessionEvent>, mut value: ResMut<NativeAccountExpansion>) {
    value.0 = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_server_expansion_is_optional_and_disconnect_scoped() {
        let mut app = App::new();
        register(&mut app);
        assert_eq!(app.world().resource::<NativeAccountExpansion>().0, None);
        for expansion in [Some(0), Some(255), Some(2), None] {
            crate::net::handlers::dispatch(
                app.world_mut(),
                vec![SessionEvent::WrathAccountExpansion { expansion }],
            );
            assert_eq!(
                app.world().resource::<NativeAccountExpansion>().0,
                expansion
            );
        }
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::WrathAccountExpansion { expansion: Some(0) }],
        );
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::Disconnected {
                reason: "test".into(),
                end: wrathplus_protocol::SessionEnd::Lost,
            }],
        );
        assert_eq!(app.world().resource::<NativeAccountExpansion>().0, None);
    }

    #[test]
    fn replacement_session_cannot_keep_previous_expansion() {
        let mut app = App::new();
        register(&mut app);
        let dispatch = |app: &mut App, event| {
            crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        };
        dispatch(
            &mut app,
            SessionEvent::WrathAccountExpansion { expansion: Some(2) },
        );
        dispatch(
            &mut app,
            SessionEvent::Disconnected {
                reason: "replace session".into(),
                end: wrathplus_protocol::SessionEnd::Lost,
            },
        );
        assert_eq!(app.world().resource::<NativeAccountExpansion>().0, None);
        dispatch(
            &mut app,
            SessionEvent::WrathAccountExpansion { expansion: Some(0) },
        );
        assert_eq!(app.world().resource::<NativeAccountExpansion>().0, Some(0));
        dispatch(
            &mut app,
            SessionEvent::WrathAccountExpansion { expansion: None },
        );
        assert_eq!(app.world().resource::<NativeAccountExpansion>().0, None);
    }
}
