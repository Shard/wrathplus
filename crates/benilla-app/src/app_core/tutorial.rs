//! What world code fires tutorials through: the sites' ids, the controller's input hooks and the
//! enter-world cascade they mark. The tutorial feed is `crate::tutorial`.

use std::time::Instant;

use bevy::prelude::*;

use crate::app_core::state::TutorialEvent;

/// The internal (0-based) ids of the sites this app produces, named by their published title.
pub(crate) mod id {
    pub(crate) const QUESTGIVERS: u32 = 0x00;
    pub(crate) const MOVEMENT: u32 = 0x01;
    pub(crate) const CAMERAS: u32 = 0x02;
    pub(crate) const TARGETING: u32 = 0x03;
    pub(crate) const LOOTING: u32 = 0x06;
    pub(crate) const BACKPACK: u32 = 0x07;
    pub(crate) const TRAINERS: u32 = 0x0d;
    pub(crate) const GROUPING: u32 = 0x11;
    pub(crate) const VENDORS: u32 = 0x13;
    pub(crate) const QUEST_LOG: u32 = 0x14;
    pub(crate) const FRIENDS: u32 = 0x15;
    pub(crate) const CHATTING: u32 = 0x16;
    pub(crate) const EQUIPPABLE_ITEMS: u32 = 0x17;
    pub(crate) const DEATH: u32 = 0x18;
    pub(crate) const RESTED: u32 = 0x19;
    pub(crate) const FATIGUE: u32 = 0x1a;
    pub(crate) const SWIMMING: u32 = 0x1b;
    pub(crate) const BREATH: u32 = 0x1c;
    pub(crate) const RESTING: u32 = 0x1d;
    pub(crate) const HEARTHSTONES: u32 = 0x1e;
    pub(crate) const PVP_COMBAT: u32 = 0x1f;
    pub(crate) const TRAVEL: u32 = 0x22;
    pub(crate) const WELCOME: u32 = 0x29;
    pub(crate) const RANGED_WEAPONS: u32 = 0x2b;
    pub(crate) const RAID_GROUPS: u32 = 0x2d;
    pub(crate) const MEETING_STONES: u32 = 0x2e;
    pub(crate) const BATTLEGROUND_QUEUE: u32 = 0x2f;
    pub(crate) const PORT_TO_BATTLEGROUND: u32 = 0x30;
    pub(crate) const KEYRINGS: u32 = 0x31;
}

/// The Targeting popup's delay at its one site (`0x514a8d mov edx,0x2710`).
pub(crate) const TARGETING_DELAY_MS: u32 = 10_000;

/// The enter-world cascade `0x4908c0`'s two unconditional triggers, Welcome (`0x490a48`) then
/// Questgivers (`0x490a51`), once per world session when the local player exists; and the
/// Movement popup's silence window.
#[derive(Resource, Default)]
pub(crate) struct WorldEnterCascade {
    pub(crate) armed: bool,
    pub(crate) entered_at: Option<Instant>,
    /// A movement input happened this session (the Movement auto-acknowledge site).
    pub(crate) moved: bool,
    /// The descriptor watcher's memory is owed a reset (a new world session).
    pub(crate) reset_watch: bool,
}

/// The world-input sites (`0x514840`), bundled for the controller: a movement input
/// acknowledges Movement and arms the 10 s Targeting popup; a mouse-look acknowledges Cameras.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct InputHooks<'w> {
    /// Optional, so a controller test's partial app runs without the tutorial plugin.
    asks: Option<MessageWriter<'w, TutorialEvent>>,
    cascade: Option<ResMut<'w, WorldEnterCascade>>,
}

impl InputHooks<'_> {
    /// A movement input this frame (`test ecx,0x1030`, taken as the forward and side axes).
    pub(crate) fn moved(&mut self) {
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
    pub(crate) fn mouselooked(&mut self) {
        if let Some(asks) = self.asks.as_mut() {
            asks.write(TutorialEvent::Acknowledge { id: id::CAMERAS });
        }
    }
}
