//! The world packets' shared payload types (`SessionEvent` payloads, the descriptor model) and
//! the remaining 1.12-era bodies; build 12340's codecs live in the `wrath_*` modules.

mod action_bar;
pub mod addons;
mod area_trigger;
mod attack;
mod auction;
mod bank;
mod battlefield;
mod binder;
mod broadcast;
mod channel;
mod chat;
mod client;
mod combat_log;
mod death;
mod duel;
mod gameobject;
mod gm_ticket;
mod gossip;
mod group;
mod guild;
mod instance;
mod items;
mod loot;
mod mail;
mod meeting_stone;
mod mirror_timer;
mod movement;
mod packet;
mod page_text;
mod pet;
mod petition;
mod pose;
mod progression;
mod pvp;
mod quest;
mod reputation;
mod roster;
mod skills;
mod social;
mod spellbook;
mod spells;
mod stable;
mod summon;
mod tabard;
mod taxi;
mod trade;
mod trainer;
mod tutorial;
mod update_object;
mod vendor;
mod world_state;
pub use action_bar::{
    set_action_button, ActionButton, ACTION_KIND_ITEM, ACTION_KIND_MACRO,
    ACTION_KIND_SPELL,
};
pub use addons::{SecureAddon, STANDARD_MODULUS_CRC, STOCK_SECURE_ADDONS};
pub use area_trigger::area_trigger;
pub use attack::{AttackSwingError, AttackerState};
pub use auction::{
    auction_action, auction_duration, auction_error, auction_filter, auction_hello,
    auction_list_items, auction_place_bid,
    auction_remove_item, auction_sell_item, AuctionBidderNotification, AuctionCommandTail,
    AuctionListEntry, AuctionOwnerNotification, AUCTION_PAGE_SIZE, AUCTION_RECORD_BYTES,
};
pub use bank::{
    bank_slot_result, };
pub use battlefield::{
    BattlefieldList, BattlefieldPosition, BattlefieldPositions, BattlefieldStatus, PvpLogData,
    PvpLogRow, BATTLEFIELD_POSITIONS_MAX,
};
pub use binder::{PlayerBound};
pub use channel::{channel_notice, ChannelNoticeTail, ChannelNotify};
pub use chat::{
    chat_tag, ChatMessage, CHAT_MSG_AFK, CHAT_MSG_BATTLEGROUND, CHAT_MSG_BATTLEGROUND_LEADER,
    CHAT_MSG_BG_SYSTEM_ALLIANCE, CHAT_MSG_BG_SYSTEM_HORDE, CHAT_MSG_BG_SYSTEM_NEUTRAL,
    CHAT_MSG_CHANNEL, CHAT_MSG_DND, CHAT_MSG_EMOTE, CHAT_MSG_FILTERED, CHAT_MSG_GUILD,
    CHAT_MSG_IGNORED, CHAT_MSG_MONSTER_EMOTE, CHAT_MSG_MONSTER_SAY, CHAT_MSG_MONSTER_WHISPER,
    CHAT_MSG_MONSTER_YELL, CHAT_MSG_OFFICER, CHAT_MSG_PARTY, CHAT_MSG_RAID,
    CHAT_MSG_RAID_BOSS_EMOTE, CHAT_MSG_RAID_BOSS_WHISPER, CHAT_MSG_RAID_LEADER,
    CHAT_MSG_RAID_WARNING, CHAT_MSG_SAY, CHAT_MSG_SYSTEM, CHAT_MSG_WHISPER,
    CHAT_MSG_WHISPER_INFORM, CHAT_MSG_YELL, MACRO_EXPANDED_TYPES,
};
pub use client::{
    channel_list,
    char_create,
    creature_query, movement, ping, played_time, text_emote,
};
pub use combat_log::{
    DamageShield, DispelFailed, DispelledAura, EnchantmentLog, EnvironmentalDamageLog, ExecuteLog,
    PartyKillLog, PeriodicAuraLog, PeriodicTick, SpellDamageLog, SpellDispelLog, SpellEnergizeLog,
    SpellHealLog, SpellInstaKillLog, SpellLogExecute, SpellLogMiss, SpellOutcomeLog,
};
pub use death::{
    AreaSpiritHealerTime, CorpseLocation, ResurrectRequestBody,
};
pub use duel::{
    DuelRequested, DuelWinner,
};
pub use gameobject::{GameObjectQueryInfo};
pub use gm_ticket::{
    GmTicket, GMTICKET_QUEUE_ENABLED,
    GMTICKET_STATUS_DEFAULT, GMTICKET_STATUS_HASTEXT, RESERVED_FOR_FUTURE_USE,
};
pub use gossip::{
    npc_text_query, select_greeting, GossipOption, GossipPoi,
    NpcTextBlock, QuestOption, NPC_TEXT_BLOCKS,
};
pub use group::{
    loot_method, member_status, minimap_ping, party_member_mask,
    party_operation, party_result, patch_auras, GroupLootInfo, GroupMemberEntry, PartyMemberStatsInfo, RaidInstanceEntry, RaidTargetUpdate,
    ReadyCheck, GROUP_MEMBER_ASSISTANT,
};
pub use guild::{
    guild_command, guild_command_error, guild_default_rank, guild_event, guild_presence,
    guild_rank, guild_rank_right, GuildCommandResult, GuildEventNotice, GuildInfo,
    GuildQueryResponse, GuildRoster, GuildRosterMember, GUILD_INFO_MAX_LENGTH,
    GUILD_MOTD_MAX_LENGTH, GUILD_NAME_MAX_LENGTH, GUILD_NOTE_MAX_LENGTH, GUILD_RANKS_MAX_COUNT,
    GUILD_RANKS_MIN_COUNT, GUILD_RANK_MAX_LENGTH,
};
pub use instance::{
    InstanceResetFailed, InstanceResetFailure, RaidGroupOnly, RaidInstanceMessage,
    RaidInstanceWarning,
};
pub use items::{
    item_query, open_item, use_item, wrap_item, ItemDamage, ItemInfo,
    ItemSpellEntry, ItemUseSpell, UseItemTarget, BAG_PLAYER_INVENTORY, ITEM_DYNFLAG_UNLOCKED,
    ITEM_DYNFLAG_WRAPPED, ITEM_FLAG_LOOTABLE, ITEM_FLAG_WRAPPER, SLOT_BAG_FIRST, SLOT_PACK_FIRST,
};
pub use loot::{
    autostore_loot_item, loot, loot_error, loot_master_give, loot_roll,
    loot_type, roll_vote, slot_type, ItemPushResult, LootAllPassed, LootItem, LootResponseBody,
    LootRoll, LootRollWon, LootStartRoll,
};
pub use mail::{
    item_text_query, mail_action, mail_error,
    mail_message_type, mail_take_money,
    MailAttachment, MailListEntry,
};
pub use meeting_stone::{
    MeetingStoneNotice, MeetingStoneSetQueue,
};
pub use mirror_timer::{
    MirrorTimerKind,
    MirrorTimerStart,
};
pub use movement::{
    JumpInfo, MoveMode, MovementInfo, RelayVerb, SpeedKind, SplineMode, TransportPose,
};
pub use packet::{CreatureQueryInfo, MonsterMoveFacing};
pub use pet::{
    pet_action, pet_tame_failure_key, PetActionEntry, PetMode, PetSpellCooldown,
    PetSpells, PetUnlearnConfirm, PET_ACTION_SLOTS, PET_ACT_COMMAND, PET_ACT_DISABLED,
    PET_ACT_ENABLED, PET_ACT_PASSIVE, PET_ACT_REACTION, PET_AUTOCAST_ALLOWED, PET_AUTOCAST_ON,
    PET_COMMAND_ATTACK, PET_COMMAND_DISMISS, PET_COMMAND_FOLLOW, PET_COMMAND_STAY,
    PET_COOLDOWN_PERMANENT, PET_REACT_AGGRESSIVE, PET_REACT_DEFENSIVE, PET_REACT_PASSIVE,
    PET_STATE_BAR_DISABLED, PET_TALK_ATTACK, PET_TALK_ORDER, PET_TYPE_SPELL_FIRST,
    PET_TYPE_SPELL_LAST, PET_UNUSABLE_UNIT_FLAGS,
};
pub use petition::{
    petition_result, PetitionQueryResponse, PetitionRename, PetitionShowList, PetitionShowListEntry,
    PetitionShowSignatures, PetitionSignResults, PetitionSignature, CHARTER_DISPLAY_ID,
    CHARTER_ITEM_ENTRY, CHARTER_NAME_MAX_LENGTH, ITEM_FLAG_CHARTER, MAX_PETITION_SIGNATURES,
};
pub use pose::{set_sheathed, };
pub use progression::{
    ExplorationXp, LevelUpInfo, TalentWipeConfirm, XpGain,
};
pub use pvp::{InspectHonorStats, PvpCredit};
pub use quest::{
    dialog_status, quest_confirm_accept, quest_flags, quest_push_result,
    QuestComplete, QuestConfirmAccept, QuestDetails,
    QuestGiverList, QuestListEntry, QuestObjective, QuestOfferReward, QuestPushResult,
    QuestRequestItems, QuestRequiredItem, QuestRewardItem, QuestShareMsg, QuestTemplate,
    WrathQuestInfo, QUEST_EMOTE_COUNT, QUEST_OBJECTIVES_COUNT, QUEST_PVP_KILL_COUNTER,
    QUEST_REWARDS_COUNT, QUEST_REWARD_CHOICES_COUNT,
};
pub use reputation::{
    FACTION_LIST_LEN,
    WATCHED_FACTION_NONE,
};
pub use roster::{
    CharCreateReq, CharEnumItem, Character, CHARACTER_FLAG_GHOST, CHARACTER_FLAG_HIDE_CLOAK,
    CHARACTER_FLAG_HIDE_HELM, CHARACTER_FLAG_RENAME, CHAR_CREATE_NAME_IN_USE,
    CHAR_CREATE_SERVER_LIMIT, CHAR_CREATE_SUCCESS, CHAR_DELETE_SUCCESS, CLASS_WARRIOR, GENDER_MALE,
    RACE_HUMAN,
};
pub use social::{
    friend_list, friend_result, friend_status,
    who,
    FriendEntry, FriendOnline, FriendStatusUpdate, WhoEntry, WhoRequest, WhoResults,
    WHO_MAX_SEARCH_TERMS, WHO_MAX_ZONES,
};
pub use spellbook::SpellCooldown;
pub use spells::{
    cancel_aura, cast_spell, CastOutcome, CorpseTarget, SpellCastTargets,
    SpellChainTargets, SpellGo, SpellStart,
};
pub use stable::{
    list_stabled_pets, stable_result, stable_swap_pet, unstable_pet,
    StabledPet,
};
pub use summon::{summon_response, SummonRequest};
pub use tabard::{
    tabard_vendor_activate, GUILD_EMBLEM_RESULT_MESSAGES,
};
pub use taxi::{
    taxi_reply, TaxiMask,
};
pub use trade::{
    TradeItem,
    TradeStatus, TradeStatusExtended, TRADE_SLOT_COUNT, TRADE_SLOT_NONTRADED,
    TRADE_SLOT_TRADED_COUNT,
};
pub use trainer::{train_fail, trainer_list, trainer_spell_state, TrainerSpell};
pub use tutorial::{TutorialFlags};
pub use update_object::field;
pub use update_object::{
    power_display_scale, quest_slot_state, CorpseLook, CreateSpline, Field, FieldSpan,
    MovementBlock, MoverState, NpcFlags, NpcService, Object, ObjectFields, ObjectType,
    OwnerFallback, PlayerSkillSlot, QuestLogSlot, UnitAuraSlot, AURA_FLAG_CANCELABLE,
    AURA_FLAG_EFF_INDEX_MASK, FIELD_PLAYER_SKILL_INFO_1_1, PLAYER_EXPLORED_ZONES_SLOTS,
    PLAYER_QUEST_LOG_SLOTS, PLAYER_SKILL_SLOTS, UNIT_AURA_POSITIVE_SLOTS, UNIT_AURA_SLOTS,
};
pub use vendor::{
    buy_result, sell_result, VendorItem,
};
pub use world_state::InitWorldStates;

/// World `SMSG_AUTH_RESPONSE` result codes, not realmd's `AuthLogonResult` (`crate::AuthReject`):
/// the ranges overlap, and 0x0C is `AUTH_OK` here but `AUTH_LOGON_FAILED_SUSPENDED` there. Each
/// name is the `GlueStrings` key the client shows (cmangos `SharedDefines.h:1721`).
pub const AUTH_OK: u8 = 0x0C;
pub const AUTH_FAILED: u8 = 0x0D;
pub const AUTH_REJECT: u8 = 0x0E;
pub const AUTH_BAD_SERVER_PROOF: u8 = 0x0F;
pub const AUTH_UNAVAILABLE: u8 = 0x10;
pub const AUTH_SYSTEM_ERROR: u8 = 0x11;
pub const AUTH_BILLING_ERROR: u8 = 0x12;
pub const AUTH_BILLING_EXPIRED: u8 = 0x13;
pub const AUTH_VERSION_MISMATCH: u8 = 0x14;
pub const AUTH_UNKNOWN_ACCOUNT: u8 = 0x15;
pub const AUTH_INCORRECT_PASSWORD: u8 = 0x16;
pub const AUTH_SESSION_EXPIRED: u8 = 0x17;
pub const AUTH_SERVER_SHUTTING_DOWN: u8 = 0x18;
pub const AUTH_ALREADY_LOGGING_IN: u8 = 0x19;
pub const AUTH_LOGIN_SERVER_NOT_FOUND: u8 = 0x1A;
/// The realm is full and we are queued, not refused: the one code here that is not an ending.
pub const AUTH_WAIT_QUEUE: u8 = 0x1B;
// The 1.12 client's `OKAY_WITH_URL` table (`0x803740`) keys on banned, no-time, db-busy,
// suspended and parental, so the URL dialog is reachable only from this enum, never from realmd.
pub const AUTH_BANNED: u8 = 0x1C;
pub const AUTH_ALREADY_ONLINE: u8 = 0x1D;
pub const AUTH_NO_TIME: u8 = 0x1E;
pub const AUTH_DB_BUSY: u8 = 0x1F;
pub const AUTH_SUSPENDED: u8 = 0x20;
pub const AUTH_PARENTAL_CONTROL: u8 = 0x21;
/// `LogoutResult::Success` (`SMSG_LOGOUT_RESPONSE`).
pub const LOGOUT_SUCCESS: u32 = 0x0;
/// Faction-tongue `Language` ids (vmangos `SharedDefines.h:256-261`); vmangos rejects
/// `Universal` (0) from clients in ordinary chat.
pub const LANGUAGE_COMMON: u32 = 0x7;
pub const LANGUAGE_ORCISH: u32 = 0x1;

/// `LANG_ADDON` (vmangos `SharedDefines.h:270`): marks a chat line as addon data, not speech;
/// 1.12 has no addon opcode, and the client routes such a line to `CHAT_MSG_ADDON`. The server
/// skips language, flood and sanitize checks for it, never rewrites it, and allows it only on the
/// group, guild and channel lanes (`ChatHandler.cpp:49,84,172-219`). The 1.12 `SendAddonMessage`
/// (`0x49f920`) sends only PARTY, RAID, GUILD and BATTLEGROUND, and the receive side names any
/// other lane "UNKNOWN" (`0x49aff4`).
pub const LANGUAGE_ADDON: u32 = 0xFFFF_FFFF;

/// A race's faction tongue. vmangos drops a chat line, `.command`s included, in a language the
/// speaker does not know (`KnowsLanguage`, `Handlers/ChatHandler.cpp`); races 2/5/6/8 learn 669
/// Language Orcish, the rest 668 Language Common (`playercreateinfo_spell`).
pub fn faction_language(race: u8) -> u32 {
    match race {
        2 | 5 | 6 | 8 | 10 => LANGUAGE_ORCISH, // orc, undead, tauren, troll, blood elf
        _ => LANGUAGE_COMMON, // human, dwarf, night elf, gnome, draenei
    }
}
/// `ChatMsg` values (vmangos `SharedDefines.h:1191-1301`) as `CMSG_MESSAGECHAT`'s `u32` `type`
/// (`Server/Packets/Chat.h:12`). The inbound `u8` field has its own set, [`CHAT_MSG_SAY`] etc.
pub const CHAT_TYPE_SAY: u32 = 0x0;
pub const CHAT_TYPE_PARTY: u32 = 0x1;
pub const CHAT_TYPE_RAID: u32 = 0x2;
pub const CHAT_TYPE_GUILD: u32 = 0x3;
pub const CHAT_TYPE_OFFICER: u32 = 0x4;
pub const CHAT_TYPE_YELL: u32 = 0x5;
pub const CHAT_TYPE_WHISPER: u32 = 0x6;
pub const CHAT_TYPE_EMOTE: u32 = 0x8;
pub const CHAT_TYPE_CHANNEL: u32 = 0xE;
pub const CHAT_TYPE_AFK: u32 = 0x14;
pub const CHAT_TYPE_DND: u32 = 0x15;
/// `#if SUPPORTED_CLIENT_BUILD > CLIENT_BUILD_1_10_2`: active for 5875.
pub const CHAT_TYPE_RAID_LEADER: u32 = 0x57;
pub const CHAT_TYPE_RAID_WARNING: u32 = 0x58;
/// `#if SUPPORTED_CLIENT_BUILD > CLIENT_BUILD_1_11_2`: active for 5875.
pub const CHAT_TYPE_BATTLEGROUND: u32 = 0x5C;
pub const CHAT_TYPE_BATTLEGROUND_LEADER: u32 = 0x5D;
