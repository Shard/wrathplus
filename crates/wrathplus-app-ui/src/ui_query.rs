//! The Lua query provider: one snapshot of the frame's inputs, built after the net drain and
//! before the UI feeds, which the VM asks inside a verb's call ([`wrathplus_ui::script::GameQuery`])
//! with the same code the action bar runs. The snapshot holds inputs, not answers, so a first ask
//! of any spell at any streamed unit is answered.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use wrathplus_assets::LockRecover;
use wrathplus_formats::RangeUnit;
use wrathplus_protocol::types::ObjectType;
use wrathplus_ui::script::{GameQuery, ItemAsk, UiScript};

use crate::app_core::cooldowns::Cooldowns;
use crate::app_core::session::{ObjectStore, SelfPlayer};
use crate::app_core::state::{Factions, Selection, Spells};
use crate::items::Items;
use crate::net::wrath_combo_points::{banked, NativeComboPoints};
use crate::net::{GuidIndex, Objects, Reputations};
use crate::spell::usable::{self, TargetFacts, WornCached, WornSlots};
use crate::spell::{range_check, RangeUnits, SpellModifiers};

pub use crate::app_core::schedule::GameQueryBuild;

/// The handle the builder swaps each frame's snapshot into and every VM asks through. A call
/// clones the inner `Arc` under the lock and computes unlocked, so a reader sees a whole frame.
#[derive(Resource, Clone, Default)]
pub struct GameQueryLink(Arc<Mutex<Arc<FrameQuery>>>);

impl GameQueryLink {
    fn frame(&self) -> Arc<FrameQuery> {
        self.0.lock_recover().clone()
    }
}

impl GameQuery for GameQueryLink {
    fn spell_in_range(&self, spell: u32, pet: bool, unit: u64) -> Option<bool> {
        self.frame().spell_in_range(spell, pet, unit)
    }

    fn spell_usable(&self, spell: u32, pet: bool) -> Option<(bool, bool)> {
        self.frame().spell_usable(spell, pet)
    }

    fn spell_has_range(&self, spell: u32, pet: bool) -> Option<bool> {
        self.frame().spell_has_range(spell, pet)
    }

    fn spell_cooldown(&self, spell: u32) -> Option<(f64, f64, bool)> {
        self.frame().cooldown(spell, 0)
    }

    fn item_usable(&self, item: ItemAsk) -> Option<(bool, bool)> {
        self.frame().item_usable(item)
    }

    fn item_cooldown(&self, item: ItemAsk) -> Option<(f64, f64, bool)> {
        self.frame().cooldown(item.spell, item.entry)
    }
}

/// One frame's inputs. With no player or no spells it answers nothing, the no-provider path.
#[derive(Default)]
pub struct FrameQuery {
    spells: Option<Arc<Spells>>,
    mods: Arc<SpellModifiers>,
    me: Option<Me>,
    /// Every streamed unit and player, ours included, by guid.
    units: HashMap<u64, UnitRow>,
}

/// Our own player: the caster `RangeUnits::caster` reads, the `"pet"` unit, and the usable
/// walk's inputs as the action bar reads them this frame.
struct Me {
    pos: Vec3,
    range: RangeUnit,
    pet: u64,
    store: ObjectStore,
    /// The current target's facts; none when we target ourselves, as the bar reads it.
    target: Option<TargetFacts>,
    cooldowns: Cooldowns,
    combo_banked: u8,
    carried: HashMap<u32, u32>,
    /// The entries worn in equipment slots 0-18.
    equipped: [u32; 19],
    /// A held entry's use-spell slot category, where it has one (`+0x150`): the cooldown test's
    /// override of the spell's (`0x807e28`).
    use_categories: HashMap<u32, u32>,
    worn: WornSlots,
    /// The frame's `(Instant, GetTime)` pair every cooldown push maps through.
    clock: (Instant, f64),
}

#[derive(Clone, Copy)]
struct UnitRow {
    pos: Vec3,
    range: RangeUnit,
}

impl FrameQuery {
    /// `IsSpellInRange`'s `0x809610`: the action bar's compare ([`range_check::in_range`]) from
    /// the caster, the pet for the pet book (`0x5d3390`), to `unit`; nil without either unit or
    /// a range. Labelled: `0x809610`'s target-validity legs (target-type and creature-type masks,
    /// dead, can attack or assist) are not read, so a heal asked of an enemy answers 1 or 0 where
    /// the reference answers nil; and the compare is the 1.12 `GetMinMaxRange` order the bar uses.
    pub fn spell_in_range(&self, spell: u32, pet: bool, unit: u64) -> Option<bool> {
        let (me, spells) = (self.me.as_ref()?, self.spells.as_deref()?);
        let caster = match pet {
            true => *self.units.get(&me.pet)?,
            false => UnitRow {
                pos: me.pos,
                range: me.range,
            },
        };
        let d = spells.catalog.get(spell)?;
        let target = self.units.get(&unit)?;
        let row = spells.ranges.get(d.range_index);
        let d2 = caster.pos.distance_squared(target.pos);
        range_check::in_range(&self.mods, d, row, caster.range, Some(target.range), d2)
    }

    /// `IsUsableSpell`: the action bar's walk ([`usable::spell_usable`]) on this frame's inputs,
    /// so it equals `IsUsableAction` for the same spell. Labelled: that walk is the 1.12
    /// `0x6e3d60` port, not 12340's `0x80b5d0`/`0x8017e0` (runes, health costs), and a pet-book
    /// spell runs the player's walk where 12340 runs the pet's.
    pub fn spell_usable(&self, spell: u32, pet: bool) -> Option<(bool, bool)> {
        let (me, spells) = (self.me.as_ref()?, self.spells.as_deref()?);
        if pet && me.pet == 0 {
            return None;
        }
        let d = spells.catalog.get(spell)?;
        Some(usable::spell_usable(spell, d, spells, &self.walk(me)))
    }

    /// `SpellHasRange`'s `0x801650` with no target from the player, or the pet for the pet book
    /// ([`range_check::has_range`], the bar's `ActionHasRange`, the range mod applied); false with
    /// no pet (`0x541bba`).
    pub fn spell_has_range(&self, spell: u32, pet: bool) -> Option<bool> {
        let (me, spells) = (self.me.as_ref()?, self.spells.as_deref()?);
        let d = spells.catalog.get(spell)?;
        let caster = match pet {
            true => match self.units.get(&me.pet) {
                Some(row) => row.range,
                None => return Some(false),
            },
            false => me.range,
        };
        let row = spells.ranges.get(d.range_index);
        Some(range_check::has_range(&self.mods, d, row, caster))
    }

    /// The cooldown manager's record (`0x807980`) for `spell` keyed with `item` (0 for a spell),
    /// as `GetTime` seconds: `GetCooldownInfo`'s longest remainder, as the bar and the book push
    /// it, or `(0, 0, true)` when none runs.
    pub fn cooldown(&self, spell: u32, item: u32) -> Option<(f64, f64, bool)> {
        let (me, spells) = (self.me.as_ref()?, self.spells.as_deref()?);
        let (anchor, ui_now) = me.clock;
        let info = me
            .cooldowns
            .info(spell, item, spells.catalog.get(spell), anchor);
        Some(
            info.ui_triple(anchor, ui_now)
                .map_or((0.0, 0.0, true), |(start, dur, on)| {
                    (start as f64 / 1000.0, f64::from(dur) / 1000.0, on)
                }),
        )
    }

    /// `IsUsableItem` (`0x51c450`): not held (the default walk `0x754d00`, here the carried
    /// counts and the worn slots) or on hold (`0x80ac10` → `0x807da0`, the same on-hold test as
    /// the bar's item gate) is nil, nil; no use spell or no row is 1, nil (`0x51c61d`); else the
    /// use spell's walk. Labelled: the arena leg (map type 4, a recovery past 900 s or negative
    /// charges, `0x51c51f`-`0x51c5b1`) is not built, the held count misses the keyring and the
    /// token band, and this differs from the bar's 1.12 item arm on a use spell with no row.
    pub fn item_usable(&self, item: ItemAsk) -> Option<(bool, bool)> {
        let (me, spells) = (self.me.as_ref()?, self.spells.as_deref()?);
        let held = me.carried.get(&item.entry).is_some_and(|&n| n > 0)
            || me.equipped.contains(&item.entry);
        let d = spells.catalog.get(item.spell);
        let category = (me.use_categories.get(&item.entry).copied())
            .unwrap_or_else(|| d.map_or(0, |d| d.category));
        let on_hold = (me.cooldowns).has_on_hold_record(item.spell, item.entry, category);
        if !held || (d.is_some() && on_hold) {
            return Some((false, false));
        }
        let Some(d) = d else {
            return Some((true, false));
        };
        Some(usable::spell_usable(item.spell, d, spells, &self.walk(me)))
    }

    fn walk<'a>(&'a self, me: &'a Me) -> usable::UsableCtx<'a> {
        usable::UsableCtx {
            store: &me.store,
            target: me.target,
            cooldowns: &me.cooldowns,
            spell_mods: &self.mods,
            combo_banked: me.combo_banked,
            carried: &me.carried,
            worn: &me.worn,
        }
    }
}

/// What the usable walk reads, as `feed_action_state` reads it.
#[derive(SystemParam)]
pub(crate) struct WalkInputs<'w, 's> {
    objects: Objects<'w, 's>,
    items: Res<'w, Items>,
    cooldowns: Res<'w, Cooldowns>,
    combo: Res<'w, NativeComboPoints>,
    selection: Res<'w, Selection>,
    factions: Option<Res<'w, Factions>>,
    reputations: Res<'w, Reputations>,
    relations: crate::target::RelationInputs<'w, 's>,
    clock: Res<'w, crate::app_core::ui_flags::UiClock>,
}

impl WalkInputs<'_, '_> {
    fn me(&self, store: &ObjectStore, bodies: &Bodies, pos: Vec3, range: RangeUnit) -> Me {
        let objects = &self.objects;
        let target = (self.selection.guid.and_then(|g| objects.entity(g)))
            .and_then(|e| bodies.get(e).ok())
            .filter(|(_, _, ours)| !ours)
            .map(|(t, _, _)| {
                let (f, r) = (self.factions.as_deref(), &*self.reputations);
                TargetFacts::of(t, store, f, r, self.relations.context())
            });
        let items = &self.items;
        let carried = crate::inventory::carried_counts(&store.0, objects);
        let equipped: [u32; 19] = std::array::from_fn(|i| {
            let guid = store.0.player_inv_slot(i as u8).unwrap_or(0);
            (objects.object(guid)).map_or(0, |o| o.object_entry().unwrap_or(0))
        });
        let use_categories = (carried.keys().chain(&equipped))
            .filter_map(|&entry| {
                let use_spell = items.template_cached(entry)?.use_spell?;
                (use_spell.category != 0).then_some((entry, use_spell.category))
            })
            .collect();
        Me {
            pos,
            range,
            pet: store.0.unit_pet_guid().unwrap_or(0),
            store: store.clone(),
            target,
            cooldowns: self.cooldowns.clone(),
            combo_banked: banked(&self.combo).0,
            carried,
            equipped,
            use_categories,
            worn: WornSlots::copy(&WornCached {
                store,
                objects,
                items,
            }),
            clock: (self.clock.anchor, self.clock.ui_now),
        }
    }
}

type Bodies<'w, 's> = Query<'w, 's, (&'static ObjectStore, &'static Transform, Has<SelfPlayer>)>;

/// The builder's memory: the spare snapshot it fills next (the one the VM let go of last frame),
/// the shared catalogs re-wrapped only on change, and the trace clock.
#[derive(Default)]
pub(crate) struct BuildMemo {
    spare: Arc<FrameQuery>,
    spells: Option<Arc<Spells>>,
    mods: Option<Arc<SpellModifiers>>,
    last_trace: Option<Instant>,
}

/// Fill the frame's snapshot and swap it into the link.
#[allow(clippy::too_many_arguments)] // a Bevy system's inputs
pub(crate) fn build_game_query(
    link: Res<GameQueryLink>,
    spells: Option<Res<Spells>>,
    mods: Option<Res<SpellModifiers>>,
    range_units: RangeUnits,
    me: Query<(&ObjectStore, &Transform), With<SelfPlayer>>,
    index: Option<Res<GuidIndex>>,
    bodies: Bodies,
    walk: WalkInputs,
    mut memo: Local<BuildMemo>,
) {
    let started = Instant::now();
    match spells {
        Some(s) if s.is_changed() || memo.spells.is_none() => {
            memo.spells = Some(Arc::new(s.clone()));
        }
        Some(_) => {}
        None => memo.spells = None,
    }
    if let Some(m) = mods.filter(|m| m.is_changed() || memo.mods.is_none()) {
        memo.mods = Some(Arc::new(m.clone()));
    }
    if Arc::get_mut(&mut memo.spare).is_none() {
        memo.spare = Arc::default(); // a caller still holds last frame's: start a fresh one
    }
    let (shared_spells, shared_mods) = (memo.spells.clone(), memo.mods.clone());
    let frame = Arc::get_mut(&mut memo.spare).expect("unshared");
    frame.spells = shared_spells;
    frame.mods = shared_mods.unwrap_or_default();
    let caster = range_units.caster();
    frame.me = (me.iter().next()).map(|(s, t)| walk.me(s, &bodies, t.translation, caster));
    frame.units.clear();
    for (&guid, &e) in index.iter().flat_map(|i| i.0.iter()) {
        let Ok((store, t, _)) = bodies.get(e) else {
            continue;
        };
        // Units and players: a game object, corpse or dynamic object is never a spell's unit.
        use ObjectType::{Corpse, DynamicObject, GameObject};
        if matches!(
            store.0.object_type(),
            Some(GameObject | DynamicObject | Corpse)
        ) {
            continue;
        }
        if let Some(range) = range_units.unit(e) {
            let pos = t.translation;
            frame.units.insert(guid, UnitRow { pos, range });
        }
    }
    let rows = frame.units.len();
    std::mem::swap(&mut *link.0.lock_recover(), &mut memo.spare);
    // The builder's cost (`WOW_MOVE_TRACE` sink, tag "qry"): once per second.
    if wrathplus_assets::trace::enabled()
        && memo
            .last_trace
            .is_none_or(|t| started.duration_since(t).as_secs_f32() >= 1.0)
    {
        memo.last_trace = Some(started);
        let us = started.elapsed().as_secs_f64() * 1e6;
        wrathplus_assets::trace::line("qry", &format!("build us={us:.1} units={rows}"));
    }
}

/// Seat the provider in a new VM, when the app runs the builder (`new_vm`'s every VM).
pub(crate) fn seed(world: &World, script: &mut UiScript) {
    if let Some(link) = world.get_resource::<GameQueryLink>() {
        script.set_game_query(Box::new(link.clone()));
    }
}

/// The resource, the builder and its place in the frame.
pub(crate) fn build(app: &mut App) {
    use crate::app_core::schedule::UiFeed;
    app.init_resource::<GameQueryLink>()
        .configure_sets(
            Update,
            GameQueryBuild
                .after(wrathplus_world::schedule::WorldStage::Net)
                // The relation inputs' vehicle state, derived from this frame's packets.
                .after(crate::net::wrath_vehicle::derive_vehicle_state)
                .before(UiFeed),
        )
        .add_systems(Update, build_game_query.in_set(GameQueryBuild));
}
