//! Game object messages: `CMSG_GAMEOBJ_USE`, the one verb for every usable object, which the
//! server answers through loot, gossip or state; the template query; and one-shot animations.

/// A game object template from `SMSG_GAMEOBJECT_QUERY_RESPONSE`; `data` is the raw
/// `GameObjectData` union, whose slots mean different things per `type_id`.
#[derive(Debug, Clone, PartialEq)]
pub struct GameObjectQueryInfo {
    pub type_id: u32,
    pub display_id: u32,
    pub name: String,
    pub data: [i32; 24],
}
