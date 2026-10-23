//! Taxi completion ACK, AzerothCore TaxiHandler::HandleMoveSplineDoneOpcode:
//! packed mover GUID, Wrath MovementInfo, then spline counter.
use crate::wrath_gameplay::{CodecError, MovementInfo, OutboundMovement, Position};

pub const CMSG_MOVE_SPLINE_DONE: u32 = 0x2c9;

pub fn encode(
    guid: u64,
    position: [f32; 3],
    orientation: f32,
    timestamp: u32,
    spline_id: u32,
) -> Result<Vec<u8>, CodecError> {
    if !position.iter().all(|v| v.is_finite()) || !orientation.is_finite() {
        return Err(CodecError::InvalidFlags("nonfinite taxi completion pose"));
    }
    let mut body = OutboundMovement {
        mover_guid: guid,
        movement: MovementInfo {
            flags: 0,
            flags2: 0,
            timestamp,
            position: Position {
                x: position[0],
                y: position[1],
                z: position[2],
                orientation,
            },
            transport: None,
            pitch: None,
            fall_time: 0,
            jump: None,
            spline_elevation: None,
        },
    }
    .to_bytes()?;
    body.extend(spline_id.to_le_bytes());
    Ok(body)
}

#[cfg(test)]
#[path = "wrath_taxi_ack/tests.rs"]
mod tests;
