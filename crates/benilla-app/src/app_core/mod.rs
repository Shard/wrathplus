//! What every layer of the app reads: the schedule sets systems order themselves on, the shared
//! game state and the UI error queues. The first piece of the crate split
//! (`docs/APP-CRATE-SPLIT-PLAN.md`, step 0); the owning modules re-export from their old paths.

pub(crate) mod errors;
pub(crate) mod group;
pub(crate) mod loot;
pub(crate) mod schedule;
pub(crate) mod state;
