//! The core layer of wrathplus-app: see docs/APP-CRATE-SPLIT-PLAN.md.
#![cfg_attr(not(feature = "dev"), allow(dead_code, unused_imports))]

pub mod run_mode;
pub mod local_state;
pub mod query_cache;
pub mod area;
pub mod chr_classes;
pub mod text_filter;
pub mod text_reshape;
pub mod glue_strings;
pub mod crash;
pub mod shutdown;
pub mod pending_item_ops;
pub mod quest_reputation;
pub mod cvars;
pub mod launch_plan;
pub mod app_core;
pub mod wrath_vehicle_seats;
