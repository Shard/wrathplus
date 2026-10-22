//! The inbound handler table, the reference's shape: `NetClient` holds an opcode → handler table
//! (`+0x74`, 828 slots) and its dispatcher `0x537aa0` calls each packet's handler in packet order,
//! dropping an unregistered opcode in silence. Here each subsystem registers one-shot systems per
//! [`SessionEventKind`] through [`NetHandlerApp::net_handler`], and the exclusive drain runs them
//! in packet order, each seeing what the one before it did.

use std::collections::HashMap;

use wrathplus_protocol::{SessionEvent, SessionEventKind};
use bevy::ecs::system::SystemId;
use bevy::prelude::*;

/// One registered handler: the one-shot system and its name for the census.
type Handler = (SystemId<In<SessionEvent>>, &'static str);

/// The table, filled at plugin build and read by the drain.
#[derive(Resource, Default)]
pub(crate) struct NetHandlers {
    by_kind: HashMap<SessionEventKind, Vec<Handler>>,
}

impl NetHandlers {
    /// Every kind with a handler, and the handlers' names in registration order.
    #[cfg(test)]
    pub(crate) fn census(&self) -> std::collections::BTreeMap<SessionEventKind, Vec<&'static str>> {
        self.by_kind
            .iter()
            .map(|(k, v)| (*k, v.iter().map(|(_, n)| *n).collect()))
            .collect()
    }

    /// Runs every handler once with `ev`, whatever its kind, and names those that could not run:
    /// a wrong-kind event still fetches the system's parameters.
    #[cfg(test)]
    pub(crate) fn probe(&self, world: &mut World, ev: &SessionEvent) -> Vec<String> {
        let mut failed = Vec::new();
        for (kind, list) in &self.by_kind {
            for (id, name) in list {
                if let Err(e) = world.run_system_with(*id, ev.clone()) {
                    failed.push(format!("{kind:?}: `{name}` — {e}"));
                }
            }
        }
        failed.sort();
        failed
    }

    fn push(&mut self, kind: SessionEventKind, handler: Handler) {
        self.by_kind.entry(kind).or_default().push(handler);
    }

    /// Runs the event's handlers in registration order.
    fn run(&self, world: &mut World, ev: SessionEvent) {
        let Some(list) = self.by_kind.get(&SessionEventKind::from(&ev)) else {
            return;
        };
        let Some(((last_id, last_name), rest)) = list.split_last() else {
            return;
        };
        for (id, name) in rest {
            call(world, *id, name, ev.clone());
        }
        call(world, *last_id, last_name, ev);
    }
}

fn call(world: &mut World, id: SystemId<In<SessionEvent>>, name: &str, ev: SessionEvent) {
    if let Err(e) = world.run_system_with(id, ev) {
        // A plugin bug (most likely a missing resource), logged loud so the smoke gate sees it.
        error!("net: handler `{name}` did not run: {e}");
    }
}

/// `app.net_handler(SessionEventKind::X, on_x)` registers a system taking `In<SessionEvent>`;
/// several handlers on one kind run in registration order.
pub(crate) trait NetHandlerApp {
    fn net_handler<M>(
        &mut self,
        kind: SessionEventKind,
        handler: impl IntoSystem<In<SessionEvent>, (), M> + 'static,
    ) -> &mut Self;
}

impl NetHandlerApp for App {
    fn net_handler<M>(
        &mut self,
        kind: SessionEventKind,
        handler: impl IntoSystem<In<SessionEvent>, (), M> + 'static,
    ) -> &mut Self {
        let name = std::any::type_name_of_val(&handler);
        let id = self.world_mut().register_system(handler);
        self.world_mut()
            .get_resource_or_init::<NetHandlers>()
            .push(kind, (id, name));
        self
    }
}

/// One drain's dispatch in wire order, each handler's commands applied before the next event.
pub(crate) fn dispatch(world: &mut World, events: Vec<SessionEvent>) {
    let handlers = world.remove_resource::<NetHandlers>().unwrap_or_default();
    for ev in events {
        handlers.run(world, ev);
    }
    world.insert_resource(handlers);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// What ran, in order.
    #[derive(Resource, Default)]
    struct Log(Vec<String>);

    fn on_queued(In(ev): In<SessionEvent>, mut log: ResMut<Log>) {
        let SessionEvent::LoginQueued { position, .. } = ev else {
            panic!("the table routes by kind");
        };
        log.0.push(format!("queued:{}", position.unwrap_or(0)));
    }

    fn on_logged_out(In(ev): In<SessionEvent>, mut log: ResMut<Log>) {
        assert!(matches!(ev, SessionEvent::LoggedOut));
        log.0.push("logged_out".into());
    }

    fn on_logged_out_too(In(_): In<SessionEvent>, mut log: ResMut<Log>) {
        log.0.push("logged_out_too".into());
    }

    pub(crate) fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<Log>();
        app
    }

    fn queued(position: u32) -> SessionEvent {
        SessionEvent::LoginQueued {
            position: Some(position),
            realm: None,
        }
    }

    pub(crate) fn run(app: &mut App, events: Vec<SessionEvent>) -> Vec<String> {
        dispatch(app.world_mut(), events);
        std::mem::take(&mut app.world_mut().resource_mut::<Log>().0)
    }

    #[test]
    fn events_run_in_wire_order_and_an_unhandled_kind_is_skipped() {
        let mut app = app();
        app.net_handler(SessionEventKind::LoginQueued, on_queued)
            .net_handler(SessionEventKind::LoggedOut, on_logged_out);
        let stage = || SessionEvent::LoginStage {
            stage: wrathplus_protocol::LoginStage::Connecting,
        };
        let log = run(
            &mut app,
            vec![
                queued(1),
                stage(),
                SessionEvent::LoggedOut,
                queued(2),
                stage(),
            ],
        );
        assert_eq!(log, vec!["queued:1", "logged_out", "queued:2"]);
    }

    #[test]
    fn several_handlers_on_one_kind_run_in_registration_order_each_with_the_event() {
        let mut app = app();
        app.net_handler(SessionEventKind::LoggedOut, on_logged_out)
            .net_handler(SessionEventKind::LoggedOut, on_logged_out_too);
        let log = run(&mut app, vec![SessionEvent::LoggedOut]);
        assert_eq!(log, vec!["logged_out", "logged_out_too"]);
        assert_eq!(
            app.world().resource::<NetHandlers>().census()[&SessionEventKind::LoggedOut].len(),
            2
        );
    }

    #[derive(Resource)]
    struct Absent;

    fn needs_what_nobody_inserted(In(_): In<SessionEvent>, _absent: Res<Absent>) {}

    #[test]
    fn the_probe_names_a_handler_whose_resource_is_missing() {
        let mut app = app();
        app.net_handler(SessionEventKind::LoggedOut, on_logged_out)
            .net_handler(SessionEventKind::LoginQueued, needs_what_nobody_inserted);
        let handlers = app.world_mut().remove_resource::<NetHandlers>().unwrap();
        let failed = handlers.probe(app.world_mut(), &SessionEvent::LoggedOut);
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert!(
            failed[0].contains("needs_what_nobody_inserted"),
            "{failed:?}"
        );
    }
}
