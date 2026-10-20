//! The WoW 1.12.1 (build 5875) wire protocol: [`auth`] for realmd (login protocol version 3),
//! [`world`] and [`events`] for the world server. SRP6 and the header crypto are `benilla-srp`'s.

pub mod events;
pub mod guid;
pub mod types;
pub mod wire;
pub mod wrath_auth;
pub mod wrath_bootstrap;
pub mod wrath_vehicle;
pub mod wrath_environment;
pub mod wrath_bind;
pub mod wrath_auras;
pub mod wrath_authority;
pub mod wrath_combat;
pub mod wrath_events;
pub mod wrath_gameplay;
pub mod wrath_ground;
pub mod wrath_items;
pub mod wrath_group;
pub mod wrath_guild_bank;
pub mod wrath_guild_bank_commands;
pub mod wrath_guild_bank_log;
pub mod wrath_guild_bank_query;
pub mod wrath_guild_bank_text;
pub mod wrath_guild_bank_swap;
pub mod wrath_guild_bank_tab;
pub mod wrath_guild_rank;
pub mod wrath_lfg;
pub mod wrath_instance;
pub mod wrath_instance_control;
pub mod wrath_inventory;
pub mod wrath_exchange;
pub mod wrath_achievement;
pub mod wrath_calendar;
pub mod wrath_quest;
pub mod wrath_raid_browser;
pub mod wrath_party;
pub mod wrath_petition;
pub mod wrath_channel_members;
pub mod wrath_chat_notices;
pub mod wrath_battleground;
pub mod wrath_contacts;
pub mod wrath_death;
pub mod wrath_session_meta;
pub mod wrath_help;
pub mod wrath_refer_friend;
pub mod wrath_world_objects;
pub mod wrath_session_misc;
pub mod wrath_capture;
pub mod wrath_inspect;
pub mod wrath_char_services;
pub mod wrath_faction;
pub mod wrath_loot;
pub mod wrath_logs;
pub mod wrath_movement;
pub mod wrath_npc;
pub mod wrath_relay;
pub mod wrath_session;
pub mod wrath_social;
pub mod wrath_who;
pub mod wrath_duel;
pub mod wrath_spline;
pub mod wrath_appearance;
pub mod wrath_arena_team;
pub mod wrath_guild;
pub mod wrath_guild_commands;
pub mod wrath_stable;
pub mod wrath_vendor;
pub mod wrath_channel;
pub mod wrath_mail;
pub mod wrath_mail_mutations;
pub mod wrath_roll;
pub mod wrath_taxi;
pub mod wrath_taxi_ack;
pub mod wrath_trainer;
pub mod wrath_world;
pub mod wrath_misc;
pub mod wrath_opcode_names;
pub mod wrath_progress;
pub use events::{
    CharAction, EntityKind, LoginRefusal, LoginStage, MoveSpeeds, Poll, SessionEnd,
    SessionEvent, SessionEventKind, SplineEffect, SplineInterpolation,
};
pub use types::field;
pub use types::{
    AttackSwingError, CharCreateReq, CharEnumItem, Character, CorpseLook, CreateSpline, ItemInfo,
    JumpInfo, MonsterMoveFacing, MoveMode, MoverState, ObjectFields, OwnerFallback, RelayVerb,
    SpeedKind, SplineMode, TransportPose, CHARACTER_FLAG_GHOST,
    CHARACTER_FLAG_HIDE_CLOAK, CHARACTER_FLAG_HIDE_HELM, CHARACTER_FLAG_RENAME,
};


/// The port a stock vmangos `realmd` listens on.
pub const AUTH_PORT: u16 = 3724;

/// Splits an optional `:port` off a host; one without a numeric port, a raw IPv6 address
/// included, comes back whole with `default`.
pub fn host_port(host: &str, default: u16) -> (&str, u16) {
    match host.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => match p.parse::<u16>() {
            Ok(p) => (h, p),
            Err(_) => (host, default),
        },
        _ => (host, default),
    }
}

/// A non-success realmd result byte, typed so the app can map it to its `AUTH_*` glue string; it
/// survives [`wrath_auth::logon`]'s `anyhow` contexts via `downcast_ref`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthReject {
    /// The grunt result byte (`WOW_FAIL_*`).
    pub code: u8,
}

impl std::fmt::Display for AuthReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "server rejected logon: result {:#04x}", self.code)
    }
}

impl std::error::Error for AuthReject {}

/// A dial that got no socket, split by step: the name did not resolve, or nothing answered (one
/// `io::Error` cannot tell them apart; macOS reports a failed lookup as `Uncategorized`).
#[derive(Debug, Clone)]
pub struct DialFailure {
    /// What we tried to reach, as the player would recognise it (`host:port`).
    pub address: String,
    /// `true` = the host name resolved to nothing; `false` = it resolved and the connection failed.
    pub unresolved: bool,
}

impl std::fmt::Display for DialFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.unresolved {
            write!(f, "cannot find a server called {}", self.address)
        } else {
            write!(f, "nothing answered at {}", self.address)
        }
    }
}

impl std::error::Error for DialFailure {}


/// A realm from realmd's realm list, with every field the wire carries.
#[derive(Debug, Clone)]
pub struct RealmInfo {
    pub name: String,
    /// `host:port` of the world server, as the client would connect to it.
    pub address: String,
    /// The population, sentinels already rewritten as the reference parser does
    /// ([`auth::MAGIC_POPULATIONS`]); the list shows a band against all realms' mean and deviation.
    pub population: f32,
    /// This account's character count there, shown as `"(3)"` (client realm record `+0x130`).
    pub characters: u8,
    /// The realm type (record `+0x04`), a join key, not an enum: `GetRealmInfo` takes `(pvp, rp)`
    /// from the `Cfg_Configs.dbc` row whose `RealmType` equals it.
    pub realm_type: u32,
    /// The realm flags as the client holds them (record `+0x08`): the wire carries only `0x01`
    /// invalid, `0x02` offline and `0x04`; `0x20`/`0x40`/`0x80` (Recommended, New, Full) are
    /// synthesized from the magic populations ([`auth::MAGIC_POPULATIONS`]).
    pub flags: u8,
    /// The category (timezone) byte the list's tabs group on, matched by equality, not an index.
    pub category: u8,
    /// The wire's realm id.
    pub id: u8,
}


#[cfg(test)]
mod host_port_tests {
    use super::{host_port, AUTH_PORT};

    #[test]
    fn explicit_port_splits() {
        assert_eq!(
            host_port("play.example.com:5000", AUTH_PORT),
            ("play.example.com", 5000)
        );
    }

    #[test]
    fn bare_host_gets_default() {
        assert_eq!(host_port("localhost", AUTH_PORT), ("localhost", AUTH_PORT));
    }

    #[test]
    fn raw_ipv6_is_not_split() {
        assert_eq!(host_port("::1", 3724), ("::1", 3724));
    }

    #[test]
    fn non_numeric_suffix_stays_host() {
        assert_eq!(
            host_port("play.example.com:realmd", 3724),
            ("play.example.com:realmd", 3724)
        );
    }
}

#[cfg(test)]
mod wrath_native_stream_tests;

#[cfg(test)]
mod wrath_native_codec_guard_tests;

pub mod wrath_pet;
pub mod wrath_spell_control;
pub mod wrath_speed;
pub mod wrath_movement_misc;

pub mod wrath_chat;

pub mod wrath_trade;
pub mod wrath_transfer;

pub mod wrath_logout;

pub mod wrath_pet_name;

pub mod wrath_quests_completed;
pub mod wrath_ui_timer;
pub mod wrath_mirror_image;
pub mod wrath_spell_failure;
