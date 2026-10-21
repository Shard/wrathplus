//! The UI runtime's shared flags and clocks: inspect mode, UI scale, keyboard capture, the
//! pointer's click and hover, the glue canvas, the realm list and the login intent; their owners
//! keep the systems and re-export the types.

use benilla_formats::RealmCategory;
use benilla_protocol::RealmInfo;
use bevy::prelude::*;

/// Whether the dev `I` inspector's world picking is armed; `player::control` and `target::click`
/// read it every frame, so it lives here, and the default (disarmed) is the player's.
#[derive(Resource, Default)]
pub(crate) struct InspectMode {
    pub(crate) enabled: bool,
}

/// The mouse-enabled player-UI frame under the cursor; world pick and camera look yield to it.
#[derive(Resource, Default)]
pub(crate) struct PlayerUiHover(pub(crate) Option<u32>);

/// Who owns this frame's keys (the client's `DAT_00cf4dc8 != 0` gate), written in [`UiInput`] and
/// read after it: a focused EditBox takes every key, a shown keyboard-enabled frame only the key
/// it ate. Neither releases a held key: only the window deactivate (`0x514490`, from `0x493058`)
/// and the world-enter cascade (`0x5144c0`) clear the direction bits, so W keeps running while
/// you type.
#[derive(Resource, Default)]
pub(crate) struct UiKeyboardCapture {
    /// A focused EditBox eats every key (`0x77b35e` returns 1 on every path but alt-arrow).
    pub(crate) typing: bool,
    /// Keys a shown keyboard-enabled frame consumed this frame (the existence gate, `0x76b7d0`),
    /// per key so the map eating `M` suppresses no other binding; raw codes, as read.
    pub(crate) consumed: Vec<bevy::input::keyboard::KeyCode>,
    /// The arrows fall through: the focused box is in alt-arrow mode and ALT is up, so the
    /// reference declines them at `0x77b1c4` and `CGWorldFrame` runs their bindings.
    pub(crate) arrows_fall_through: bool,
}

/// A left press the UI consumed without hitting a frame (a payload dropped on empty world), which
/// world click-pick and orbit-start yield to. Over a world object the reference keeps an item
/// payload and runs SELECT instead.
#[derive(Resource, Default)]
pub(crate) struct PlayerUiClickConsumed(pub(crate) bool);

/// The frame's atomic (`Instant`, `GetTime`) pair, the one base for mapping a store `Instant` onto
/// `GetTime`: `anchor`'s steps are the VM clock's deltas, so a mapped start is stable frame to
/// frame. Neither leg restarts with the VM: the reference's `GetTime` is `GetTickCount`, and stock
/// `Cooldown.lua:3` gates on `start > 0`, so a reset would hide every running cooldown's sweep.
#[derive(Resource)]
pub(crate) struct UiClock {
    /// `Time<Real>::last_update()` at the tick that produced [`Self::ui_now`].
    pub(crate) anchor: std::time::Instant,
    /// The VM clock after that tick, in seconds since this process started.
    pub(crate) ui_now: f64,
}

/// The reference's `uiScale` CVar: the VM's screen is `768/uiScale` units tall, so
/// `uiScale = 768/screenH` is pixel-perfect. Tests pin the `Default` 1.0.
#[derive(Resource)]
pub(crate) struct UiScaleCvar(pub(crate) f32);

/// The world-entry UI load still owed: armed at `OnEnter(InWorld)`, run by
/// [`run_pending_entry_load`] once the loading cover is on screen. `OnEnter(InWorld)` runs on the
/// frame that first presents the cover, so a load there would freeze the character screen for its
/// whole duration. The wait is [`crate::loading_screen::EntryCover`]'s, and the loading
/// screen does not clear while this exists, so the reveal never precedes the UI.
#[derive(Resource, Default)]
pub(crate) struct PendingEntryUiLoad;

/// The rect a glue screen's chrome lays out into: the pillarboxed scene's box, not the window, as
/// a reference client of the box's aspect would lay it out.
///
/// The screen's root stays full-window: the bars are the booth camera's clear inside a
/// window-sized target, which the full-bleed scene pane must keep covering.
#[derive(Component)]
pub(crate) struct GlueCanvas;

/// The realm list, the selection on it, and the policy's memory.
#[derive(Resource, Default)]
pub(crate) struct Realms {
    /// Every realm the auth server advertised, in wire order.
    pub(crate) realms: Vec<RealmInfo>,
    /// The highlighted row by realm name, so a refresh that adds or drops a realm cannot move it.
    pub(crate) selected: Option<String>,
    /// The client Region's categories ([`category`]), read once at startup; `None` until then,
    /// and every realm lists untabbed.
    pub(crate) categories: Option<Vec<RealmCategory>>,
    /// The tab in front, `RealmList.selectedCategory`: a 1-based ordinal over the categories
    /// holding a realm.
    pub(crate) category: Option<usize>,
    /// `[0x837e98]`, the stored category `GetSelectedCategory` opens the list on, by index into
    /// [`Self::categories`]; `None` is its `-1` load value.
    pub(crate) chosen_category: Option<usize>,
    /// First visible row within the selected category (`RealmList.offset`).
    pub(crate) offset: usize,
    /// Which column the list is sorted on, and which way.
    pub(crate) sort: Sort,
    /// `RealmList:IsVisible()`. While set, a published list is a refresh and is never
    /// auto-answered.
    pub(crate) shown: bool,
    /// `GetRealmInfo`'s `currentRealm`, the row highlighted when nothing else is. The reference
    /// case-folds the `realmName` CVar (`0x46ef9e`, via `0x5ab7d0` on the handle from `0x63db90`)
    /// against the realm name, and `ConnectToRealm` (`0x46b217`) writes the CVar before it dials.
    /// Ours is set when we answer a park and seeded from the persisted CVar, because benilla
    /// writes `realmName` only at world entry (the VM's `set_realm_name`).
    pub(crate) current: Option<String>,
    /// Seconds until the next refresh request.
    pub(crate) refresh_in: f32,
    /// `WOW_REALM`, read once.
    pub(crate) env_realm: Option<String>,
    pub(crate) env_read: bool,
}

impl Realms {}

impl Realms {}

impl Realms {
    /// `RealmList.selectedCategory`: nil until the first `RealmListUpdate` makes it 1.
    pub(crate) fn category(&self) -> usize {
        self.category.unwrap_or(1)
    }

    /// The rebuild's tail (`0x46e6f0`): the category holding the realm this session names becomes
    /// the stored one; with no such realm, or one on no tab, it is left as it was.
    pub(crate) fn store_current_category(&mut self) {
        let (Some(categories), Some(want)) = (&self.categories, &self.current) else {
            return;
        };
        // The walk visits every realm, so the last one of the name decides.
        let Some(realm) = self
            .realms
            .iter()
            .rfind(|r| r.name.eq_ignore_ascii_case(want))
        else {
            return;
        };
        let byte = match realm.category {
            0 => match categories.first() {
                Some(first) => first.id,
                None => return,
            },
            b => u32::from(b),
        };
        if let Some(at) = categories.iter().position(|c| c.id == byte) {
            self.chosen_category = Some(at);
        }
    }
}

impl Realms {
    /// The highlighted realm, if it is still on the list.
    pub(crate) fn selected(&self) -> Option<&RealmInfo> {
        let name = self.selected.as_deref()?;
        self.realms.iter().find(|r| r.name == name)
    }

    /// Highlight a realm by name.
    pub(crate) fn select(&mut self, name: &str) {
        self.selected = Some(name.to_string());
    }

    /// `RealmList:Hide()`, and nothing else.
    pub(crate) fn hide(&mut self) {
        self.shown = false;
    }
}

/// One of the four sort columns, `SortRealms`' string argument.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SortKey {
    /// `SortRealms("characters")`, comparator case 0: the account's character count.
    Characters,
    /// `SortRealms("load")`, case 1: the computed load band, not the raw population.
    Load,
    /// `SortRealms("name")`, case 2: a case-folding compare.
    Name,
    /// `SortRealms("mode")`, case 3: the `(pvp, rp)` pair.
    Type,
}

/// The sort config (`0xb41f40`): four `{key, descending}` records. The comparator (`0x46e790`)
/// walks them in order and the first that does not tie decides, negated by its own `descending`.
/// The default, `[characters, load, name, mode]` all ascending, is written once per process
/// (`0x46e430`), so a click persists for the session. Ascending characters is higher count first.
#[derive(Clone, Copy)]
pub(crate) struct Sort(pub(crate) [(SortKey, bool); 4]);

impl Default for Sort {
    fn default() -> Self {
        Self([
            (SortKey::Characters, false),
            (SortKey::Load, false),
            (SortKey::Name, false),
            (SortKey::Type, false),
        ])
    }
}

impl Sort {
    /// A column header click (`0x46e9b0`): the front key toggles its direction; any other moves
    /// to the front keeping its direction, and the records it passes shift down.
    pub(crate) fn click(&mut self, key: SortKey) {
        if self.0[0].0 == key {
            self.0[0].1 = !self.0[0].1;
            return;
        }
        let Some(at) = self.0.iter().position(|(k, _)| *k == key) else {
            return;
        };
        let record = self.0[at];
        self.0.copy_within(0..at, 1);
        self.0[0] = record;
    }
}

/// Where the IO thread's read loop is, as far as the app can tell; credentials are submitted
/// only while it is parked pre-logon.
#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub(crate) enum IoPark {
    /// Parked pre-logon (boot, a failure, a disconnect, a Back).
    #[default]
    AtLogin,
    /// Past logon: at select or in the world.
    Active,
}

/// The credential policy's memory: the session's credentials, the in-flight and park state, and
/// the resubmit timer.
#[derive(Resource, Default)]
pub(crate) struct LoginIntent {
    /// Kept in-world so the logout relist and an unattended reconnect re-authenticate silently;
    /// cleared by select's Back, a refusal, a Cancel and a lost session.
    pub(crate) creds: Option<(String, String)>,
    /// Between our send and its LoginFailed or CharacterList answer.
    pub(crate) in_flight: bool,
    /// The in-flight submit came from the screen, which shows its connecting dialog and failure.
    pub(crate) announced: bool,
    pub(crate) park: IoPark,
    /// `Time::elapsed_secs` deadline for the next silent resubmit.
    pub(crate) retry_at: Option<f32>,
    /// The env fast path is read once, on the first policy run.
    pub(crate) env_read: bool,
}

impl LoginIntent {
    /// Forget the session's credentials and any scheduled retry.
    pub(crate) fn clear(&mut self) {
        self.creds = None;
        self.retry_at = None;
    }

    /// The account this session authenticated as, from the env fast path or the screen; it
    /// decides whether the probe shield applies.
    pub(crate) fn account(&self) -> Option<&str> {
        self.creds.as_ref().map(|(user, _)| user.as_str())
    }
}
