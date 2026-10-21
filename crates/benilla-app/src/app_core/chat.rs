//! The chat event model and its queues: every line the session, combat log and UI raise
//! (`ChatEvent`, `ChatEventKind`), the client-composed lines, pending broadcasts and
//! the drain's log. `ui_chat` keeps the systems and re-exports these from their old paths.

use benilla_protocol::types::{ChatMessage, LevelUpInfo};
use bevy::prelude::*;

/// One value slot of a client-composed line's format string, in the order the shipped template
/// consumes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// `%s`: the subject's name, resolved at the drain off [`PendingCombat::subject`].
    Subject,
    /// `%s`: a name, already localized (a faction) or resolved at the drain
    /// ([`PendingCombat::item`]).
    Named,
    /// `%d`: the amount.
    Amount,
}

/// A client-composed line: its GlobalString key and the ordered slots its template consumes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Family {
    pub key: &'static str,
    pub slots: &'static [Slot],
}

/// The value bound to each [`Slot`] for one line.
#[derive(Clone, Debug, Default)]
pub(crate) struct Fills {
    /// The [`Slot::Subject`] text.
    pub subject: String,
    /// The [`Slot::Named`] text.
    pub named: String,
    pub amount: i64,
}

/// A client-composed line queued for the drain, which resolves its names and then its template off
/// the VM's `GlobalStrings.lua`.
#[derive(Clone, Debug)]
pub(crate) struct PendingCombat {
    pub kind: ChatEventKind,
    pub family: Family,
    pub fills: Fills,
    /// The unit naming [`Slot::Subject`], through the name cache; 0 when the family has none.
    pub subject: u64,
    /// The item naming [`Slot::Named`], through the item cache, as the reference's line waits on
    /// its item callback (`0x67ca30`).
    pub item: Option<u32>,
    /// Drain passes spent waiting on a name.
    pub tries: u16,
}

/// The chat-event kinds: the `ChatTypeInfo` keys benilla produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ChatEventKind {
    Say,
    Party,
    Raid,
    Guild,
    Officer,
    Yell,
    Whisper,
    WhisperInform,
    Emote,
    TextEmote,
    System,
    MonsterSay,
    MonsterYell,
    MonsterEmote,
    MonsterWhisper,
    Channel,
    ChannelJoin,
    ChannelLeave,
    ChannelNotice,
    ChannelNoticeUser,
    ChannelList,
    Afk,
    Dnd,
    Ignored,
    Skill,
    Loot,
    Money,
    /// The client-composed XP line (`SMSG_LOG_XPGAIN`).
    CombatXpGain,
    /// The client-composed honor line (`SMSG_PVP_CREDIT`), built once the victim's name resolves.
    CombatHonorGain,
    RaidLeader,
    RaidWarning,
    RaidBossEmote,
    /// `CHAT_MSG_FILTERED` (`0x5B`), the server's "your message was filtered" notice; `arg2` is the
    /// addressee the stock frame formats `CHAT_FILTERED` over (`ChatFrame.lua:1406`).
    Filtered,
    Battleground,
    BattlegroundLeader,
    BgSystemNeutral,
    BgSystemAlliance,
    BgSystemHorde,
    // ── the combat log ───────────────────────────────────────────────────────────────
    // Every kind below has a producer.
    /// `0x19`, the reference's catch-all combat line: `DURABILITYDAMAGE_DEATH`, `SPELLDISMISSPET*`,
    /// `SPELLHAPPINESSDRAIN*` and the "string not found" warning.
    CombatMiscInfo,
    /// `0x55`, a reputation delta (`FACTION_STANDING_INCREASED`/`_DECREASED`).
    CombatFactionChange,
    /// `0x1e`, a cast's created item (`TRADESKILL_LOG_*`, `0x50bd90`).
    Tradeskills,
    /// `0x1f`, the pet eating what it was fed (`FEEDPET_LOG_*`, `0x50baf0`).
    PetInfo,
}

/// One `CHAT_MSG_*` fire, typed. The fire helper `0x49b0b0` signals it (`0x703f50`) with the
/// format `"%s%s%s%s%s%s%d%d%s%d"` (`.rdata 0x844608`): arg7, arg8 and arg10 are numbers, the
/// rest strings, never nil, as `ChatFrame_OnEvent` compares `arg7 > 0` bare.
///
/// | arg | field | what `ChatFrame_OnEvent` reads |
/// |---|---|---|
/// | 1 | `text`, `notice` | the body, or a channel notice's `CHAT_<token>_NOTICE` token |
/// | 2 | `sender` | the speaker, or a notice's affected player |
/// | 3 | `language` | a language name; empty prints no header |
/// | 4 | `channel` | the display form, "N. Name - Zone" when numbered |
/// | 5 | `target` | a two-name notice's second name, "X kicked by Y" |
/// | 6 | `flag` | "AFK", "DND" or "GM", as `CHAT_FLAG_<flag>` |
/// | 7 | `zone_channel_id` | the ChannelID, 0 for a custom channel (`ChatFrame.lua:1379`) |
/// | 8 | `channel_number` | the local slot, `ChatTypeInfo["CHANNEL"..arg8]` |
/// | 9 | `channel_base` | the channel name without its number |
/// | 10 | none | the split index, 0 from vmangos (`Chat/Channel.cpp:823-827`) |
///
/// arg4, arg7, arg8 and arg9 are one record (`ui_chat::edit::ChannelState::stamp_channel`). arg1
/// is the garbled text: the reference fills one buffer (`SStrCopy` at `0x49a9f0`, the garble
/// `0x49b560` at `0x49aa7c`) that the line, arg1 and the bubble share (`ui_chat::language`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ChatEvent {
    pub kind: Option<ChatEventKind>,
    pub text: String,
    pub sender: String,
    pub language: String,
    pub channel: String,
    pub target: String,
    pub flag: String,
    /// arg7, the ChannelID behind a zone channel (1 General, 2 Trade, 22 LocalDefense), else 0.
    pub zone_channel_id: u32,
    pub channel_number: u32,
    pub channel_base: String,
    pub notice: String,
    /// Our slot's state for this channel as the notice arrived (`None`: no slot), set before
    /// `ui_chat::feed::deliver` changes it: the reference's notice arms read `slot+0x9c` for the
    /// token (`0x49c0c2`) before they write it (`0x49bb20`).
    pub slot_state: Option<SlotState>,
}

/// One parked world broadcast, as the wire sent it, awaiting the catalogs and the channel walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Broadcast {
    /// `SMSG_ZONE_UNDER_ATTACK`: the attacked area's `AreaTable.dbc` id.
    ZoneUnderAttack { area_id: u32 },
    /// `SMSG_DEFENSE_MESSAGE`: the zone it is about, and the server's text.
    Defense { zone_id: u32, text: String },
    /// `SMSG_SERVER_MESSAGE`: a `ServerMessages.dbc` row id and the text for its `%s`.
    Server { message_type: u32, text: String },
    /// `SMSG_CHAT_RESTRICTED`, which carries nothing.
    ChatRestricted,
}

/// One queued item awaiting its turn through [`feed_chat`].
pub(crate) enum Pending {
    Native {
        msg: benilla_protocol::wrath_chat::Message,
        tries: u16,
    },
    /// A decoded `SMSG_MESSAGECHAT`; the player types wait on the sender's name.
    Wire { msg: ChatMessage, tries: u16 },
    /// A channel notice with guids to name (`b_guid` is a kick's or ban's actor, 0 when absent).
    Notice {
        notice: u8,
        channel: String,
        a_guid: u64,
        b_guid: u64,
        tries: u16,
    },
    /// An inbound addon line waiting on its sender's name: the reference fires `CHAT_MSG_ADDON`
    /// (event 227, `0x49a95f`) from the name-query callback (`0x49ccc0`), so `sender` is a name.
    Addon {
        prefix: String,
        message: String,
        distribution: String,
        guid: u64,
        tries: u16,
    },
    /// A `/random` broadcast awaiting the roller's name.
    Roll {
        min: u32,
        max: u32,
        roll: u32,
        guid: u64,
        tries: u16,
    },
    /// An XP award waiting on the victim's name (0: unnamed); `bonus` is total minus base.
    XpGain {
        victim: u64,
        total: u32,
        bonus: u32,
        tries: u16,
    },
    /// An honor award waiting on the victim's name; `rank` (0 none) becomes a title at the drain,
    /// from the victim's side in the same name-query record.
    HonorGain {
        victim: u64,
        honor: i32,
        rank: u8,
        tries: u16,
    },
    /// A text emote waiting on its performer's name: the reference's `PENDINGTEXTEMOTE` node
    /// (`0x49cc00`), queued when `0x49dbe0` finds no cached name and drained by `0x49d0d0`.
    /// `target` is the wire's name, empty when untargeted, used as sent.
    TextEmote {
        performer: u64,
        text_id: u32,
        target: String,
        tries: u16,
    },
    /// An area discovery (`SMSG_EXPLORATION_EXPERIENCE`), fired at the drain, which holds the VM.
    Discovery { area: String, xp: u32 },
    /// One combat-log line, classified at the packet, waiting on its two endpoint names; the
    /// reference's deferred queue (`0xc4e208`) waits only on an item name.
    Combat(Box<PendingCombat>),
    /// A ready event (client-composed lines; name-carrying notices).
    Event(ChatEvent),
}

/// The chat items the feeds queue and [`feed_chat`] drains. World broadcasts wait apart, as their
/// resolve needs more than [`feed_chat`] can hold at Bevy's `SystemParam` limit;
/// `feed_broadcasts` resolves them onto `pending` earlier in the same frame.
#[derive(Resource, Default)]
pub(crate) struct ChatLog {
    pub(crate) pending: Vec<Pending>,
    pub(crate) broadcasts: Vec<Broadcast>,
    /// A level-up's gains for `ui_unit`'s `PLAYER_LEVEL_UP` fire, matched by level: the fire
    /// follows a descriptor change and the gains a packet.
    pub(crate) level_up_gains: Vec<(LevelUpInfo, u32)>,
}

/// The reference's per-slot state (`+0x9c`), in the values whose notice token differs: the stock
/// `ChatFrame_OnEvent`'s `YOU_LEFT` branch deletes the window's channel registration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SlotState {
    /// `0` (confirmed) and `1` (join sent) in one: the reference's walk sends a LEAVE only from 0,
    /// so an unconfirmed slot here sends one the server answers "Not on channel".
    #[default]
    Joined,
    /// `2`, renamed with its re-join pending (`0x49bcd3`, in the rename `0x49bc50`): the confirming
    /// `YOU_JOINED` carries the `YOU_CHANGED` token, "Changed Channel: [%s]".
    Renamed,
    /// `3`, suspended (`0x49bcf0`): the row stopped applying, in 1.12 data a city-only channel
    /// outside a capital. The record and number survive, and `YOU_LEFT` carries the `SUSPENDED`
    /// token (`0x49c0e0`), which keeps the window's registration.
    Suspended,
}
