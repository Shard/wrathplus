//! Build 12340's paid character services through the create screen (`CharacterCreate.lua`'s
//! `PAID_SERVICE_TYPE`, `/tmp/azb-app-mig/ghidra/paid-*.log`):
//!
//! - The select screen offers one service per row by the roster's customize flags
//!   (`UpdateCharacterList`: faction over race over customize, `GetCharacterInfo` `0x4e3170`) and
//!   enters the create screen with the character's look (`CustomizeExistingCharacter` `0x4e2330`).
//! - `CharacterChangeFixup` keeps the class and narrows the races: a faction change offers the
//!   character's race or the other faction's that may be its class, a race change its own
//!   faction's, a customize the race alone.
//! - Accept asks `CONFIRM_PAID_SERVICE`; Done runs `CreateCharacter`'s paid branch (`0x4e0380`): the
//!   current name passes unchecked, another is name-checked (a refusal opens the dialog with its
//!   `CHAR_NAME_*` key), then the character's own flags pick the request, race change
//!   (`0x100000`, `0x4d9040`, CMSG `0x4F8`) before faction change (`0x10000`, `0x4d8f20`, `0x4D9`)
//!   before customize (`0x4d8e10`, `0x473`), each under its `*_IN_PROGRESS` status with Cancel.
//!
//! Labelled: the name check is the 2-12 character rule; `0x7e18c0`'s locale rules are left to the
//! server's reply.

use benilla_protocol::wrath_char_services::{Appearance, Command};
use benilla_protocol::Character;
use bevy::prelude::*;

/// `CHAR_CUSTOMIZE_FLAG_*`.
const FLAG_CUSTOMIZE: u32 = 0x1;
const FLAG_FACTION: u32 = 0x1_0000;
const FLAG_RACE: u32 = 0x10_0000;

/// The service a row offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaidService {
    Customize,
    RaceChange,
    FactionChange,
}

impl PaidService {
    /// `UpdateCharacterList`'s one button: faction over race over customize.
    pub(crate) fn offered(flags: u32) -> Option<Self> {
        if flags & FLAG_FACTION != 0 {
            Some(Self::FactionChange)
        } else if flags & FLAG_RACE != 0 {
            Some(Self::RaceChange)
        } else if flags & FLAG_CUSTOMIZE != 0 {
            Some(Self::Customize)
        } else {
            None
        }
    }

    /// The button's quarter of `UI-PaidCharacterCustomization-Button`.
    pub(crate) fn tc(self) -> [f32; 4] {
        match self {
            Self::Customize => [0.0, 0.5, 0.0, 0.5],
            Self::RaceChange => [0.5, 1.0, 0.0, 0.5],
            Self::FactionChange => [0.0, 0.5, 0.5, 1.0],
        }
    }
}

/// The character a paid service is reshaping, from the roster row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaidTarget {
    pub(crate) guid: u64,
    pub(crate) name: String,
    pub(crate) race: u8,
    pub(crate) class: u8,
    pub(crate) sex: u8,
    /// Skin, face, hair style, hair colour, facial hair: the create screen's dial order.
    pub(crate) dials: [u8; 5],
    pub(crate) flags: u32,
    pub(crate) service: PaidService,
}

/// The create screen's paid mode; `None` is a plain create.
#[derive(Resource, Default)]
pub(crate) struct PaidChange(pub(crate) Option<PaidTarget>);

impl PaidTarget {
    pub(crate) fn of(c: &Character, service: PaidService) -> Self {
        Self {
            guid: c.guid,
            name: c.name.clone(),
            race: c.race,
            class: c.class,
            sex: c.gender,
            dials: [c.skin, c.face, c.hair_style, c.hair_color, c.facial_hair],
            flags: c.customize_flags,
            service,
        }
    }

    /// `CharacterChangeFixup`'s race gate; `class_ok` is `IsRaceClassValid`.
    pub(crate) fn race_allowed(&self, race: u8, class_ok: impl Fn(u8, u8) -> bool) -> bool {
        let alliance = |r: u8| super::ALLIANCE.contains(&r);
        race == self.race
            || match self.service {
                PaidService::FactionChange => {
                    alliance(race) != alliance(self.race) && class_ok(race, self.class)
                }
                PaidService::RaceChange => {
                    alliance(race) == alliance(self.race) && class_ok(race, self.class)
                }
                PaidService::Customize => false,
            }
    }

    /// `CreateCharacter`'s paid branch: the request and its status key, or the `CHAR_NAME_*` key
    /// that refuses the typed name.
    pub(crate) fn request(
        &self,
        name: &str,
        race: u8,
        sex: u8,
        dials: [u8; 5],
    ) -> Result<(Command, &'static str), &'static str> {
        if name.to_lowercase() != self.name.to_lowercase() {
            match name.chars().count() {
                0 => return Err("CHAR_NAME_NO_NAME"),
                1 => return Err("CHAR_NAME_TOO_SHORT"),
                2..=12 => {}
                _ => return Err("CHAR_NAME_TOO_LONG"),
            }
        }
        let [skin, face, hair_style, hair_color, facial_hair] = dials;
        let appearance = Appearance {
            gender: sex,
            skin,
            hair_color,
            hair_style,
            facial_hair,
            face,
        };
        let (guid, name) = (self.guid, name.to_string());
        Ok(if self.flags & FLAG_RACE != 0 {
            (
                Command::ChangeRace {
                    faction: false,
                    guid,
                    name,
                    appearance,
                    race,
                },
                "RACE_CHANGE_IN_PROGRESS",
            )
        } else if self.flags & FLAG_FACTION != 0 {
            (
                Command::ChangeRace {
                    faction: true,
                    guid,
                    name,
                    appearance,
                    race,
                },
                "FACTION_CHANGE_IN_PROGRESS",
            )
        } else {
            (
                Command::Customize {
                    guid,
                    name,
                    appearance,
                },
                "CHAR_CUSTOMIZE_IN_PROGRESS",
            )
        })
    }
}

#[cfg(test)]
#[path = "paid_tests.rs"]
mod tests;
