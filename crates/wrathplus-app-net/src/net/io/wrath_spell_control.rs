//! The spell-control route's bytes: the pet bar's orders and the other spell-control requests,
//! as the Wrath writer sends them ([`spell_control_packet`]), and the info line each pet order
//! logs on its way out ([`log_pet_send`]).

use std::io;

use crate::net::ClientCommand;

/// The opcode and body the writer sends for a spell-control command: `CMSG_PET_ACTION` and the
/// other pet orders, the spell cancels, resurrect replies and far sight.
pub fn spell_control_packet(command: ClientCommand) -> io::Result<(u32, Vec<u8>)> {
    wrathplus_protocol::wrath_spell_control::encode(&spell_control_command(command))
}

/// Every pet order at info, with its bytes (`CMSG_PET_*` but the name query, which every pet in
/// view asks): a pet that ignores its orders shows in the log whether the client sent them, which
/// a capture (server to client only) cannot.
pub(super) fn log_pet_send(opcode: u32, body: &[u8]) {
    let name = u16::try_from(opcode)
        .ok()
        .and_then(wrathplus_protocol::wrath_opcode_names::opcode_name);
    if let Some(name) = name.filter(|n| n.starts_with("CMSG_PET_") && *n != "CMSG_PET_NAME_QUERY") {
        bevy::log::info!("net(wrath) SEND {name} 0x{opcode:04x} {body:02x?}");
    }
}

/// The native request for a spell-control command.
fn spell_control_command(
    command: ClientCommand,
) -> wrathplus_protocol::wrath_spell_control::Command {
    use wrathplus_protocol::wrath_spell_control::Command as C;
    match command {
        ClientCommand::NativeSpellControl(command) => command,
        ClientCommand::PetAction {
            pet_guid,
            packed,
            target_guid,
        } => C::PetAction {
            pet: pet_guid,
            word: packed,
            target: target_guid,
        },
        ClientCommand::PetSetAction { pet_guid, entries } => C::PetSetAction {
            pet: pet_guid,
            entries,
        },
        ClientCommand::PetStopAttack { pet_guid } => C::PetStopAttack { pet: pet_guid },
        ClientCommand::PetCancelAura { pet_guid, spell_id } => C::PetCancelAura {
            pet: pet_guid,
            spell_id,
        },
        ClientCommand::PetSpellAutocast {
            pet_guid,
            spell_id,
            enabled,
        } => C::PetSpellAutocast {
            pet: pet_guid,
            spell_id,
            enabled,
        },
        ClientCommand::PetAbandon { pet_guid } => C::PetAbandon { pet: pet_guid },
        ClientCommand::PetRename { pet_guid, name } => C::PetRename {
            pet: pet_guid,
            name,
        },
        ClientCommand::CancelAutoRepeat => C::CancelAutoRepeatSpell,
        ClientCommand::CancelChannelling { spell_id } => C::CancelChannelling { spell_id },
        ClientCommand::ResurrectResponse { caster, accept } => {
            C::ResurrectResponse { caster, accept }
        }
        ClientCommand::FarSight { engage } => C::FarSight { engage },
        _ => unreachable!("not a spell-control route"),
    }
}
