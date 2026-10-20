//! Game object messages: `CMSG_GAMEOBJ_USE`, the one verb for every usable object, which the
//! server answers through loot, gossip or state; the template query; and one-shot animations.

/// `CMSG_GAMEOBJ_USE` body: the object's full guid (`Handlers/SpellHandler.cpp`). A chest's loot
/// opens through this, never `CMSG_LOOT`, which rejects a game object guid.
pub fn gameobj_use(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

/// `CMSG_GAMEOBJECT_QUERY` body: the template entry, then a full guid (vmangos `QueryGameObject`).
pub fn gameobject_query(entry: u32, guid: u64) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&entry.to_le_bytes());
    body.extend_from_slice(&guid.to_le_bytes());
    body
}

/// A game object template from `SMSG_GAMEOBJECT_QUERY_RESPONSE`; `data` is the raw
/// `GameObjectData` union, whose slots mean different things per `type_id`.
#[derive(Debug, Clone, PartialEq)]
pub struct GameObjectQueryInfo {
    pub type_id: u32,
    pub display_id: u32,
    pub name: String,
    pub data: [i32; 24],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hx(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn cmsg_gameobj_use_body_golden() {
        // SpellHandler.cpp reads a single ObjectGuid.
        assert_eq!(
            gameobj_use(0x1234_5678_9abc_def0),
            hx("f0debc9a78563412"),
            "CMSG_GAMEOBJ_USE body"
        );
    }

    #[test]
    fn cmsg_gameobject_query_body_golden() {
        // As vmangos `QueryGameObject::ReadFromWorldPacket` reads it.
        assert_eq!(
            gameobject_query(1731, 0x1234_5678_9abc_def0),
            hx("c3060000f0debc9a78563412"),
            "CMSG_GAMEOBJECT_QUERY body"
        );
    }
}
