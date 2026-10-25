//! The spell-control route's bytes: the pet bar's orders and the other spell-control requests,
//! as the Wrath writer sends them ([`spell_control_packet`]).

use std::io;

use crate::net::ClientCommand;

/// The opcode and body the writer sends for a spell-control command: `CMSG_PET_ACTION` and the
/// other pet orders, the spell cancels, resurrect replies and far sight.
pub fn spell_control_packet(command: ClientCommand) -> io::Result<(u32, Vec<u8>)> {
    wrathplus_protocol::wrath_spell_control::encode(&spell_control_command(command))
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
