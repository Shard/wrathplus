//! Game state that world, net and UI code all read: the spell catalogs and learned latches, the
//! client lifecycle and roster, the open NPC, loot gates and UI pointer flags. Each owner keeps
//! its systems and impls and re-exports the type from its old path.

use std::collections::HashMap;

use benilla_formats::SpellCatalog;
use benilla_protocol::messages::{
    QuestDetails, QuestGiverList, QuestOfferReward, QuestRequestItems,
};
use benilla_protocol::Character;
use bevy::prelude::*;

use super::errors::UiError;

/// The spell DBC catalogs, absent without client data; the cast-visual router shares the catalog.
#[derive(Resource)]
pub(crate) struct Spells {
    pub(crate) catalog: SpellCatalog,
    /// Form id to its `SpellShapeshiftForm.dbc` row: `BonusActionBar` pages the bar
    /// (`GetBonusBarOffset 0x4e7620`), `flags1` holds the stance and cancel bits.
    pub(crate) forms: HashMap<u32, benilla_formats::ShapeshiftForm>,
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

/// The wire packet that opened the current panel.
pub(crate) enum QuestView {
    Greeting(QuestGiverList),
    Detail(QuestDetails),
    Progress(QuestRequestItems),
    Reward(QuestOfferReward),
}

/// The open questgiver window; the per-guid dialog statuses survive a close.
#[derive(Resource, Default)]
pub(crate) struct QuestGiver {
    pub(crate) completed_fanfare: bool,
    /// The panel's source: an NPC, a GameObject, a quest-starter item or a sharing player.
    pub(crate) npc: Option<u64>,
    pub(crate) view: Option<QuestView>,
    /// The reference's latch `0xbe0824`, written by DETAILS and OFFER_REWARD (`0x500ef0`) and by
    /// REQUEST_ITEMS' `closeOnCancel` (`0x501070`), and zeroed only on world enter (`0x500af0`).
    /// Non-zero turns a plain unit's decline into the silent teardown
    /// ([`decline_leg_native`]). vmangos sends 1 on every DETAILS and OFFER_REWARD; REQUEST_ITEMS
    /// carries 1 on a single-quest auto-open and 0 on `COMPLETE_QUEST`
    /// (`QuestHandler.cpp:390-395`).
    pub(crate) close_on_cancel: u32,
    /// The reference's already-acted latch `0xbe0844`, set by `AcceptQuest` and by a re-opening
    /// `DeclineQuest`, which keeps the panel up; both refuse while it is set (`0x5013a8`,
    /// `0x5013fe`) until a panel publish clears it ([`Self::open`]).
    pub(crate) acted: bool,
    /// The reference's `0xbe0828`: the chosen reward's item id, latched by `GetQuestReward`
    /// (`0x50161a`) for the turn-in's item line (`0x5dc676`-`0x5dc6c6`), and zeroed by a panel
    /// publish (`0x500d8c`), world enter (`0x500b15`) and the turn-in.
    pub(crate) chosen_reward: u32,
    /// Per-guid dialog status from `SMSG_QUESTGIVER_STATUS`, the `!`/`?` marker's value.
    pub(crate) statuses: HashMap<u64, u32>,
    pub(crate) wrath_statuses: HashMap<u64, u32>,
    /// Messages queued for [`feed_quest`], each a GlobalStrings key and its fills.
    pub(crate) messages: Vec<UiError>,
    pub(crate) reask: u32,
    /// The dialog statuses dropped at the reference's typemask-8 lookup (GameObject answers, which
    /// vmangos sends), per guid, so a live probe can tell a drop from a query never sent.
    pub(crate) refused: HashMap<u64, u32>,
    pub(crate) refused_count: u32,
}

/// This frame's binding activity, which engine-side consumers read instead of raw keys: the
/// [`Input`]s the running bodies' Lua calls set.
#[derive(Resource, Default)]
pub(crate) struct BindingsState {
    /// Live latches, (base input, command): a press not yet released, whose release runs the
    /// command's `runOnUp` half. The reference replays the press-time chord at key-up (`0x483bd0`);
    /// latching the resolved command is equivalent.
    pub(crate) latched: Vec<(Held, String)>,
    /// The held bits a Start set and no Stop has cleared.
    pub(crate) held: Vec<Input>,
    /// Inputs whose Start, or whose one-shot, came this frame (the press edge).
    pub(crate) just: Vec<Input>,
    /// One-shots fired this frame.
    pub(crate) fired: Vec<Input>,
    /// Amount per one-shot this frame: a zoom's yards, else 1.0.
    pub(crate) amounts: Vec<(Input, f32)>,
    /// The keys we believe are down: a press of one is a repeat. The reference classifies
    /// auto-repeat off its own list (`0x4248b3`), never the Win32 `lParam` repeat bit.
    /// Normalized like [`latched`](Self::latched) and reconciled against `ButtonInput` every
    /// pass, so a window deactivate empties it and a key held across an alt-tab resumes, as in
    /// the reference. Keyboard only: mouse edges cannot repeat.
    pub(crate) down: Vec<KeyCode>,
}

/// The physical input a latch waits on: a keyboard key by its code, not its name, so its release
/// ends the latch whatever the layout names it by then.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Held {
    Key(KeyCode),
    Mouse(MouseButton),
    Wheel,
}

/// One engine input a binding function drives: a held bit of `[InputControl+4]` or a one-shot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Input {
    /// `MoveForwardStart`/`Stop`, bit `0x10`.
    MoveForward,
    /// `MoveBackwardStart`/`Stop`, bit `0x20`.
    MoveBackward,
    /// `TurnLeftStart`/`Stop`, bit `0x100`.
    TurnLeft,
    /// `TurnRightStart`/`Stop`, bit `0x200`.
    TurnRight,
    /// `StrafeLeftStart`/`Stop`, bit `0x40`.
    StrafeLeft,
    /// `StrafeRightStart`/`Stop`, bit `0x80`.
    StrafeRight,
    /// `TurnOrActionStart`/`Stop`, bit `0x1`.
    TurnOrAction,
    /// `CameraOrSelectOrMoveStart`/`Stop`, bit `0x2`.
    CameraOrSelectOrMove,
    /// `PitchUpStart`/`Stop`, bit `0x400`: the swimmer's or flier's pitch keys.
    PitchUp,
    /// `PitchDownStart`/`Stop`, bit `0x800`.
    PitchDown,
    /// `JumpOrAscendStart`/`AscendStop`: the flier's ascend key.
    Ascend,
    /// `SitStandOrDescendStart`/`DescendStop`: the flier's descend key.
    Descend,
    /// `Jump()`.
    Jump,
    /// `ToggleRun()`, the walk toggle.
    ToggleRun,
    /// `ToggleAutoRun()`.
    ToggleAutoRun,
    /// `SitOrStand()`.
    SitOrStand,
    /// `ToggleSheath()`.
    ToggleSheath,
    /// Native ATTACKTARGET binding while the Wrath stock binding VM is not yet qualified.
    AttackTarget,
    /// Native TARGETNEARESTENEMY / reverse direction, using the ordinary target cycler.
    TargetNearestEnemy,
    TargetPreviousEnemy,
    /// `CameraZoomIn([yards])`, its yards in [`BindingsState::amount`].
    CameraZoomIn,
    /// `CameraZoomOut([yards])`, its yards in [`BindingsState::amount`].
    CameraZoomOut,
}
