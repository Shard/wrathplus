//! Build 12340's native receipts that feed the FrameXML VM directly: each module captures or
//! reduces its family and publishes into `UiScript`, so it lives with the feed layer
//! (`tests/ui_script_wall.rs`). `crate::net` re-exports them under their old paths.

pub(crate) mod wrath_achievements;
pub(crate) mod wrath_appearance;
pub(crate) mod wrath_death_capture;
pub(crate) mod wrath_equipment_sets;
pub(crate) mod wrath_item_refunds;
pub(crate) mod wrath_quest_pois;
pub(crate) mod wrath_quests_completed_capture;
pub(crate) mod wrath_raid_browser_feed;
pub(crate) mod wrath_sockets;
pub(crate) mod wrath_state_feed;
pub(crate) mod wrath_threat;
pub(crate) mod wrath_ui_timer_capture;
