//! What every layer of the app reads: the schedule sets systems order themselves on, the shared
//! game state and UI error types, the movement model the wire and the world share, and the death
//! state the wire fills. The first piece of the crate split (`docs/APP-CRATE-SPLIT-PLAN.md`,
//! step 0); the owning modules re-export from their old paths.

pub(crate) mod anim;
pub(crate) mod catalogs;
pub(crate) mod chat;
pub(crate) mod cooldowns;
pub(crate) mod death;
pub(crate) mod errors;
pub(crate) mod group;
pub(crate) mod interaction;
pub(crate) mod loot;
pub(crate) mod movement;
pub(crate) mod player;
pub(crate) mod schedule;
pub(crate) mod session;
pub(crate) mod state;
pub(crate) mod ui_flags;
pub(crate) mod vm_memo;
