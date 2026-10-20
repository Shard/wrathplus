//! What every layer of the app reads: the schedule sets systems order themselves on, the shared
//! state and UI error types, and the movement model the wire and the world share. The first piece
//! of the crate split (`docs/APP-CRATE-SPLIT-PLAN.md`, step 0); the owning modules re-export from
//! their old paths.

pub(crate) mod errors;
pub(crate) mod movement;
pub(crate) mod schedule;
pub(crate) mod state;
