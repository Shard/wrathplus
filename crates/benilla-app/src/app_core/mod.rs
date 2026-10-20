//! What every layer of the app reads: the schedule sets systems order themselves on. The first
//! piece of the crate split (`docs/APP-CRATE-SPLIT-PLAN.md`, step 0); the owning modules re-export
//! from their old paths.

pub(crate) mod schedule;
