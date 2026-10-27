//! The item layer: item objects in the guid index and the item template cache.
//!
//! An item is `Guid` + [`ObjectStore`] + [`ItemObject`] in the guid index; item fields are private,
//! so only our own arrive. Templates are keyed by entry, asked once, and survive disconnect.

use std::time::{Duration, Instant};

use bevy::prelude::*;

use wrathplus_protocol::{ItemInfo, ObjectFields};

use bevy::ecs::system::SystemParam;

use crate::app_core::session::ObjectStore;
use crate::net::{ClientCommand, Guid, GuidIndex, NetCommands, Objects};
use crate::query_cache::QueryCache;

/// An item or container entity's kind (`TYPEMASK_ITEM` / `TYPEMASK_CONTAINER`), with no model or
/// pose. Unit readers filter it out (`Without<ItemObject>`): an item block's dwords overlap the
/// unit block's indices.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemObject {
    /// `TYPEMASK_CONTAINER`: a bag's slot array lives past the item block.
    pub container: bool,
}

/// Spawn an item object into the index. The seed is never merged, so the create raises no field
/// notify; a re-create of a live guid takes the handler's values path instead.
pub fn spawn_item(
    commands: &mut Commands,
    index: &mut GuidIndex,
    guid: u64,
    fields: ObjectFields,
    container: bool,
) -> Entity {
    let e = commands
        .spawn((
            Guid(guid),
            ObjectStore(fields),
            ItemObject { container },
            Countdowns::default(),
        ))
        .id();
    index.0.insert(guid, e);
    e
}

/// `ITEM_FIELD_ENCHANTMENT`'s 21 dwords, three per slot; the reference's seven deadline cells at
/// `obj + 0x324 + slot*4` (`0x5d9d00`) end where the next member begins, at `+0x340`.
pub const ENCHANT_SLOTS: usize = 7;

/// An item's countdowns, the reference's deadline cells on `CGItem_C`: its lifetime at `+0x320`
/// (fed only by `SMSG_ITEM_TIME_UPDATE`) and one temporary-enchant deadline per slot at `+0x324`
/// (fed only by `SMSG_ITEM_ENCHANT_TIME_UPDATE`; the enchantment field's duration is never read).
/// Absolute deadlines, recomputed on every read and never ticked (`0x5d9c60`, `0x5d9d00`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Countdowns {
    lifetime: Option<Instant>,
    enchants: [Option<Instant>; ENCHANT_SLOTS],
}

/// The setters' store rule (`0x5d9c00`, `0x5d9cc0`): a signed `<= 0` (`jle`) clears the cell, so a
/// top-bit value is absence, not a 68-year deadline; anything else parks `now + seconds`.
fn deadline(seconds: u32) -> Option<Instant> {
    ((seconds as i32) > 0).then(|| Instant::now() + Duration::from_secs(u64::from(seconds)))
}

/// A cell's time left: `None` when unset, `Some(0)` once elapsed (`0x5d9d00`'s `max(0, …)`).
fn left(cell: Option<Instant>) -> Option<Duration> {
    cell.map(|at| at.saturating_duration_since(Instant::now()))
}

/// The tooltip's read: an elapsed cell is no timer (its `!= 0` gate prints the plain line).
fn remaining_ms(cell: Option<Instant>) -> Option<u64> {
    left(cell)
        .filter(|l| !l.is_zero())
        .map(|l| l.as_millis() as u64)
}

impl Countdowns {
    /// `SMSG_ITEM_ENCHANT_TIME_UPDATE`'s setter `0x5d9cc0`. A slot past the seventh is refused
    /// (`false`), where the reference indexes unchecked and overruns into the next member.
    pub(crate) fn set_enchant(&mut self, slot: u32, seconds: u32) -> bool {
        let Some(cell) = self.enchants.get_mut(slot as usize) else {
            return false;
        };
        *cell = deadline(seconds);
        true
    }

    /// `SMSG_ITEM_TIME_UPDATE`'s setter `0x5d9c00`.
    pub(crate) fn set_lifetime(&mut self, seconds: u32) {
        self.lifetime = deadline(seconds);
    }

    fn enchant(&self, slot: u32) -> Option<Instant> {
        self.enchants.get(slot as usize).copied().flatten()
    }

    /// Milliseconds left on the temporary enchant in `slot`; `None` also once expired.
    pub fn enchant_remaining_ms(&self, slot: u32) -> Option<u64> {
        remaining_ms(self.enchant(slot))
    }

    /// [`Self::enchant_remaining_ms`] floored to the second, for the snapshot feeds: a per-ms read
    /// would fire `UNIT_INVENTORY_CHANGED` every frame while a timer runs.
    pub fn enchant_remaining_display_ms(&self, slot: u32) -> Option<u64> {
        self.enchant_remaining_ms(slot).map(|ms| ms - ms % 1000)
    }

    /// The deadline read without the expired-is-absent collapse: `Some(0)` once run out, `None`
    /// only when never set. `GetWeaponEnchantInfo` returns `max(0, deadline - now)` (`0x5d9d00`),
    /// so an elapsed enchant answers 0 and `BuffFrame_Enchant_OnUpdate` draws "0 s".
    pub fn enchant_deadline_ms(&self, slot: u32) -> Option<u64> {
        left(self.enchant(slot)).map(|l| l.as_millis() as u64)
    }

    /// Milliseconds left on the item's own lifetime; `None` also once elapsed.
    pub fn lifetime_remaining_ms(&self) -> Option<u64> {
        remaining_ms(self.lifetime)
    }

    /// [`Self::lifetime_remaining_ms`] floored to the whole second, as the enchant read is.
    pub fn lifetime_remaining_display_ms(&self) -> Option<u64> {
        self.lifetime_remaining_ms().map(|ms| ms - ms % 1000)
    }

    /// This item's share of [`ItemChanges::countdown_steps`]: `floor(seconds left) + 1` per live
    /// cell, 0 when unset or elapsed, so the final `Some(0)` to `None` collapse is its own step.
    fn steps(&self, now: Instant) -> u64 {
        std::iter::once(self.lifetime)
            .chain(self.enchants)
            .flatten()
            .map(|at| {
                let left = at.saturating_duration_since(now);
                if left.is_zero() {
                    0
                } else {
                    left.as_secs() + 1
                }
            })
            .sum()
    }
}

/// An item entity whose fields or countdown cells were written since the reader last ran.
type ItemMoved = (
    With<ItemObject>,
    Or<(Changed<ObjectStore>, Changed<Countdowns>)>,
);

/// Whether any item object was created, written or destroyed this frame; the inventory feeds' gate.
#[derive(SystemParam)]
pub struct ItemChanges<'w, 's> {
    changed: Query<'w, 's, (), ItemMoved>,
    removed: RemovedComponents<'w, 's, ItemObject>,
    countdowns: Query<'w, 's, &'static Countdowns>,
}

impl ItemChanges<'_, '_> {
    /// Drains the removal reader, so call it once per run.
    pub fn moved(&mut self) -> bool {
        let removed = self.removed.read().count() > 0;
        !self.changed.is_empty() || removed
    }

    /// The sum of every item's [`Countdowns`] steps: it moves exactly when a second-floored read
    /// can, and each term only falls between landings, so no change masks another.
    pub fn countdown_steps(&self) -> u64 {
        let now = Instant::now();
        self.countdowns.iter().map(|c| c.steps(now)).sum()
    }
}

/// The player's inventory as one read: the self descriptor, the object lookup and the item watch.
#[derive(SystemParam)]
pub struct Inventory<'w, 's> {
    pub self_store: Query<'w, 's, &'static ObjectStore, With<crate::app_core::session::SelfPlayer>>,
    pub self_changed: Query<
        'w,
        's,
        (),
        (
            With<crate::app_core::session::SelfPlayer>,
            Changed<ObjectStore>,
        ),
    >,
    pub objects: Objects<'w, 's>,
    pub changes: ItemChanges<'w, 's>,
}

/// The Copy slice of an [`ItemInfo`] that equipment rendering and combat animation read.
#[derive(Clone, Copy)]
pub struct HeldTemplate {
    pub display_info_id: u32,
    pub inventory_type: u32,
    pub sheath: u32,
    pub class: u32,
    pub subclass: u32,
    /// `Material.dbc` id (1 metal, 2 wood, 5 chain, 6 plate, 7 cloth, 8 leather, 0 undefined), the
    /// only input to the draw/stow sound: every `SheatheSoundLookups` row of a material agrees.
    pub material: u32,
}

/// `SpellItemEnchantment.dbc`, for the weapon glow ([`crate::entities::item_glow`]) and the
/// tooltip enchant line ([`crate::ui_items`]). Optional: absent, neither draws.
#[derive(Resource)]
pub struct Enchants(pub wrathplus_formats::EnchantCatalog);

/// One enchant slot: `(slot index, id, charges, remaining ms)`. The id is signed: the sign picks
/// the line's colour only, and `abs(id)` names the DBC row (`0x52c9f9`).
pub(crate) type EnchantSlot = (u8, i32, u32, Option<u64>);

/// The tooltip lines an item's enchant slots contribute; every tooltip surface feeds through here.
/// The reference's per-slot gate (`0x52c9f9` to `0x52ca23`): `id != 0` and `abs(id)` names a
/// `SpellItemEnchantment` row, else nothing. Our own items carry all seven slots; another
/// player's descriptor carries only the permanent and temporary ones (vmangos).
pub fn enchant_lines(
    slots: impl IntoIterator<Item = EnchantSlot>,
    enchants: Option<&Enchants>,
) -> Vec<wrathplus_ui::script::EnchantView> {
    let lines = enchant_lines_quiet(slots, enchants);
    // Logged once per session: a missing enchant looks exactly like an unenchanted item.
    if !lines.is_empty() {
        static FIRST: std::sync::Once = std::sync::Once::new();
        // The whole view: the id, charges and countdown each ride a different wire lane.
        FIRST.call_once(|| info!("item enchant: {lines:?} (the first resolved this session)"));
    }
    lines
}

/// `0x5da2c0`: already bound when `ITEM_FIELD_FLAGS & 1` is set or a live enchant slot names a
/// binding `SpellItemEnchantment` row (`0x5da300` to `0x5da320`). The enchant cursor's bind
/// question (gate `0x495d60`) and the tooltip's Soulbound line both read it. It reads the raw
/// descriptor, not [`enchant_lines`]: Firestone and Orb of Fire both bind and print no line.
pub fn already_bound(fields: &ObjectFields, cat: Option<&Enchants>) -> bool {
    fields.item_flags().is_some_and(|f| f & 0x1 != 0)
        || (0..7).any(|slot| live_enchant(fields, slot, cat).is_some_and(|id| binds(id, cat)))
}

/// One enchant slot as the bind checks read it (`0x495eec`): the raw id must be positive (`jl` at
/// `0x495ef4`, `0x5da306`) and name a row, where the tooltip line uses `abs(id)`.
pub fn live_enchant(fields: &ObjectFields, slot: u8, cat: Option<&Enchants>) -> Option<u32> {
    let id = u32::try_from(fields.item_enchant(slot)?).ok()?;
    cat.is_some_and(|c| c.0.has_row(id)).then_some(id)
}

/// `SpellItemEnchantment.Flags & 1`: this enchant soulbinds the item it lands on.
pub fn binds(id: u32, cat: Option<&Enchants>) -> bool {
    cat.is_some_and(|c| c.0.binds_the_item(id))
}

/// [`enchant_lines`] without the log line, for the load-time resolve of the whole
/// `ItemRandomProperties` table ([`random_property_views`]).
fn enchant_lines_quiet(
    slots: impl IntoIterator<Item = EnchantSlot>,
    enchants: Option<&Enchants>,
) -> Vec<wrathplus_ui::script::EnchantView> {
    let Some(enchants) = enchants else {
        return Vec::new();
    };
    let lines: Vec<wrathplus_ui::script::EnchantView> = slots
        .into_iter()
        .filter(|&(_, id, _, _)| id != 0)
        // `SpellItemEnchantment.Flags & 0x2`: no line; both reference printers return before
        // reading the name (`0x6290e4`, `0x62923e`). Twelve rows: the totem weapon imbues,
        // Firestone, Orb of Fire.
        .filter(|&(_, id, _, _)| !enchants.0.tooltip_hides_name(id.unsigned_abs()))
        .filter_map(|(slot, id, charges, remaining_ms)| {
            let name = enchants.0.name(id.unsigned_abs())?.to_string();
            Some(wrathplus_ui::script::EnchantView {
                slot,
                name,
                negative: id < 0,
                charges,
                remaining_ms,
            })
        })
        .collect();
    lines
}

/// `ItemRandomProperties.dbc`, the random-suffix roll: a name suffix and tooltip enchant slots
/// 2..6 (`0x52b7e0`). Optional: absent, names stay unsuffixed.
#[derive(Resource)]
pub struct RandomProperties(pub wrathplus_formats::RandomPropertyCatalog);

/// The item's display name, the reference's one formatter `0x5d8b00(entry, randomPropertyId)`:
/// `ITEM_SUFFIX_TEMPLATE` (`"%s %s"`) with the roll's suffix, or the plain name when the id names
/// no suffixed row. Every surface that shows an item name goes through here.
pub fn item_display_name(
    base: &str,
    random_property_id: i32,
    props: Option<&RandomProperties>,
) -> String {
    match props.and_then(|p| p.0.get(random_property_id)) {
        Some(row) => format!("{base} {}", row.suffix),
        None => base.to_string(),
    }
}

/// The tooltip lines of a roll, for a source with no item object: the reference (`0x52b7bf` to
/// `0x52b7fb`) copies the row's five enchant ids into slots 2..6 and prints them as its own.
pub(crate) fn random_property_lines(
    row: &wrathplus_formats::RandomProperty,
    enchants: Option<&Enchants>,
) -> Vec<wrathplus_ui::script::EnchantView> {
    let slots = row.enchants.iter().enumerate().map(|(i, &id)| {
        (
            wrathplus_formats::RANDOM_PROPERTY_FIRST_SLOT + i as u8,
            id as i32,
            0,
            None,
        )
    });
    enchant_lines_quiet(slots, enchants)
}

/// Every `ItemRandomProperties` row as its suffix and named enchant lines, pushed to the engine
/// whole: a chat-link tooltip has no hover re-enter, so a per-id ask would miss the first click.
pub fn random_property_views(
    props: &RandomProperties,
    enchants: Option<&Enchants>,
) -> std::collections::HashMap<u32, wrathplus_ui::script::RandomPropertyView> {
    props
        .0
        .iter()
        .map(|(id, row)| {
            (
                id,
                wrathplus_ui::script::RandomPropertyView {
                    suffix: row.suffix.clone(),
                    enchants: random_property_lines(row, enchants),
                },
            )
        })
        .collect()
}

/// The two catalogs a random-suffix roll resolves through: the roll table and the enchant names.
#[derive(Clone, Copy)]
pub struct RollCatalogs<'a> {
    pub props: Option<&'a RandomProperties>,
    pub enchants: Option<&'a Enchants>,
}

impl RollCatalogs<'_> {
    /// No DBC catalogs: the plain name and no suffix lines.
    #[cfg(any(test, feature = "test-support"))]
    pub const NONE: RollCatalogs<'static> = RollCatalogs {
        props: None,
        enchants: None,
    };

    /// [`item_display_name`] with this roll's suffix.
    pub fn name(&self, base: &str, random_property_id: u32) -> String {
        item_display_name(base, random_property_id as i32, self.props)
    }

    /// [`random_property_lines`] for one id; the live surfaces read the engine's pushed table.
    #[cfg(any(test, feature = "test-support"))]
    pub fn lines(&self, random_property_id: u32) -> Vec<wrathplus_ui::script::EnchantView> {
        match self.props.and_then(|p| p.0.get(random_property_id as i32)) {
            Some(row) => random_property_lines(row, self.enchants),
            None => Vec::new(),
        }
    }
}

/// [`Items::take_answers`]' bound: older answers drop when nothing drains the log.
const ANSWER_LOG_BOUND: usize = 1024;

/// Build 12340's nine-field item link (`0x61e290`, `0x61e360`):
/// `item:entry:ench:gem1:gem2:gem3:0:random:suffix:level`, coloured by quality (table `0xad2a84`,
/// heirlooms sharing artifacts' colour). The mail inbox and the dungeon finder's rewards share it.
#[allow(clippy::too_many_arguments)]
pub fn wrath_item_link(
    entry: u32,
    name: &str,
    quality: u32,
    enchants: [u32; 4],
    random_property: i32,
    suffix_factor: u32,
    level: u32,
) -> String {
    let color = match quality {
        0 => "ff9d9d9d",
        2 => "ff1eff00",
        3 => "ff0070dd",
        4 => "ffa335ee",
        5 => "ffff8000",
        6 | 7 => "ffe6cc80",
        _ => "ffffffff",
    };
    format!(
        "|c{color}|Hitem:{}:{}:{}:{}:{}:0:{}:{}:{}|h[{name}]|h|r",
        entry as i32,
        enchants[0] as i32,
        enchants[1] as i32,
        enchants[2] as i32,
        enchants[3] as i32,
        random_property,
        suffix_factor as i32,
        level as i32,
    )
}

/// Item templates by entry, asked once, and the enchant times queued for items not yet held.
#[derive(Resource, Default)]
pub struct Items {
    /// A `None` answer is the server's unknown entry, cached so it is never re-asked.
    templates: QueryCache<u32, ItemInfo>,
    /// Landed since the last [`Self::take_fresh`]; pushed to the UI unprompted.
    fresh: Vec<u32>,
    /// Every answer in arrival order, `(entry, found)`, for the dungeon finder's item-callback
    /// queue ([`Self::take_answers`]), its one consumer; bounded at [`ANSWER_LOG_BOUND`]. Behind a
    /// lock so that consumer drains through a shared borrow and orders against no reader.
    answers: std::sync::Mutex<std::collections::VecDeque<(u32, bool)>>,
    /// The reference's `PlayerPendingItemExpiration` list (`0x5ebd40`, at `CGPlayer_C + 0x1cc8`):
    /// `(guid, slot, seconds)` enchant updates for an item not yet held. `0x5ebde0` applies them
    /// when the item is set up (`0x5d8440`), counting seconds from then; they live as long as the
    /// player, here the session. `SMSG_ITEM_TIME_UPDATE` has no such list.
    pending_enchant_times: Vec<(u64, u32, u32)>,
}

impl Items {
    /// Queue an `SMSG_ITEM_ENCHANT_TIME_UPDATE` for an item not held. The caller has checked the
    /// active player resolves: the reference drops the update when none does.
    pub(crate) fn queue_enchant_time(&mut self, guid: u64, slot: u32, seconds: u32) {
        debug!("enchant timer: item {guid:#x} not held — queued for its arrival");
        self.pending_enchant_times.push((guid, slot, seconds));
    }

    /// The item `guid` arrived: its queued enchant updates in arrival order, unlinked (`0x5ebde0`).
    pub(crate) fn take_enchant_times(&mut self, guid: u64) -> Vec<(u32, u32)> {
        let mut taken = Vec::new();
        self.pending_enchant_times.retain(|&(g, slot, seconds)| {
            let hit = g == guid;
            if hit {
                taken.push((slot, seconds));
            }
            !hit
        });
        taken
    }

    /// The template for `entry` if known; a miss asks once per entry per connection (`guid` rides
    /// along, `0` for template-only). A cached negative is also `None`, without a re-ask.
    pub fn template(&self, entry: u32, guid: u64, commands: &NetCommands) -> Option<&ItemInfo> {
        if self.templates.answered(entry) {
            return self.templates.get(entry);
        }
        let command = ClientCommand::ItemQuery { entry, guid };
        if !commands.supports(&command) {
            return None;
        }
        self.templates.get_or_ask(entry, || {
            debug!("items: asking template (entry {entry})");
            let _ = commands.0.send(command);
        })
    }

    /// Whether the server answered `entry` as unknown, not still pending; the cast-fail redisplay
    /// then shows the reference's `"UNKNOWN"` instead of waiting.
    pub fn template_answered_unknown(&self, entry: u32) -> bool {
        self.templates.answered_unknown(entry)
    }

    /// The [`HeldTemplate`] view of [`Self::template`], asked template-only.
    pub fn held(&self, entry: u32, commands: &NetCommands) -> Option<HeldTemplate> {
        self.template(entry, 0, commands).map(|i| HeldTemplate {
            display_info_id: i.display_info_id,
            inventory_type: i.inventory_type,
            sheath: i.sheath,
            class: i.class,
            subclass: i.subclass,
            material: i.material,
        })
    }

    /// [`Self::template`] without the ask: the cached record only.
    pub fn template_cached(&self, entry: u32) -> Option<&ItemInfo> {
        self.templates.get(entry)
    }

    /// Record a template answer (`SMSG_ITEM_QUERY_SINGLE_RESPONSE`); `None` = unknown entry.
    pub fn insert_template(&mut self, entry: u32, info: Option<ItemInfo>) {
        if info.is_some() {
            self.fresh.push(entry);
        }
        let answers = self.answers.get_mut().unwrap_or_else(|e| e.into_inner());
        if answers.len() == ANSWER_LOG_BOUND {
            answers.pop_front();
        }
        answers.push_back((entry, info.is_some()));
        // A negative answer moves the generation too: "still asking" to "answered unknown" is a
        // display transition for the cast-fail redisplay.
        self.templates.insert(entry, info);
    }

    /// Bumped by every landed answer, for consumers besides the drain; the stand-in for the
    /// reference's `DBCACHECALLBACK` redisplay (`0x6e29b0`).
    pub fn template_epoch(&self) -> u64 {
        self.templates.generation()
    }

    /// Every entry with a cached template.
    pub fn cached_template_ids(&self) -> Vec<u32> {
        self.templates
            .iter()
            .filter_map(|(&id, t)| t.is_some().then_some(id))
            .collect()
    }

    /// Drain the answers since the last drain, in arrival order, `(entry, found)`.
    pub fn take_answers(&self) -> Vec<(u32, bool)> {
        let mut answers = self.answers.lock().unwrap_or_else(|e| e.into_inner());
        answers.drain(..).collect()
    }

    /// Drain the entries whose template landed since the last drain.
    pub fn take_fresh(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.fresh)
    }

    /// World enter: release the ask-once latch and keep what the cache learned. An ask is marked
    /// pending before the send, and a send made while no writer is held is dropped silently.
    pub(crate) fn clear_pending(&mut self) {
        self.templates.clear_pending();
    }

    /// Disconnect: drop the in-flight asks and the queued enchant times, keep the templates. The
    /// item objects and their countdowns go with the index's sweep.
    pub(crate) fn clear_session(&mut self) {
        self.pending_enchant_times.clear();
        self.templates.clear_pending();
    }
}

impl crate::query_cache::AskOnce for Items {
    fn clear_pending(&mut self) {
        Items::clear_pending(self);
    }
}

/// The ask-once test fixture; it holds the live receiver, without which every send fails.
#[cfg(any(test, feature = "test-support"))]
pub struct TestDeps {
    pub items: Items,
    pub commands: NetCommands,
    /// The object index: seed with [`Self::spawn_item`], read through [`Self::with_objects`].
    pub(crate) world: World,
    rx: crossbeam_channel::Receiver<ClientCommand>,
}

#[cfg(any(test, feature = "test-support"))]
impl Default for TestDeps {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut world = World::new();
        world.init_resource::<GuidIndex>();
        TestDeps {
            items: Items::default(),
            commands: NetCommands(tx),
            world,
            rx,
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl TestDeps {
    pub fn new() -> Self {
        Self::default()
    }

    /// An item object in the index, as the wire's `ItemCreate` spawns it.
    pub fn spawn_item(&mut self, guid: u64, fields: ObjectFields) -> Entity {
        test_spawn_item(&mut self.world, guid, fields, false)
    }

    /// Run `f` with the object lookup, the template cache and the command channel borrowed apart.
    pub fn with_objects<R>(&mut self, f: impl FnOnce(&Objects, &Items, &NetCommands) -> R) -> R {
        let (world, items, commands) = (&mut self.world, &self.items, &self.commands);
        let mut state = bevy::ecs::system::SystemState::<Objects>::new(world);
        let objects = state.get(world);
        f(&objects, items, commands)
    }

    /// The entries asked of the server, observable while the icon they feed is still `None`.
    pub fn queried_entries(&self) -> Vec<u32> {
        self.rx
            .try_iter()
            .filter_map(|c| match c {
                ClientCommand::ItemQuery { entry, .. } => Some(entry),
                _ => None,
            })
            .collect()
    }
}

/// A test's object index: a world to seed with items, and the [`Objects`] lookup over it. Seed
/// first, then read: `get()` borrows the world while its answer lives, so call sites write
/// `&objs.get()` inline.
#[cfg(any(test, feature = "test-support"))]
pub struct TestObjects {
    world: World,
    state: bevy::ecs::system::SystemState<Objects<'static, 'static>>,
}

#[cfg(any(test, feature = "test-support"))]
impl Default for TestObjects {
    fn default() -> Self {
        let mut world = World::new();
        world.init_resource::<GuidIndex>();
        let state = bevy::ecs::system::SystemState::new(&mut world);
        Self { world, state }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl TestObjects {
    pub fn new() -> Self {
        Self::default()
    }

    /// One item instance in the index, as the wire's `ItemCreate` makes it.
    pub fn spawn(&mut self, guid: u64, fields: ObjectFields) {
        test_spawn_item(&mut self.world, guid, fields, false);
    }

    /// The same for a container instance (`TYPEMASK_CONTAINER`).
    pub fn spawn_container(&mut self, guid: u64, fields: ObjectFields) {
        test_spawn_item(&mut self.world, guid, fields, true);
    }

    pub fn get(&mut self) -> Objects<'_, '_> {
        self.state.get(&self.world)
    }
}

/// `ItemSubClass.dbc`: the slot and type line's alternate-proficiency and hidden-name gates; the
/// item tooltip loads it (`ui_items`).
#[derive(Resource)]
pub struct ItemSubClasses(pub wrathplus_formats::ItemSubClassCatalog);

/// [`spawn_item`] straight into a test world, without the command queue.
#[cfg(any(test, feature = "test-support"))]
pub fn test_spawn_item(
    world: &mut World,
    guid: u64,
    fields: ObjectFields,
    container: bool,
) -> Entity {
    let e = world
        .spawn((Guid(guid), ObjectStore(fields), ItemObject { container }))
        .id();
    world.resource_mut::<GuidIndex>().0.insert(guid, e);
    e
}

/// `EQUIPMENT_SLOT_MAINHAND` / `_OFFHAND` (vmangos `EquipmentSlots`).
pub const EQUIPMENT_SLOT_MAINHAND: u8 = 15;
pub const EQUIPMENT_SLOT_OFFHAND: u8 = 16;

/// One equipment slot's item class; `None` while empty or its template is in flight.
fn equipped_class(
    store: &ObjectStore,
    objects: &Objects,
    items: &Items,
    commands: &NetCommands,
    slot: u8,
) -> Option<u8> {
    let guid = store.0.player_inv_slot(slot).filter(|&g| g != 0)?;
    let entry = objects.object(guid).and_then(|o| o.object_entry())?;
    Some(items.template(entry, guid, commands)?.class as u8)
}

/// Whether the disarm flag is up (`UNIT_FIELD_FLAGS & UNIT_FLAG_DISARMED`), which hides one hand
/// from the equipped-item search.
pub fn is_disarmed(store: &ObjectStore) -> bool {
    store.0.unit_flags() & crate::app_core::state::UNIT_FLAG_DISARMED != 0
}

/// The equipment slot this character's disarm hides ([`crate::app_core::state::disarmed_hand`] over
/// the raw inventory), for the action bar's equipped-item requirement (`0x5f0c50`) and the
/// item-use refusal (`CGItem::Use` rung 15). `None` while the flag is down or neither hand holds
/// a weapon.
pub fn disarmed_equipment_slot(
    store: &ObjectStore,
    objects: &Objects,
    items: &Items,
    commands: &NetCommands,
) -> Option<u8> {
    if !is_disarmed(store) {
        return None;
    }
    let main = equipped_class(store, objects, items, commands, EQUIPMENT_SLOT_MAINHAND);
    let off = equipped_class(store, objects, items, commands, EQUIPMENT_SLOT_OFFHAND);
    crate::app_core::state::disarmed_hand(main, off)
        .map(|hand| EQUIPMENT_SLOT_MAINHAND + hand as u8)
}

/// [`disarmed_equipment_slot`] without the ask.
pub fn disarmed_equipment_slot_cached(
    store: &ObjectStore,
    objects: &Objects,
    items: &Items,
) -> Option<u8> {
    if !is_disarmed(store) {
        return None;
    }
    let class_of = |slot: u8| -> Option<u8> {
        let guid = store.0.player_inv_slot(slot).filter(|&g| g != 0)?;
        let entry = objects.object(guid).and_then(|o| o.object_entry())?;
        Some(items.template_cached(entry)?.class as u8)
    };
    crate::app_core::state::disarmed_hand(
        class_of(EQUIPMENT_SLOT_MAINHAND),
        class_of(EQUIPMENT_SLOT_OFFHAND),
    )
    .map(|hand| EQUIPMENT_SLOT_MAINHAND + hand as u8)
}

/// A minimal valid item template named `name`; the sentinels that matter are `allowable_*` = -1
/// and `stackable` = 1.
#[cfg(any(test, feature = "test-support"))]
pub fn test_template(name: &str) -> ItemInfo {
    ItemInfo {
        class: 0,
        subclass: 0,
        name: name.into(),
        display_info_id: 1,
        quality: 1,
        flags: 0,
        buy_price: 0,
        sell_price: 0,
        inventory_type: 0,
        allowable_class: -1,
        allowable_race: -1,
        item_level: 0,
        required_level: 0,
        required_skill: 0,
        required_skill_rank: 0,
        required_spell: 0,
        required_honor_rank: 0,
        required_city_rank: 0,
        required_rep_faction: 0,
        required_rep_rank: 0,
        max_count: 0,
        stackable: 1,
        container_slots: 0,
        stats: Vec::new(),
        damages: Vec::new(),
        dmg_min: 0.0,
        dmg_max: 0.0,
        dmg_type: 0,
        armor: 0,
        resistances: [0; 6],
        delay_ms: 0,
        ammo_type: 0,
        ranged_mod_range: 0.0,
        spells: Vec::new(),
        spell_charges_0: 0,
        use_spell: None,
        bonding: 0,
        description: String::new(),
        page_text: 0,
        language_id: 0,
        page_material: 0,
        start_quest: 0,
        lock_id: 0,
        material: 0,
        sheath: 0,
        random_property: 0,
        block: 0,
        item_set: 0,
        max_durability: 0,
        area: 0,
        map: 0,
        bag_family: 0,
        totem_category: 0,
    }
}

#[cfg(test)]
#[path = "items_tests.rs"]
mod tests;
