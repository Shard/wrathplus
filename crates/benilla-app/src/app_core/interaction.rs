//! The open windows and NPC interactions the wire opens and the UI reads: a trainer, mailbox,
//! bank, reader, gossip, binder, meeting stone, spirit healer, taxi, trade skill, merchant,
//! social lists and tabard designer; their owners keep the systems and re-export the types.

use std::collections::HashMap;
use std::time::Instant;

use benilla_protocol::messages::{
    FriendEntry, FriendStatusUpdate, GossipOption, MailListEntry, MeetingStoneNotice, NpcTextBlock,
    TrainerSpell, VendorItem,
};
use bevy::prelude::*;

use crate::query_cache::QueryCache;

/// The open trainer as `SMSG_TRAINER_LIST` delivered it.
#[derive(Resource, Default)]
pub(crate) struct TrainerOpen {
    pub(crate) trainer: Option<u64>,
    /// In wire order, which is the 1-based display order.
    pub(crate) services: Vec<TrainerSpell>,
    pub(crate) trainer_type: u32,
    pub(crate) greeting: String,
    /// A list landed and the engine's filter, collapse set and selection are not yet reset, as
    /// the reference's builder resets them per list (`0x4d7b42` for the selection); an open window
    /// repaints through [`reeval`], never a second list.
    pub(crate) fresh_list: bool,
    /// One of the reference's twelve re-evaluation triggers fired with the window open; the feed
    /// re-derives and fires `TRAINER_UPDATE` even if no state moved, as `0x4d7d40`'s tail does
    /// (`0x4d83f4`), and the stock window re-reads `GetMoney()` on it.
    pub(crate) re_derive: bool,
}

/// The `Stationery.dbc` catalog; without it every mail gets the default backdrop.
#[derive(Resource)]
pub(crate) struct Stationery(pub(crate) benilla_formats::StationeryCatalog);

/// `Package.dbc`'s rows, in file order; without them `GetNumPackages()` answers 0.
#[derive(Resource)]
pub(crate) struct Packages(pub(crate) Vec<benilla_formats::PackageRow>);

/// A `SEND` result queued for [`feed_mail`]. `MAIL_FAILED` fires on every one, success included:
/// the reference uses it as "the send resolved" (`0x4ad15f`), and the stock handler only
/// re-enables the button.
pub(crate) struct MailSendAck {
    pub(crate) ok: bool,
    /// The message key of a failure other than an equip error.
    pub(crate) refusal: Option<&'static str>,
}

/// The open mailbox session: the guid, the inbox rows as the wire sent them, the letter-body cache
/// and the result queues. Cleared on close and on disconnect.
#[derive(Resource, Default)]
pub(crate) struct MailOpen {
    pub(crate) mailbox: Option<u64>,
    /// Wire order, which is the 1-based display order.
    pub(crate) mails: Vec<MailListEntry>,
    /// Native inbox retained without projecting twelve attachments into one Vanilla slot.
    pub(crate) wrath_inbox: Option<benilla_protocol::wrath_mail::Inbox>,
    /// Letter bodies by `item_text_id`, asked once.
    pub(crate) bodies: QueryCache<u32, String>,
    /// `MAIL_SHOW` owed: every mailbox use re-shows the window (`0x4acd10`).
    pub(crate) show_requested: bool,
    /// When the last `CMSG_GET_MAIL_LIST` went out, in `Time::elapsed_secs_f64`.
    pub(crate) last_list_query: Option<f64>,
    pub(crate) send_acks: Vec<MailSendAck>,
    /// 1-based rows a take emptied and purged, each owed a `CLOSE_INBOX_ITEM`.
    pub(crate) close_inbox: Vec<u32>,
    /// Take, return and delete refusals, as message keys.
    pub(crate) errors: Vec<&'static str>,
}

/// The vault the player opened, set by the object click and cleared by the frame's close.
#[derive(Resource, Default)]
pub(crate) struct GuildBankOpen {
    pub(crate) banker: Option<u64>,
    /// The click is waiting for this feed to send the activate.
    pub(crate) pending: bool,
}

/// Build 12340's letter texts, by item GUID: `CMSG_ITEM_TEXT_QUERY` names the item and
/// `SMSG_ITEM_TEXT_QUERY_RESPONSE` answers with it (AC `HandleItemTextQuery`); a `None` answer is
/// an item the server has no text for.
#[derive(Resource, Default)]
pub(crate) struct LetterTexts {
    pub(crate) texts: QueryCache<u64, String>,
}

/// Which of the reference's two text sources a session shows; the object decides, not the click.
pub(crate) enum ReadSource {
    /// A mail-made letter: one body, asked by the letter's GUID, and a creator line.
    Letter,
    /// A page chain. `visited` runs from the first page to the one shown, as pages link only
    /// forwards (the reference's array at `0xbc3fd0`); it is empty until the object's template
    /// answers, since the reference asks for the page id at paint time (`0x5f59d0`).
    Pages { visited: Vec<u32> },
}

pub(crate) struct ReadSession {
    /// An item's or a GameObject's guid; reading the same object again closes the reader.
    pub(crate) object_guid: u64,
    pub(crate) source: ReadSource,
    /// Keyed on the VM, so a `/reload` re-fires `ITEM_TEXT_BEGIN` for a reader still open.
    pub(crate) told: crate::ui_script::VmMemo<ItemTextTold>,
}

#[derive(Default)]
pub(crate) struct ItemTextTold {
    /// `ITEM_TEXT_BEGIN` fired: once per open, before the text lands.
    pub(crate) begun: bool,
    pub(crate) ready: bool,
}

#[derive(Resource, Default)]
pub(crate) struct ItemTextOpen {
    pub(crate) pending: Option<ReadSession>,
    /// A toggle closed the reader and the frame is not told yet; the feed holds the script.
    pub(crate) closing: bool,
}

/// `PageTextMaterial.dbc`; absent without client data, leaving every reader on Parchment.
#[derive(Resource)]
pub(crate) struct PageMaterials(pub(crate) benilla_formats::PageTextMaterialCatalog);

/// The open gossip menu, plus the NPC-text records by id, which outlive it. The cache holds whole
/// records: the reference draws the greeting afresh at each open, from the NPC's gender and a roll.
#[derive(Resource, Default)]
pub(crate) struct GossipState {
    pub(crate) npc: Option<u64>,
    pub(crate) text_id: u32,
    /// This open's drawn greeting; `None` while its text query is out.
    pub(crate) greeting: Option<String>,
    pub(crate) options: Vec<GossipOption>,
    /// The packet's quest rows, `(quest_id, dialog-status icon, level, title)`. The level is kept
    /// because `GetGossipAvailableQuests` and `GetGossipActiveQuests` return `(title, level)`
    /// pairs, walked in twos by stock `GossipFrame.lua` (lines 66 and 90).
    pub(crate) quests: Vec<(u32, u32, u32, String)>,
    pub(crate) quest_info: Vec<benilla_protocol::messages::WrathQuestInfo>,
    pub(crate) records: HashMap<u32, Vec<NpcTextBlock>>,
}

/// The innkeeper's pending bind question; everything the dialog reads rides the event's argument.
#[derive(Resource, Default)]
pub(crate) struct BinderState {
    /// The innkeeper who asked, sent back on the wire; vmangos binds only a live one in range.
    pub(crate) npc: Option<u64>,
    /// A dialog the feed still owes, set per packet: asking again after a decline is a second
    /// dialog, which a state diff would swallow.
    pub(crate) ask: bool,
}

/// The current-area spirit healer (`[0xb4e330/334]`) and its wave clock (`[0xb4e338]`).
#[derive(Resource, Default)]
pub(crate) struct AreaSpiritHealer {
    /// Written only by [`Self::set_healer`], the reference's `0x4921c0`.
    pub(crate) healer: Option<u64>,
    pub(crate) deadline: Option<Instant>,
    pub(crate) in_range: bool,
}

/// The status text (`[0xb7203c]`): none until the server's `0x295` lands and after world leave,
/// then a built line; localized at push time.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) enum StoneText {
    #[default]
    None,
    Built(String),
}

/// The meeting-stone queue: the two globals and what the wire still owes the screen.
#[derive(Resource, Default)]
pub(crate) struct MeetingStone {
    /// `[0xb72038]`, the queued area id, 0 for none.
    pub(crate) area: u32,
    pub(crate) text: StoneText,
    /// `0x295` arrivals as `(previous area, status)`; the handler stores the new area before it
    /// switches, so that one is already in `area`.
    pub(crate) updates: Vec<(u32, u8)>,
    pub(crate) notices: Vec<MeetingStoneNotice>,
    /// `0x299` guids whose name has not resolved yet.
    pub(crate) pending_members: Vec<u64>,
    /// The VM's copy of the two globals is stale.
    pub(crate) dirty: bool,
}

/// A right-click on a `GAMEOBJECT_TYPE_MEETINGSTONE` (23) that passed the shared click gates.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MeetingStoneUse {
    pub(crate) go_guid: u64,
}

/// A flight master's `SMSG_TAXINODE_STATUS`: `known = false` shows the green `TalkToMeGreen`
/// overhead icon (`0x5ecdd0` to `0x607480`, resource table `0xc4d9d8` index 4). The query and the
/// teardown belong to [`crate::quest_markers::query`]: the reference's only sender, `0x5eb170`, is
/// called only from `0x607380` (at `0x6073e8`), the per-unit function that also sends the
/// questgiver query, and both icons share one marker slot (`unit+0xb2c`).
#[derive(Component, Clone, Copy)]
pub(crate) struct FlightMasterStatus {
    pub(crate) known: bool,
}

/// Opener spell ids (effect 47) that `send_spell_cast` intercepted, opened by
/// [`open_trade_skill`] the same frame.
#[derive(Resource, Default)]
pub(crate) struct TradeSkillOpens(pub(crate) Vec<u32>);

/// The open vendor and its `SMSG_LIST_INVENTORY` rows, until a close or a disconnect.
#[derive(Resource, Default)]
pub(crate) struct MerchantOpen {
    pub(crate) vendor: Option<u64>,
    /// In wire order, which is the 1-based display order.
    pub(crate) items: Vec<VendorItem>,
}

/// The social session mirror: filled by the social handlers, read by the feed, cleared on
/// disconnect.
#[derive(Resource, Default)]
pub(crate) struct SocialState {
    /// The friend list in wire order (vmangos walks a guid-keyed map); the feed sorts for display.
    pub(crate) friends: Vec<FriendEntry>,
    /// Build 12340's contact notes by friend guid (`GetFriendInfo`'s seventh return).
    pub(crate) notes: std::collections::HashMap<u64, String>,
    pub(crate) ignores: Vec<u64>,
    /// The selected friend as a guid, like the reference's `+0x648`; 0 is none.
    pub(crate) selected_friend: u64,
    /// The selected ignore as a guid (the reference's `+0x720`).
    pub(crate) selected_ignore: u64,
    /// The friend guids in the order the feed last showed them, for Lua's row indices.
    pub(crate) display_order: Vec<u64>,
    pub(crate) ignore_display_order: Vec<u64>,
    /// The last `/who` answer; `who_total` is the server's full match count.
    pub(crate) who: Vec<benilla_protocol::messages::WhoEntry>,
    pub(crate) who_total: u32,
    /// Native routing compares the uncapped first wire count (006b8720).
    pub(crate) native_who_count: Option<usize>,
    pub(crate) pending_native_who:
        std::collections::VecDeque<(u32, Vec<benilla_protocol::wrath_who::Entry>)>,
    /// Native rows retained losslessly; shared display rows intentionally omit gender.
    pub(crate) native_who_rows: Vec<benilla_protocol::wrath_who::Entry>,
    /// The `/who` sort chain (`SortWho`'s seven `{key, dir}` slots at `0xc2817c`), pushed with the
    /// rows so the binding can re-sort synchronously.
    pub(crate) who_sort: benilla_ui::script::WhoSortChain,
    /// `SetWhoToUI`, which the Who frame sets on show and clears on hide.
    pub(crate) who_to_ui: bool,
    /// Results whose line waits on a name query (the reference formats from the name cache).
    pub(crate) pending_lines: Vec<FriendStatusUpdate>,
    /// Set when the friend list changed, so the feed fires its list event.
    pub(crate) friends_dirty: bool,
    /// A `ShowFriends()` is out: the next list fires `FRIENDLIST_SHOW`, not `FRIENDLIST_UPDATE`.
    pub(crate) friends_show_pending: bool,
    pub(crate) ignores_dirty: bool,
    pub(crate) who_dirty: bool,
}

/// The design our own body wears while the frame is open.
#[derive(Resource, Default)]
pub(crate) struct TabardDesign(pub(crate) Option<[i32; 5]>);

/// `UNIT_NPC_FLAGS` bit 6, `SPIRITGUIDE`, which the acquire callback `0x4924c0` tests (`0x4924fc`).
/// The area spirit healer's aura, cancelled by `0x4921c0` and `CancelAreaSpiritHeal`; `0x6e7040`
/// fires `AREA_SPIRIT_HEALER_OUT_OF_RANGE` for this spell id alone (`0x6e70b6`).
pub(crate) const AREA_SPIRIT_HEALER_AURA: u32 = 2584;
