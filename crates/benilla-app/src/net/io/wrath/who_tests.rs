use super::*;
use benilla_protocol::wrath_who::{Command, Query};
#[test]
fn native_commands_admitted_but_shared_who_stays_disabled() {
    let (tx, _) = crossbeam_channel::unbounded();
    let commands = crate::net::NetCommands(tx);
    for native in [
        Command::Whois {
            name: b"Synthetic".to_vec(),
        },
        Command::Who(Query {
            level_min: 0,
            level_max: 80,
            player_name: vec![],
            guild_name: vec![],
            race_mask: u32::MAX,
            class_mask: u32::MAX,
            zones: vec![],
            search_terms: vec![],
        }),
    ] {
        let command = ClientCommand::NativeWhoCommand(native.clone());
        assert!(matches!(route(&command), WrathRoute::NativeWho));
        assert!(commands.supports(SessionDialect::Wrath12340, &command));
        assert!(native.encode().is_ok());
        if matches!(native, Command::Whois { .. }) {
            assert!(!format!("{command:?}").contains("Synthetic"));
        }
    }
    let shared = ClientCommand::Who {
        request: Box::new(benilla_protocol::messages::WhoRequest::default()),
    };
    assert!(matches!(route(&shared), WrathRoute::DisabledGameAction));
    assert!(!commands.supports(SessionDialect::Wrath12340, &shared));
}
