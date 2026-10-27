//! The guild wire messages: roster, guild query, invitations, rank administration and the event
//! broadcast (opcodes `0x54`/`0x55`, `0x81`-`0x93`, `0x231`-`0x235`, `0x2FC`).
//!
//! Deviation: string reads are unbounded. The reference `0x4191b0` caps each field, counting the
//! NUL (roster name `0x30`, notes `0x80`, MOTD `0x200`, info `0x7d0`, guild name `0x60`, rank name
//! `0x40`), and on overflow empties it and no-ops every later read in the packet. Unbounded reads
//! cannot poison a packet, and every cap is far above the server's own length limits, so a
//! legitimate packet reads the same.

/// Minimum rank count: the five defaults in [`guild_default_rank`] (`Guild/Guild.h:31`).
pub const GUILD_RANKS_MIN_COUNT: usize = 5;
/// Maximum rank count (`Guild/Guild.h:32`), and the exact number of rank names
/// `SMSG_GUILD_QUERY_RESPONSE` carries, unused ones empty (`Guild/Guild.cpp:868-872`).
pub const GUILD_RANKS_MAX_COUNT: usize = 10;

/// Rank-name cap in characters (`Guild/Guild.h:36`). The builders do not truncate, and an
/// over-long rank name gets the session kicked (`Handlers/GuildHandler.cpp:580-584`, `:600-604`).
pub const GUILD_RANK_MAX_LENGTH: usize = 15;
/// Guild-name cap, in characters (`Guild/Guild.h:37`).
pub const GUILD_NAME_MAX_LENGTH: usize = 24;
/// Public/officer note cap, in characters (`Guild/Guild.h:38`).
pub const GUILD_NOTE_MAX_LENGTH: usize = 31;
/// Guild info text cap, in characters (`Guild/Guild.h:39`).
pub const GUILD_INFO_MAX_LENGTH: usize = 500;
/// MOTD cap, in characters (`Guild/Guild.h:40`).
pub const GUILD_MOTD_MAX_LENGTH: usize = 128;

/// The five ranks every guild is created with and cannot delete (`Guild/Guild.h:44-54`). Ids are
/// 0-based with 0 = guild master; promote does `rank--`, demote `rank++`.
pub mod guild_default_rank {
    /// `GR_GUILDMASTER`; the server forces this rank's rights to `GR_RIGHT_ALL` whatever we send.
    pub const GUILDMASTER: u32 = 0;
    pub const OFFICER: u32 = 1;
    pub const VETERAN: u32 = 2;
    pub const MEMBER: u32 = 3;
    /// `GR_INITIATE`, the lowest default rank.
    pub const INITIATE: u32 = 4;
}

/// One rank's rights bitmask (`Guild/Guild.h:56-74`) with the `GR_RIGHT_EMPTY` bit factored out:
/// vmangos spells each right `0x40 | bit` for its test `(rights & right) != GR_RIGHT_EMPTY`.
pub mod guild_rank_right {
    /// `GR_RIGHT_EMPTY`, the sentinel vmangos ORs into every right; it grants nothing.
    pub const EMPTY: u32 = 0x0000_0040;
    /// Read guild chat (`GR_RIGHT_GCHATLISTEN`, vmangos `0x41`).
    pub const GCHAT_LISTEN: u32 = 0x0000_0001;
    /// Speak in guild chat (`GR_RIGHT_GCHATSPEAK`, vmangos `0x42`).
    pub const GCHAT_SPEAK: u32 = 0x0000_0002;
    /// Read officer chat (`GR_RIGHT_OFFCHATLISTEN`, vmangos `0x44`).
    pub const OFFCHAT_LISTEN: u32 = 0x0000_0004;
    /// Speak in officer chat (`GR_RIGHT_OFFCHATSPEAK`, vmangos `0x48`).
    pub const OFFCHAT_SPEAK: u32 = 0x0000_0008;
    /// Invite new members (`GR_RIGHT_INVITE`, vmangos `0x50`).
    pub const INVITE: u32 = 0x0000_0010;
    /// Kick members (`GR_RIGHT_REMOVE`, vmangos `0x60`).
    pub const REMOVE: u32 = 0x0000_0020;
    /// Promote members (`GR_RIGHT_PROMOTE`, vmangos `0xC0`).
    pub const PROMOTE: u32 = 0x0000_0080;
    /// Demote members (`GR_RIGHT_DEMOTE`, vmangos `0x140`).
    pub const DEMOTE: u32 = 0x0000_0100;
    /// Set the message of the day (`GR_RIGHT_SETMOTD`, vmangos `0x1040`).
    pub const SET_MOTD: u32 = 0x0000_1000;
    /// Edit any member's public note (`GR_RIGHT_EPNOTE`, vmangos `0x2040`).
    pub const EDIT_PUBLIC_NOTE: u32 = 0x0000_2000;
    /// See officer notes (`GR_RIGHT_VIEWOFFNOTE`, vmangos `0x4040`); without it the server blanks
    /// every roster `officerNote` (`Guild/Guild.cpp:821`, `:844`).
    pub const VIEW_OFFICER_NOTE: u32 = 0x0000_4000;
    /// Edit officer notes (`GR_RIGHT_EOFFNOTE`, vmangos `0x8040`).
    pub const EDIT_OFFICER_NOTE: u32 = 0x0000_8000;
    /// Edit the guild information text (`GR_RIGHT_MODIFY_GUILD_INFO`, vmangos `0x10040`).
    pub const MODIFY_GUILD_INFO: u32 = 0x0001_0000;
    /// `GR_RIGHT_ALL`, forced onto rank 0 by the server. A raw vmangos value: it includes
    /// [`EMPTY`] and bits no 1.12 right owns, so test ranks against the named bits, never this.
    pub const ALL: u32 = 0x000F_F1FF;
}

/// A roster member's presence byte (`Guild/Guild.h:139-144`); only `0` changes the wire, adding
/// the `f32 lastOnlineTime`. 12340's `GetGuildRosterInfo` (`0x5cc9c0`) tests DND before AFK,
/// so a member flagged both shows as DND.
pub mod guild_presence {
    /// Offline; only this value carries the `lastOnlineTime` float.
    pub const OFFLINE: u8 = 0x00;
    /// `GRF_ONLINE` (vmangos value; the reference only tests the whole byte against zero).
    pub const ONLINE: u8 = 0x01;
    /// `GRF_AFK`, set alongside [`ONLINE`]; the reference's own `CHAT_FLAG_AFK`.
    pub const AFK: u8 = 0x02;
    /// `GRF_DND`, set alongside [`ONLINE`]; the reference's own `CHAT_FLAG_DND`.
    pub const DND: u8 = 0x04;
}

/// `SMSG_GUILD_EVENT`'s leading byte (`Guild/Guild.h:121-137`), the arms of the reference
/// handler's `switch` (jump table `0x5e74dc`).
pub mod guild_event {
    /// `GE_PROMOTION`, params: promoter, promoted, new rank name.
    pub const PROMOTION: u8 = 0x00;
    /// `GE_DEMOTION`, params: demoter, demoted, new rank name.
    pub const DEMOTION: u8 = 0x01;
    /// `GE_MOTD`, param: the new MOTD. The reference routes it through the CVar-gated profanity
    /// filter (`0x703f50`), not the normal event path.
    pub const MOTD: u8 = 0x02;
    /// `GE_JOINED`, param: the joiner's name.
    pub const JOINED: u8 = 0x03;
    /// `GE_LEFT`, param: the leaver's name.
    pub const LEFT: u8 = 0x04;
    /// `GE_REMOVED`, params: the removed player, then who removed them.
    pub const REMOVED: u8 = 0x05;
    /// `GE_LEADER_IS`, param: the current leader.
    pub const LEADER_IS: u8 = 0x06;
    /// `GE_LEADER_CHANGED`, params: old leader, new leader.
    pub const LEADER_CHANGED: u8 = 0x07;
    /// `GE_DISBANDED`, no params.
    pub const DISBANDED: u8 = 0x08;
    /// `GE_TABARDCHANGE`; the reference handles it in its default arm.
    pub const TABARD_CHANGE: u8 = 0x09;
    /// `GE_UPDATE_RANK_NAME`, params: rank id, new rank name. The reference writes it into the
    /// roster cache (`0x560e30`) and fires no FrameScript event.
    pub const UPDATE_RANK_NAME: u8 = 0x0A;
    /// `GE_UPDATE_ROSTER`: silent in the reference, and vmangos never sends it.
    pub const UPDATE_ROSTER: u8 = 0x0B;
    /// `GE_SIGNED_ON`, param: the guildmate's name; carries the trailing guid.
    pub const SIGNED_ON: u8 = 0x0C;
    /// `GE_SIGNED_OFF`, param: the guildmate's name; carries the trailing guid.
    pub const SIGNED_OFF: u8 = 0x0D;
    /// `GE_BANK_TEXT_CHANGED`, param: the 0-based tab. 12340's handler (`0x6d92d0`, case `0x13`)
    /// fires `GUILDBANK_TEXT_CHANGED(tab + 1)` and no roster update.
    pub const BANK_TEXT_CHANGED: u8 = 0x13;
}

/// `SMSG_GUILD_COMMAND_RESULT`'s leading `u32`, the verb the result is about
/// (`Guild/Guild.h:76-85`). Coarser than the request: vmangos reports "not in a guild" for most
/// verbs as [`CREATE`] and every permission refusal as [`INVITE`].
pub mod guild_command {
    /// `GUILD_CREATE_S`, also vmangos's catch-all for "you are not in a guild".
    pub const CREATE: u32 = 0x00;
    /// `GUILD_INVITE_S`, also vmangos's catch-all for a permission refusal.
    pub const INVITE: u32 = 0x01;
    /// `GUILD_QUIT_S`: leaving or being removed.
    pub const QUIT: u32 = 0x03;
    pub const FOUNDER: u32 = 0x0E;
    pub const UNK19: u32 = 0x13;
    pub const UNK20: u32 = 0x14;
}

/// `SMSG_GUILD_COMMAND_RESULT`'s trailing `u32` (`Guild/Guild.h:96-119`). [`LEADER_LEAVE`] and
/// [`PERMISSIONS`] share `0x08`: with command `QUIT` it is the leader-cannot-leave message,
/// otherwise the permission refusal.
pub mod guild_command_error {
    /// `ERR_PLAYER_NO_MORE_IN_GUILD`: "no message/error" per vmangos.
    pub const PLAYER_NO_MORE_IN_GUILD: u32 = 0x00;
    /// `ERR_GUILD_INTERNAL`.
    pub const INTERNAL: u32 = 0x01;
    /// `ERR_ALREADY_IN_GUILD`: you are.
    pub const ALREADY_IN_GUILD: u32 = 0x02;
    /// `ERR_ALREADY_IN_GUILD_S`: they are (the string names them).
    pub const ALREADY_IN_GUILD_S: u32 = 0x03;
    /// `ERR_INVITED_TO_GUILD`.
    pub const INVITED_TO_GUILD: u32 = 0x04;
    /// `ERR_ALREADY_INVITED_TO_GUILD_S`.
    pub const ALREADY_INVITED_TO_GUILD_S: u32 = 0x05;
    /// `ERR_GUILD_NAME_INVALID`.
    pub const NAME_INVALID: u32 = 0x06;
    /// `ERR_GUILD_NAME_EXISTS_S`.
    pub const NAME_EXISTS_S: u32 = 0x07;
    /// `ERR_GUILD_LEADER_LEAVE`: the guild master cannot `/gquit` while anyone else remains.
    pub const LEADER_LEAVE: u32 = 0x08;
    /// `ERR_GUILD_PERMISSIONS`, sharing `0x08` with [`LEADER_LEAVE`].
    pub const PERMISSIONS: u32 = 0x08;
    /// `ERR_GUILD_PLAYER_NOT_IN_GUILD`: you aren't.
    pub const PLAYER_NOT_IN_GUILD: u32 = 0x09;
    /// `ERR_GUILD_PLAYER_NOT_IN_GUILD_S`: they aren't.
    pub const PLAYER_NOT_IN_GUILD_S: u32 = 0x0A;
    /// `ERR_GUILD_PLAYER_NOT_FOUND_S`.
    pub const PLAYER_NOT_FOUND_S: u32 = 0x0B;
    /// `ERR_GUILD_NOT_ALLIED`: cross-faction, refused by config.
    pub const NOT_ALLIED: u32 = 0x0C;
    /// `ERR_GUILD_RANK_TOO_HIGH_S`.
    pub const RANK_TOO_HIGH_S: u32 = 0x0D;
    /// `ERR_GUILD_RANK_TOO_LOW_S`.
    pub const RANK_TOO_LOW_S: u32 = 0x0E;
    /// `ERR_GUILD_RANKS_LOCKED` (`0x0F`/`0x10` are unused at 5875).
    pub const RANKS_LOCKED: u32 = 0x11;
    /// `ERR_GUILD_RANK_IN_USE`: the lowest rank cannot be deleted while somebody holds it.
    pub const RANK_IN_USE: u32 = 0x12;
    /// `ERR_GUILD_IGNORING_YOU_S`.
    pub const IGNORING_YOU_S: u32 = 0x13;
    /// `ERR_GUILD_UNK20`: "for Typecommand 0x05 only" per vmangos.
    pub const UNK20: u32 = 0x14;
}

/// `SMSG_GUILD_QUERY_RESPONSE`: one guild's name, ten rank names and tabard, by guild id.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildQueryResponse {
    /// The guild id we asked about.
    pub guild_id: u32,
    /// The guild's name; empty means no such guild, and the reference then drops the id from its
    /// cache instead of inserting it (`0x5552ae`).
    pub name: String,
    /// Indexed by rank id, uncreated ranks empty; the live rank count is the roster's
    /// `rank_rights` length.
    pub rank_names: [String; GUILD_RANKS_MAX_COUNT],
    /// Tabard emblem index, formatted into `Textures\GuildEmblems\Emblem_<style>_<color>_…`. All
    /// five tabard fields are `int32` (`Server/Packets/Guild.h:208`) and a new guild has each at
    /// `-1` (`Guild/Guild.cpp:86`), meaning no tabard designed.
    pub emblem_style: i32,
    /// Tabard emblem colour index. Signed, `-1` = no tabard designed.
    pub emblem_color: i32,
    /// Tabard border style index. Signed, `-1` = no tabard designed.
    pub border_style: i32,
    /// Tabard border colour index. Signed, `-1` = no tabard designed.
    pub border_color: i32,
    /// Tabard background colour index. Signed, `-1` = no tabard designed.
    pub background_color: i32,
}

/// One row of `SMSG_GUILD_ROSTER`; every field the guild pane shows is on the wire.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GuildRosterMember {
    /// The member's player guid (vmangos always builds it as `ObjectGuid(HIGHGUID_PLAYER, …)`).
    pub guid: u64,
    /// [`guild_presence`] bits; `0` puts [`Self::last_online_days`] on the wire.
    pub presence: u8,
    pub name: String,
    /// Rank id, 0-based with 0 = guild master; indexes [`GuildQueryResponse::rank_names`] and
    /// [`GuildRoster::rank_rights`].
    pub rank_id: u32,
    pub level: u8,
    /// Class id (`ChrClasses.dbc`).
    pub class: u8,
    /// Zone id (`AreaTable.dbc`); an offline member's last known zone.
    pub zone: u32,
    /// Fractional days since logout (`Guild/Guild.cpp:841`), sent for offline members only;
    /// `GetGuildRosterLastOnline` splits it into years, months and days.
    pub last_online_days: f32,
    pub public_note: String,
    /// The member's officer note, blanked by the server unless the viewer holds
    /// [`guild_rank_right::VIEW_OFFICER_NOTE`] (`Guild/Guild.cpp:821`, `:844`).
    pub officer_note: String,
}

impl GuildRosterMember {
    /// Any presence bit set. A whole-byte test, not `& ONLINE`, as it is the wire's condition for
    /// `lastOnlineTime` in vmangos and in the reference (`0x4d0c12`, `0x4d0c1d`).
    pub fn is_online(&self) -> bool {
        self.presence != guild_presence::OFFLINE
    }
}

/// `SMSG_GUILD_ROSTER`: the whole guild, re-sent complete on any change. The server truncates the
/// member list to fit `GUILD_ROSTER_MAX_LENGTH` (`0x8000 - 4` bytes, `Guild/Guild.h:41`), so a
/// large guild can arrive short.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GuildRoster {
    pub motd: String,
    /// The "Guild Information" text, written only for builds above `CLIENT_BUILD_1_8_4`.
    pub info: String,
    /// One [`guild_rank_right`] mask per rank, by rank id; the length is the guild's real rank
    /// count. More than [`GUILD_RANKS_MAX_COUNT`] is malformed but read in full, so a ten-slot
    /// consumer bounds itself (the reference overruns its array at `0xb726d0`).
    pub rank_rights: Vec<u32>,
    /// The members in wire order, by low guid; the reference re-sorts its own copy after every
    /// packet (`0x4d0d32`), so sorting is the consumer's job.
    pub members: Vec<GuildRosterMember>,
}

/// `SMSG_GUILD_EVENT`: a guild event id and its `%s` arguments.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildEventNotice {
    /// A [`guild_event`] id; the reference sends anything above `0x0D` to its default arm.
    pub event: u8,
    /// Every string `strCount` announced, uncapped like the reference read loop; the reference
    /// display passes none of them when there are four or more (`0x5e745f`).
    pub params: Vec<String>,
    /// The guildmate's guid, read only for [`guild_event::SIGNED_ON`] and
    /// [`guild_event::SIGNED_OFF`].
    pub guid: Option<u64>,
}

/// `SMSG_GUILD_COMMAND_RESULT`: the server's verdict on a guild verb we sent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildCommandResult {
    /// A [`guild_command`] tag; it decides which of result `0x08`'s two meanings applies.
    pub command: u32,
    /// The `%s` of the matching `ERR_GUILD_*` string: a player name, a guild name, or empty.
    pub name: String,
    /// A [`guild_command_error`] code.
    pub result: u32,
}

/// `SMSG_GUILD_INFO`, answering `CMSG_GUILD_INFO`: founding date, member and account counts. The
/// field names are vmangos's; the reference formats wire fields 2, 1, 3 into its enUS `M/D/Y`
/// `GUILD_INFO_TEMPLATE` (`0x5e704d`), which fits day, month, year.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildInfo {
    pub name: String,
    pub created_day: u32,
    pub created_month: u32,
    pub created_year: u32,
    pub member_count: u32,
    /// How many distinct accounts the members belong to.
    pub account_count: u32,
}

/// Body of `CMSG_GUILD_RANK` (`Server/Packets/Guild.cpp:78-83`): rank id, rights, name. Both are
/// set, so changing one resends the other; rank 0's rights become [`guild_rank_right::ALL`], and
/// a name over [`GUILD_RANK_MAX_LENGTH`] gets the session kicked.
pub fn guild_rank(rank_id: u32, rights: u32, name: &str) -> Vec<u8> {
    let mut body = Vec::with_capacity(9 + name.len());
    body.extend_from_slice(&rank_id.to_le_bytes());
    body.extend_from_slice(&rights.to_le_bytes());
    push_cstring(&mut body, name);
    body
}

fn push_cstring(body: &mut Vec<u8>, s: &str) {
    body.extend_from_slice(s.as_bytes());
    body.push(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The expected values are vmangos's `GuildRankRights` (`Guild/Guild.h:56-74`).
    #[test]
    fn rank_rights_reconstruct_the_vmangos_constants() {
        use guild_rank_right::*;
        assert_eq!(GCHAT_LISTEN | EMPTY, 0x0000_0041);
        assert_eq!(GCHAT_SPEAK | EMPTY, 0x0000_0042);
        assert_eq!(OFFCHAT_LISTEN | EMPTY, 0x0000_0044);
        assert_eq!(OFFCHAT_SPEAK | EMPTY, 0x0000_0048);
        assert_eq!(PROMOTE | EMPTY, 0x0000_00C0);
        assert_eq!(DEMOTE | EMPTY, 0x0000_0140);
        assert_eq!(INVITE | EMPTY, 0x0000_0050);
        assert_eq!(REMOVE | EMPTY, 0x0000_0060);
        assert_eq!(SET_MOTD | EMPTY, 0x0000_1040);
        assert_eq!(EDIT_PUBLIC_NOTE | EMPTY, 0x0000_2040);
        assert_eq!(VIEW_OFFICER_NOTE | EMPTY, 0x0000_4040);
        assert_eq!(EDIT_OFFICER_NOTE | EMPTY, 0x0000_8040);
        assert_eq!(MODIFY_GUILD_INFO | EMPTY, 0x0001_0040);
    }
}
