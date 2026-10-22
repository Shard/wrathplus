//! The character-screen service order (`CharRequest::Service`): a reply the server applied
//! re-emits the roster before the result, as create and delete do.
use super::{Result, SessionEvent};
use benilla_protocol::wrath_char_services::Reply;

/// A failed refresh publishes nothing; the reply never overtakes its roster.
pub(super) fn publish_char_service(
    reply: Reply,
    refresh: impl FnOnce() -> Result<SessionEvent>,
    mut publish: impl FnMut(SessionEvent) -> Result<()>,
) -> Result<()> {
    if reply.changed_roster() {
        publish(refresh()?)?;
    }
    publish(SessionEvent::WrathCharServiceResult { reply })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    fn run(reply: Reply, refresh: Result<()>) -> (Result<()>, Vec<SessionEvent>) {
        let mut events = Vec::new();
        let roster = || {
            refresh.map(|()| SessionEvent::CharacterList {
                characters: Vec::new(),
                realm: None,
            })
        };
        let result = publish_char_service(reply, roster, |event| {
            events.push(event);
            Ok(())
        });
        (result, events)
    }

    #[test]
    fn an_applied_service_publishes_the_roster_before_its_result() {
        let applied = Reply::Rename {
            result: 0,
            renamed: Some((9, "Bob".into())),
        };
        let (result, events) = run(applied.clone(), Ok(()));
        result.unwrap();
        assert!(matches!(events[0], SessionEvent::CharacterList { .. }));
        assert!(
            matches!(&events[1], SessionEvent::WrathCharServiceResult { reply } if *reply == applied)
        );
        assert_eq!(events.len(), 2);
        for kept in [
            Reply::Rename {
                result: 0x31,
                renamed: None,
            },
            Reply::DeclinedNames { result: 0, guid: 9 },
        ] {
            let (result, events) = run(kept.clone(), Err(anyhow!("unused")));
            result.unwrap();
            assert!(
                matches!(&events[..], [SessionEvent::WrathCharServiceResult { reply }] if *reply == kept)
            );
        }
        let (result, events) = run(applied, Err(anyhow!("roster transport")));
        assert!(result.is_err());
        assert!(events.is_empty(), "no result without its roster");
    }
}
