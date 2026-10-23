use super::*;
use wrathplus_protocol::CharAction::{Create, Delete};

#[test]
fn only_native_success_refreshes_before_raw_result() {
    for action in [Create, Delete] {
        for code in 0..=255 {
            let mut events = Vec::new();
            publish_character_action(
                action,
                code,
                || {
                    Ok(SessionEvent::CharacterList {
                        characters: Vec::new(),
                        realm: None,
                    })
                },
                |event| {
                    events.push(event);
                    Ok(())
                },
            )
            .unwrap();
            let success = match action {
                Create => code == 0x2f,
                Delete => code == 0x47,
            };
            assert_eq!(events.len(), if success { 2 } else { 1 });
            if success {
                assert!(matches!(events[0], SessionEvent::CharacterList { .. }));
            }
            assert!(
                matches!(events.last().unwrap(), SessionEvent::WrathCharActionResult {
                action: actual_action, code: actual_code,
            } if *actual_action == action && *actual_code == code)
            );
        }
    }
    assert!(!native_character_action_succeeded(Create, 0x2e));
    assert!(!native_character_action_succeeded(Delete, 0x39));
}

#[test]
fn refresh_or_publish_failure_cannot_publish_success_result() {
    let mut count = 0;
    assert!(publish_character_action(
        Create,
        0x2f,
        || Err(anyhow!("roster transport")),
        |_| {
            count += 1;
            Ok(())
        }
    )
    .is_err());
    assert_eq!(count, 0);
    assert!(publish_character_action(
        Delete,
        0x47,
        || Ok(SessionEvent::CharacterList {
            characters: Vec::new(),
            realm: None
        }),
        |_| {
            count += 1;
            Err(anyhow!("App stopped"))
        }
    )
    .is_err());
    assert_eq!(count, 1);
}
