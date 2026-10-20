//! The area-trigger pair: the client's report of entering an `AreaTrigger.dbc` volume, and the
//! server's refusal text. The server decides what a trigger does; a teleport arrives as
//! `SMSG_TRANSFER_PENDING` and `SMSG_NEW_WORLD`, or a same-map `MSG_MOVE_TELEPORT_ACK`.

/// `CMSG_AREATRIGGER` (180, as `0x5e2110` sends it): the `u32` `AreaTrigger.dbc` id. The server
/// obeys only a real row with the player inside the volume (5 yd slop), never while taxi-flying
/// (`Handlers/MiscHandler.cpp:622`).
pub fn area_trigger(trigger_id: u32) -> Vec<u8> {
    trigger_id.to_le_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_is_the_bare_id() {
        assert_eq!(area_trigger(542), 542u32.to_le_bytes().to_vec());
    }
}
