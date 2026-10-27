use super::*;

/// A melee command whose value the codec refuses: the stock UI's sheath toggle with a state
/// build 12340 has no `SheathState` for.
fn unencodable_sheath() -> anyhow::Error {
    melee_packet(ClientCommand::SetSheathed { state: 9 }).unwrap_err()
}

#[test]
fn an_unencodable_melee_command_is_dropped_with_one_warning_naming_its_opcode() {
    let error = unencodable_sheath();
    let lines: Vec<String> = dropped_line(WrathRoute::NativeMelee, &error)
        .into_iter()
        .collect();
    assert_eq!(lines.len(), 1, "one warning, and the session stays up");
    let label = opcode_label(wrath_combat::CMSG_SET_SHEATHED);
    assert!(label.ends_with("CMSG_SET_SHEATHED"), "{label}");
    assert!(lines[0].contains(&label), "{}", lines[0]);
    assert!(lines[0].contains("NativeMelee"), "{}", lines[0]);
}

#[test]
fn an_unencodable_loot_command_names_its_opcode_and_is_dropped() {
    let error = loot_packet(ClientCommand::Loot { guid: 0 }).unwrap_err();
    let line = dropped_line(WrathRoute::NativeLoot, &error).expect("dropped, not fatal");
    assert!(
        line.contains(&opcode_label(wrath_loot::CMSG_LOOT)),
        "{line}"
    );
}

#[test]
fn a_socket_failure_still_ends_the_session_on_any_route() {
    let socket = Err::<(), _>(io::Error::from(io::ErrorKind::BrokenPipe))
        .context(crate::net::io::wrath_send_log::SendFailed)
        .unwrap_err();
    for path in [
        WrathRoute::NativeMelee,
        WrathRoute::NativeQuest,
        WrathRoute::GroundMovement,
    ] {
        assert_eq!(dropped_line(path, &socket), None, "{path:?}");
    }
}

#[test]
fn server_ordered_routes_stay_fail_closed_on_any_error() {
    let error = unencodable_sheath();
    for path in [
        WrathRoute::SetActiveMover,
        WrathRoute::NotActiveMover,
        WrathRoute::NativeVehicleControl,
        WrathRoute::GroundMovement,
        WrathRoute::NativeTeleportAck,
        WrathRoute::NativeWorldportAck,
        WrathRoute::TaxiSplineDone,
        WrathRoute::Logout,
        WrathRoute::LogoutCancel,
    ] {
        assert_eq!(dropped_line(path, &error), None, "{path:?}");
    }
}
