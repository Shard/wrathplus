//! The cast bar's edges: our own cast's wire and local-cancel edges, which the spell's packet
//! handlers queue and the UI's cast-bar feed (`ui_cast::feed_cast_bar`) drains into the stock
//! `SPELLCAST_*` events the same frame.

use bevy::prelude::*;

/// One edge of our own cast, queued by the spell's packet handlers and its local cancel.
pub enum CastBarEdge {
    /// `SMSG_SPELL_START` with a nonzero cast time; an instant opens no bar, as in the reference.
    Start { spell_id: u32, cast_time_ms: u32 },
    /// `SMSG_SPELL_GO`: the cast completed.
    Stop,
    /// `SMSG_CAST_RESULT` failure: the bar, if shown, reads "Failed".
    Failed,
    /// `SMSG_SPELL_FAILED_OTHER`: our cast was interrupted.
    Interrupted,
    /// `SMSG_SPELL_DELAYED`: pushback extends the cast by `delay_ms`; the bar slides, never
    /// cancels.
    Delayed { delay_ms: u32 },
    /// `MSG_CHANNEL_START`, sent by vmangos to the caster only; gated by `ui_cast`'s channel arguments.
    ChannelStart { spell_id: u32, duration_ms: u32 },
    /// `MSG_CHANNEL_UPDATE`, also caster-only: the time left, 0 when the channel is over.
    ChannelUpdate { remaining_ms: u32 },
}

/// The queue of [`CastBarEdge`]s, drained by `ui_cast::feed_cast_bar`.
#[derive(Resource, Default)]
pub struct CastBarFeed(pub Vec<CastBarEdge>);
