use super::*;

fn strings(e: &UiError) -> Vec<String> {
    e.args
        .iter()
        .map(|a| match a {
            FillArg::S(s) => s.clone(),
            FillArg::D(d) => d.to_string(),
        })
        .collect()
}

#[test]
fn a_success_takes_the_actions_line_and_an_error_its_code() {
    let created = command_line(action::CREATE, "Gladiators", "", 0).unwrap();
    assert_eq!(
        (created.key, strings(&created)),
        ("ERR_ARENA_TEAM_CREATE_S", vec!["Gladiators".into()])
    );
    let invited = command_line(action::INVITE, "Gladiators", "Arthas", 0).unwrap();
    assert_eq!(
        (invited.key, strings(&invited)),
        (
            "ERR_ARENA_TEAM_INVITE_SS",
            vec!["Arthas".into(), "Gladiators".into()]
        )
    );
    assert!(
        command_line(0x02, "x", "", 0).is_none(),
        "no line for an unlisted action"
    );
    // AC fills the name into either field; a `_S` code takes the player, else the team.
    let exists = command_line(action::CREATE, "Gladiators", "", 0x07).unwrap();
    assert_eq!(strings(&exists), ["Gladiators"]);
    let low = command_line(action::CREATE, "", "Arthas", 0x15).unwrap();
    assert_eq!(
        (low.key, strings(&low)),
        ("ERR_ARENA_TEAM_TARGET_TOO_LOW_S", vec!["Arthas".into()])
    );
    assert_eq!(
        command_line(action::QUIT, "", "", 0x08).unwrap().key,
        "ERR_ARENA_TEAM_LEADER_LEAVE_S"
    );
    assert_eq!(
        command_line(action::CREATE, "", "", 0x08).unwrap().key,
        "ERR_ARENA_TEAM_PERMISSIONS"
    );
    assert!(command_line(action::CREATE, "", "", 0x99).is_none());
}

#[test]
fn events_keep_their_strings_and_errors_name_the_bracket() {
    let changed = event_line(7, &["Old".into(), "New".into(), "Gladiators".into()]).unwrap();
    assert_eq!(
        (changed.key, strings(&changed)),
        (
            "ERR_ARENA_TEAM_LEADER_CHANGED_SSS",
            vec!["Old".into(), "New".into(), "Gladiators".into()]
        )
    );
    assert!(event_line(2, &[]).is_none());
    let no_team = error_line(0, Some(3)).unwrap();
    assert_eq!(
        (no_team.key, strings(&no_team)),
        ("ERR_ARENA_NO_TEAM_II", vec!["3".into(), "3".into()])
    );
    assert!(error_line(1, None).is_none());
}

/// Every key the tables name is a real 3.3.5 global string; skips without client data.
#[test]
fn the_arena_keys_resolve_in_the_real_global_strings() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
    let src = chain
        .read_file("Interface\\FrameXML\\GlobalStrings.lua")
        .expect("GlobalStrings.lua in the chain");
    let vm = wrathplus_ui::script::UiScript::new().expect("VM");
    vm.run(&String::from_utf8_lossy(&src)).expect("runs clean");
    let mut keys = Vec::new();
    for error in [
        0u32, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0x0a, 0x0b, 0x0c, 0x13, 0x15, 0x16, 0x17, 0x1b, 0x1e,
    ] {
        for act in [
            action::CREATE,
            action::INVITE,
            action::QUIT,
            action::FOUNDER,
        ] {
            keys.extend(command_line(act, "t", "p", error).map(|e| e.key));
        }
    }
    keys.extend((3..=8).filter_map(|e| event_line(e, &[]).map(|l| l.key)));
    keys.extend(error_line(0, Some(2)).map(|e| e.key));
    for key in keys {
        assert!(
            vm.lua().globals().get::<String>(key).is_ok(),
            "{key} missing"
        );
    }
}

#[test]
fn the_drain_takes_the_line_packets_and_leaves_the_unit_destroyed() {
    let mut app = App::new();
    app.init_resource::<crate::ui_battlefield::NativeBattlegroundRest>()
        .init_resource::<crate::app_core::errors::UiErrorKeys>()
        .init_resource::<crate::ui_arena_team::ArenaRosterRefresh>()
        .add_systems(Update, drain_arena_lines);
    app.world_mut()
        .resource_mut::<crate::ui_battlefield::NativeBattlegroundRest>()
        .0
        .extend([
            Packet::ArenaEvent {
                event: 3,
                strings: vec!["Arthas".into(), "Gladiators".into()],
                guid: Some(1),
            },
            Packet::ArenaUnitDestroyed { guid: 9 },
            Packet::ArenaError {
                raw_word: 0,
                team_type: Some(5),
            },
        ]);
    app.update();
    let w = app.world();
    let keys: Vec<_> = w
        .resource::<crate::app_core::errors::UiErrorKeys>()
        .0
        .iter()
        .map(|e| e.key)
        .collect();
    assert_eq!(keys, ["ERR_ARENA_TEAM_JOIN_SS", "ERR_ARENA_NO_TEAM_II"]);
    assert_eq!(
        w.resource::<crate::ui_arena_team::ArenaRosterRefresh>().0,
        ["Gladiators"]
    );
    let rest = &w
        .resource::<crate::ui_battlefield::NativeBattlegroundRest>()
        .0;
    assert_eq!(rest.len(), 1);
    assert!(matches!(rest[0], Packet::ArenaUnitDestroyed { guid: 9 }));
}
