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

/// `CMSG_PING` body, the ~30 s keepalive (`0x537e10`): the sequence, then the last round trip in
/// ms, which the server stores as our latency; it echoes the sequence in `SMSG_PONG`.
pub fn ping(sequence: u32, last_rtt_ms: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&sequence.to_le_bytes());
    body.extend_from_slice(&last_rtt_ms.to_le_bytes());
    body
}
