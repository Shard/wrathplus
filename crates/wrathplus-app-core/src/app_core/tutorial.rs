//! What world code fires tutorials through: the sites' ids, the controller's input hooks and the
//! enter-world cascade they mark. The tutorial feed is `crate::tutorial`.

use std::time::Instant;

use bevy::prelude::*;

use crate::app_core::state::TutorialEvent;

/// The internal (0-based) ids of the sites this app produces, named by their published title.
pub mod id {
    pub const QUESTGIVERS: u32 = 0x00;
    pub const MOVEMENT: u32 = 0x01;
    pub(crate) const CAMERAS: u32 = 0x02;
    pub const TARGETING: u32 = 0x03;
    pub const LOOTING: u32 = 0x06;
    pub const BACKPACK: u32 = 0x07;
    pub const TRAINERS: u32 = 0x0d;
    pub const GROUPING: u32 = 0x11;
    pub const VENDORS: u32 = 0x13;
    pub const QUEST_LOG: u32 = 0x14;
    pub const FRIENDS: u32 = 0x15;
    pub const CHATTING: u32 = 0x16;
    pub const EQUIPPABLE_ITEMS: u32 = 0x17;
    pub const DEATH: u32 = 0x18;
    pub const RESTED: u32 = 0x19;
    pub const FATIGUE: u32 = 0x1a;
    pub const SWIMMING: u32 = 0x1b;
    pub const BREATH: u32 = 0x1c;
    pub const RESTING: u32 = 0x1d;
    pub const HEARTHSTONES: u32 = 0x1e;
    pub const PVP_COMBAT: u32 = 0x1f;
    pub const TRAVEL: u32 = 0x22;
    pub const WELCOME: u32 = 0x29;
    pub const RANGED_WEAPONS: u32 = 0x2b;
    pub const RAID_GROUPS: u32 = 0x2d;
    pub const BATTLEGROUND_QUEUE: u32 = 0x2f;
    pub const PORT_TO_BATTLEGROUND: u32 = 0x30;
    pub const KEYRINGS: u32 = 0x31;
}

/// The Targeting popup's delay at its one site (`0x514a8d mov edx,0x2710`).
pub const TARGETING_DELAY_MS: u32 = 10_000;

/// The enter-world cascade `0x4908c0`'s two unconditional triggers, Welcome (`0x490a48`) then
/// Questgivers (`0x490a51`), once per world session when the local player exists; and the
/// Movement popup's silence window.
#[derive(Resource, Default)]
pub struct WorldEnterCascade {
    pub armed: bool,
    pub entered_at: Option<Instant>,
    /// A movement input happened this session (the Movement auto-acknowledge site).
    pub moved: bool,
    /// The descriptor watcher's memory is owed a reset (a new world session).
    pub reset_watch: bool,
}

/// The world-input sites (`0x514840`), bundled for the controller: a movement input
/// acknowledges Movement and arms the 10 s Targeting popup; a mouse-look acknowledges Cameras.
#[derive(bevy::ecs::system::SystemParam)]
pub struct InputHooks<'w> {
    /// Optional, so a controller test's partial app runs without the tutorial plugin.
    asks: Option<MessageWriter<'w, TutorialEvent>>,
    cascade: Option<ResMut<'w, WorldEnterCascade>>,
}

impl InputHooks<'_> {
    /// A movement input this frame (`test ecx,0x1030`, taken as the forward and side axes).
    pub fn moved(&mut self) {
        if let Some(c) = self.cascade.as_mut() {
            c.moved = true;
        }
        if let Some(asks) = self.asks.as_mut() {
            asks.write(TutorialEvent::Acknowledge { id: id::MOVEMENT });
            asks.write(TutorialEvent::Trigger {
                id: id::TARGETING,
                delay_ms: TARGETING_DELAY_MS,
            });
        }
    }

    /// A mouse-look this frame (`[ebp+8] & 3`, the two buttons).
    pub fn mouselooked(&mut self) {
        if let Some(asks) = self.asks.as_mut() {
            asks.write(TutorialEvent::Acknowledge { id: id::CAMERAS });
        }
    }
}
