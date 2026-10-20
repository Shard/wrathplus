//! Client packet body builders: plain body bytes, framed and encrypted elsewhere.

use super::{CharCreateReq, MovementInfo};

/// `CMSG_CHAR_CREATE` body: the name cstring then nine bytes (`Packets/Character.cpp:4-19`); the
/// last, `outfit_id`, is 0 because the server ignores it (`CharacterHandler.cpp:310`).
pub fn char_create(req: &CharCreateReq) -> Vec<u8> {
    let mut body = Vec::with_capacity(req.name.len() + 10);
    body.extend_from_slice(req.name.as_bytes());
    body.push(0);
    body.extend_from_slice(&[
        req.race,
        req.class,
        req.gender,
        req.skin,
        req.face,
        req.hair_style,
        req.hair_color,
        req.facial_hair,
        0,
    ]);
    body
}

/// Back-to-back cstrings, the shape of every `CMSG_CHANNEL_*` body (`Packets/Channel.cpp`).
fn cstrings_body(strings: &[&str]) -> Vec<u8> {
    let mut body = Vec::with_capacity(strings.iter().map(|s| s.len() + 1).sum());
    for s in strings {
        body.extend_from_slice(s.as_bytes());
        body.push(0);
    }
    body
}

/// `CMSG_CHANNEL_LIST` body, the member-roster request: the channel name (`Channel.cpp:14-17`).
pub fn channel_list(name: &str) -> Vec<u8> {
    cstrings_body(&[name])
}

/// `CMSG_PLAYED_TIME` body, the `/played` request: empty (`Handlers/MiscHandler.cpp:935`).
pub fn played_time() -> Vec<u8> {
    Vec::new()
}

/// `CMSG_TEXT_EMOTE` body: `u32` EmotesText.dbc id, `u32` text variation (only relayed), `u64`
/// target or 0 (`Misc.cpp:60-65`). The server echoes `SMSG_TEXT_EMOTE` to us as well.
pub fn text_emote(text_id: u32, target: u64) -> Vec<u8> {
    let mut b = Vec::with_capacity(16);
    b.extend_from_slice(&text_id.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes()); // emoteNum: text-variation 0
    b.extend_from_slice(&target.to_le_bytes());
    b
}

/// `CMSG_CREATURE_QUERY` body: the template entry, then a full guid (vmangos `QueryCreature`).
pub fn creature_query(entry: u32, guid: u64) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&entry.to_le_bytes());
    body.extend_from_slice(&guid.to_le_bytes());
    body
}

/// Body of a `MSG_MOVE_*` message: a `MovementInfo`.
pub fn movement(info: &MovementInfo) -> Vec<u8> {
    let mut body = Vec::with_capacity(28);
    info.write(&mut body);
    body
}

/// `CMSG_MOVE_SPLINE_DONE` body: the `MovementInfo` at the ride's end, the acked spline id, and a
/// float the server skips but requires. The 1.12 client writes its completion fraction there
/// (`0x600b10`); we send only at completion, so it is always 1.0.
pub fn move_spline_done(info: &MovementInfo, spline_id: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(36);
    info.write(&mut body);
    body.extend_from_slice(&spline_id.to_le_bytes());
    body.extend_from_slice(&1.0f32.to_le_bytes()); // completion fraction (server-skipped)
    body
}

/// `CMSG_PING` body, the ~30 s keepalive (`0x537e10`): the sequence, then the last round trip in
/// ms, which the server stores as our latency; it echoes the sequence in `SMSG_PONG`.
pub fn ping(sequence: u32, last_rtt_ms: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&sequence.to_le_bytes());
    body.extend_from_slice(&last_rtt_ms.to_le_bytes());
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::Vector3d;

    /// vmangos `MoveSplineDone::ReadFromWorldPacket` reads info, spline id, then a skipped float.
    #[test]
    fn move_spline_done_body_golden() {
        let info = MovementInfo {
            flags: 0, // no SWIMMING / JUMPING ⇒ no conditional MovementInfo tails
            timestamp: 0x1122_3344,
            position: Vector3d {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            orientation: 0.5,
            transport: None,
            pitch: 0.0,
            fall_time: 0x5566_7788,
            jump: None,
        };
        let body = move_spline_done(&info, 0xAABB_CCDD);
        #[rustfmt::skip]
        let want: &[u8] = &[
            0x00, 0x00, 0x00, 0x00, // flags
            0x44, 0x33, 0x22, 0x11, // timestamp
            0x00, 0x00, 0x80, 0x3F, // pos.x = 1.0
            0x00, 0x00, 0x00, 0x40, // pos.y = 2.0
            0x00, 0x00, 0x40, 0x40, // pos.z = 3.0
            0x00, 0x00, 0x00, 0x3F, // orientation = 0.5
            0x88, 0x77, 0x66, 0x55, // fall_time
            0xDD, 0xCC, 0xBB, 0xAA, // splineId
            0x00, 0x00, 0x80, 0x3F, // completion fraction 1.0 (server-skipped)
        ];
        assert_eq!(body, want, "CMSG_MOVE_SPLINE_DONE body");
        let mi = movement(&info);
        assert_eq!(
            &body[..mi.len()],
            &mi[..],
            "MovementInfo comes first, verbatim"
        );
        assert_eq!(
            body.len(),
            mi.len() + 8,
            "…then splineId + the skipped float"
        );
    }
}
