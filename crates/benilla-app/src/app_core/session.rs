//! The net session's markers and edges that lower layers name: our own entity and the body we are
//! attached to, the descriptor store every streamed object carries, the offline flag, and the
//! world-enter and disconnect messages. Moved here for the crate split
//! (`docs/APP-CRATE-SPLIT-PLAN.md`, step 0 residue); `crate::net` re-exports each from its old path
//! and keeps the systems that write them.

use benilla_protocol::ObjectFields;
use bevy::prelude::*;

/// This process has no IO thread (`net::NetPlugin::connect` false); `crate::preflight` announces
/// it.
/// Not `net::NetStatus`'s `connected`, which a dropped connection also clears.
#[derive(Resource)]
pub(crate) struct NetOffline;

/// A streamed object's descriptor fields, seeded by the create block and merged with each
/// `Values` delta. Speeds and pose are movement-block data, not here.
#[derive(Component, Clone, Default)]
pub(crate) struct ObjectStore(pub(crate) ObjectFields);

impl ObjectStore {
    /// The object manager's unit lookup (`0x468460`, `ecx = 8`): a unit or a player, which share
    /// the unit block; a game object, corpse or item is not one.
    pub(crate) fn is_unit(&self) -> bool {
        use benilla_protocol::types::ObjectType;
        matches!(
            self.0.object_type(),
            Some(ObjectType::Unit | ObjectType::Player)
        )
    }
}

/// Our own character's entity (guid == `net::SelfGuid`): identity, not what we steer
/// ([`Embodied`]).
#[derive(Component)]
pub(crate) struct SelfPlayer;

/// The body this client is attached to: the reference's camera anchor (`camera+0x88`), written
/// only by the constructor and `SetTarget 0x50d0f0`, never by a control update. Normally our own
/// body; under possession, the possessed unit.
///
/// At most one entity carries it, and a claimed mover not yet streamed is none: outbound
/// `MSG_MOVE_*` carry no guid, so a fallback to our body would write our pose onto the creature.
/// Whether it may move is `net::ActiveMover`; `player` owns the placement.
#[derive(Component)]
pub(crate) struct Embodied;

/// We entered the world.
#[derive(Message)]
pub(crate) struct EnteredWorldMessage {
    /// Rested billing minutes from `SMSG_AUTH_RESPONSE`, the only time they arrive.
    pub(crate) billing_time_rested: u32,
    /// The tutorial flags, if `SMSG_TUTORIAL_FLAGS` came during the login handshake.
    pub(crate) tutorial_flags: Option<Vec<u8>>,
}

/// The session ended; the IO thread is back at its pre-logon park.
#[derive(Message, Clone)]
pub(crate) struct DisconnectedMessage {
    pub(crate) reason: String,
    pub(crate) end: benilla_protocol::SessionEnd,
    /// The reference's `DISCONNECTED_FROM_SERVER`: back at the account screen, nothing retried.
    /// Decided once in [`Self::new`], so its four readers (teardown, screen, cover, credentials)
    /// cannot disagree.
    pub(crate) session_over: bool,
}

impl DisconnectedMessage {
    /// `session_over` is false after a logout (the relist is the select the player asked for) and
    /// in a run declared unattended ([`crate::run_mode::unattended`]), which reconnects. Not the
    /// login env vars: a person's launch line sets them too, and a reconnect would take the
    /// account back from whoever displaced us.
    pub(crate) fn new(reason: String, end: benilla_protocol::SessionEnd) -> Self {
        let session_over =
            end == benilla_protocol::SessionEnd::Lost && !crate::run_mode::unattended();
        Self {
            reason,
            end,
            session_over,
        }
    }
}
