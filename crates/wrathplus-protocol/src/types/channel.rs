//! The channel notices and member roster (vmangos `Chat/Channel.cpp`).

/// `SMSG_CHANNEL_NOTIFY`'s notice byte (`ChatNotify`, vmangos `Chat/Channel.h:35-71`).
pub mod channel_notice {
    pub const JOINED: u8 = 0x00;
    pub const LEFT: u8 = 0x01;
    pub const YOU_JOINED: u8 = 0x02;
    pub const YOU_LEFT: u8 = 0x03;
    pub const WRONG_PASSWORD: u8 = 0x04;
    pub const NOT_MEMBER: u8 = 0x05;
    pub const NOT_MODERATOR: u8 = 0x06;
    pub const PASSWORD_CHANGED: u8 = 0x07;
    pub const OWNER_CHANGED: u8 = 0x08;
    pub const PLAYER_NOT_FOUND: u8 = 0x09;
    pub const NOT_OWNER: u8 = 0x0A;
    pub const CHANNEL_OWNER: u8 = 0x0B;
    pub const MODE_CHANGE: u8 = 0x0C;
    pub const ANNOUNCEMENTS_ON: u8 = 0x0D;
    pub const ANNOUNCEMENTS_OFF: u8 = 0x0E;
    pub const MODERATION_ON: u8 = 0x0F;
    pub const MODERATION_OFF: u8 = 0x10;
    pub const MUTED: u8 = 0x11;
    pub const PLAYER_KICKED: u8 = 0x12;
    pub const BANNED: u8 = 0x13;
    pub const PLAYER_BANNED: u8 = 0x14;
    pub const PLAYER_UNBANNED: u8 = 0x15;
    pub const PLAYER_NOT_BANNED: u8 = 0x16;
    pub const PLAYER_ALREADY_MEMBER: u8 = 0x17;
    pub const INVITE: u8 = 0x18;
    pub const INVITE_WRONG_FACTION: u8 = 0x19;
    pub const WRONG_FACTION: u8 = 0x1A;
    pub const INVALID_NAME: u8 = 0x1B;
    pub const NOT_MODERATED: u8 = 0x1C;
    pub const PLAYER_INVITED: u8 = 0x1D;
    pub const PLAYER_INVITE_BANNED: u8 = 0x1E;
    pub const THROTTLED: u8 = 0x1F;
}

/// What follows the notice byte and channel name, by notice (`Chat/Channel.cpp:804-1008`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelNoticeTail {
    /// The member who joined or left.
    Guid(u64),
    /// Our channel flags; the `u32` after them is always 0.
    YouJoined { flags: u32 },
    /// Nothing follows the channel name.
    Empty,
    /// The player who made the change or sent the invite.
    Actor(u64),
    /// A player name; `CHANNEL_OWNER` may send the literal `"Nobody"` or `"PLAYER_NOT_FOUND"`.
    Name(String),
    /// The member and its `ChannelMemberFlags` before and after.
    ModeChange {
        guid: u64,
        old_flags: u8,
        new_flags: u8,
    },
    /// The affected player, then who acted.
    Actors { target: u64, source: u64 },
}

/// One decoded `SMSG_CHANNEL_NOTIFY`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelNotify {
    pub notice: u8,
    pub channel: String,
    pub tail: ChannelNoticeTail,
}
