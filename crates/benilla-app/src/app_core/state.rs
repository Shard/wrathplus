//! Game state that world, net and UI code all read: the spell catalogs and learned latches, the
//! client lifecycle and roster, the open NPC, loot gates and UI pointer flags. Each owner keeps
//! its systems and impls and re-exports the type from its old path.

use benilla_formats::SpellCatalog;
use benilla_protocol::Character;
use bevy::prelude::*;

/// The spell DBC catalogs, absent without client data; the cast-visual router shares the catalog.
#[derive(Resource)]
pub(crate) struct Spells {
    pub(crate) catalog: SpellCatalog,
    /// Form id to its `SpellShapeshiftForm.dbc` row: `BonusActionBar` pages the bar
    /// (`GetBonusBarOffset 0x4e7620`), `flags1` holds the stance and cancel bits.
    pub(crate) forms: std::collections::HashMap<u32, benilla_formats::ShapeshiftForm>,
    /// `SpellRange.dbc`, the `GetMinMaxRange 0x6e3480` inputs; empty on a failed load.
    pub(crate) ranges: benilla_formats::SpellRangeCatalog,
    /// `SpellCastTimes.dbc`, read through `CastingTimeIndex` (`GetCastTime 0x6e3340`).
    pub(crate) cast_times: benilla_formats::SpellCastTimeCatalog,
    /// `SpellDuration.dbc`: the `$d`/`$o` tokens' source (`GetDuration 0x6ea000`).
    pub(crate) durations: benilla_formats::SpellDurationCatalog,
    /// `SpellRadius.dbc`: the `$a` token's yards.
    pub(crate) radii: benilla_formats::SpellRadiusCatalog,
}

/// The reference's learned-ability latches: at learn time `0x4b25e0` stores a spell's id by its
/// `Effect[0]`, and the unlearn path `0x4b2c50` clears it. The skin cursor requires the latch
/// (`0x482589`), so a player who never learned Skinning gets no skin cursor.
#[derive(Resource, Default)]
pub(crate) struct LearnedAbilities {
    /// `[0xb700e4]`: the known `SPELL_EFFECT_SKINNING` spell, which the skin click also casts.
    pub(crate) skinning: Option<u32>,
    /// `[0xb700e8]`: the known `SPELL_EFFECT_SKIN_PLAYER_CORPSE` spell; no cursor reads it yet.
    pub(crate) skin_player_corpse: Option<u32>,
    /// `[0xcecad8]`: the known `SPELL_EFFECT_FEED_PET` spell, latched by `0x6ea1d0` (from
    /// `0x5e9e49`); gate 3 of `ui_action::drop_item::drop_item_on_unit` and the spell it casts.
    pub(crate) feed_pet: Option<u32>,
}

/// The app's lifecycle: which screen owns the session.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum ClientState {
    /// Parked pre-logon at the login screen, the IO thread waiting for credentials. The realm list
    /// is no state: `GlueParent.lua`'s `GlueScreenInfo` omits it, a `DIALOG` frame over any screen
    /// ([`crate::realm_select::Realms::shown`]).
    #[default]
    Login,
    /// Parked at character select, the IO thread waiting for a pick.
    CharSelect,
    /// The character-creation screen, still parked at select (the IO thread serves create and
    /// delete in place).
    CharCreate,
    /// A character is in (or entering) the world.
    InWorld,
}

/// The account roster and the pick policy's memory.
#[derive(Resource, Default)]
pub(crate) struct Roster {
    pub(crate) chars: Vec<Character>,
    /// The selected row, `CharacterSelect.selectedIndex` 0-based. Written only through
    /// [`Roster::select`].
    pub(crate) selected: Option<usize>,
    /// Bumped by every [`Roster::select`], changed row or not: the reference's `SelectCharacter`
    /// zeroes the facing unconditionally (`0x472950` writes `0xb4217c` above its already-built
    /// check), so a roster refresh re-selecting the same index re-squares too. Only a row click is
    /// gated, in the stock Lua ([`Roster::click_row`]).
    pub(crate) select_seq: u64,
    /// The guid picked; kept in-world so a reconnect's roster is answered with it, cleared by a
    /// logout so the roster is shown.
    pub(crate) pending_pick: Option<u64>,
    /// `WOW_CHAR`: auto-pick this name on the first roster only.
    pub(crate) env_char: Option<String>,
    /// A just-created character's name, whose row gets selected (the reference's
    /// `SELECT_LAST_CHARACTER`, keyed by name to survive the create/enum race).
    pub(crate) just_created: Option<String>,
    /// The realm-list entry this session connected to, refreshed with each roster.
    pub(crate) realm: Option<benilla_protocol::RealmInfo>,
    pub(crate) env_read: bool,
}

/// The `"npc"` unit: the open `ui_session::NpcSession`'s world entity and guid, read by the portrait booth
/// as it reads [`crate::target::Selection`] for `"target"`. Only one session is open in play.
#[derive(Resource, Default, PartialEq, Eq)]
pub(crate) struct InteractNpc(pub(crate) Option<Entity>, pub(crate) Option<u64>);

/// The loot CVars. `autoLootDefault` is not a 1.12 CVar: 1.12 auto-loots only on a shift-click.
/// [`feed_loot`] sweeps the rows engine-side at the open, as the reference does, and a held Shift
/// inverts the setting. `showLootSpam` is 1.12's Detailed Loot Information checkbox (`0xb4e2bc`,
/// registered at `0x48fd1c`, default `"1"`), read only by the loot-roll line composers.
#[derive(Resource)]
pub(crate) struct LootConfig {
    pub(crate) auto_loot: bool,
    pub(crate) show_loot_spam: bool,
}

/// The loot-target latch, the reference's `[player+0x1d28]` guid: a loot session is open on this
/// object; kneeling is [`LootKneel`]'s question. Armed at the `CMSG_LOOT` send
/// (`0x5df253`/`0x5df40d`), at `SMSG_SPELL_GO` for an `OPEN_LOCK` on a chest (`0x6e831b`), by an
/// admitted response (`0x5eb900`), and at the `CMSG_OPEN_ITEM` send (`0x5edcc0`), which is what
/// admits vmangos' type-1 answer for the item.
///
/// Deviation: our close paths clear it guid-matched, where `CloseInteraction` clears it
/// unconditionally (`0x48f2c9`), so closing an old window keeps a newer `CMSG_LOOT`'s latch.
#[derive(Resource, Default)]
pub(crate) struct LootLatch(pub(crate) Option<u64>);

/// Whether the pointer is over any UI (the egui dev overlay or a player-UI frame), combined by
/// [`arbitrate_pointer_over_ui`]; gameplay reads it, so it is not the dev plugin's.
#[derive(Resource, Default)]
pub(crate) struct PointerOverUi(pub(crate) bool);

/// Whether the cursor carries a payload, a `Send` mirror. A click on sky with one held never
/// deselects: the reference gates its nothing-leg `SetSelection(0,0)` on no payload (`0x492d30`).
#[derive(Resource, Default)]
pub(crate) struct CursorPayloadHeld(pub(crate) bool);
