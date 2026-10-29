//! The tutorial system (the reference's `[0x4b5150, 0x4b5a60)`) behind the stock
//! `TutorialFrame.xml`: two bit banks, the fire-once trigger, the acknowledge-and-send setter,
//! the timers, the popup sound and the trigger sites.
//!
//! Both banks (`0xb711b8`, `0xb711e4`) are filled from the same `SMSG_TUTORIAL_FLAGS`; until it
//! lands nothing fires. Bank A is fire-once: [`Tutorials::trigger`] tests and sets it and raises
//! `TUTORIAL_TRIGGER(id + 1)`, never sending. Bank B is acknowledged: [`Tutorials::acknowledge`]
//! tests it, sets both banks, cancels the id's timer and sends `CMSG_TUTORIAL_FLAG`. So A ⊇ B, and
//! doing the thing suppresses its tutorial, account-wide.
//!
//! `TriggerTutorial(id, delayMs)` (`0x4b5390`): with a zero delay the `TutorialPopup` cue plays and
//! the event fires now; otherwise a timer holds it (the 10 s Targeting popup is the one site). The
//! reference bounds-checks nothing; an id past the bank is a no-op here.
//!
//! The reference has fifty-one trigger sites and seven acknowledge sites (the callers of
//! `0x4b5390` and `0x4b54c0`); the ones this app can produce reach here through
//! [`TutorialEvent`] or the self-descriptor edges below.

use std::time::{Duration, Instant};

use bevy::prelude::*;

use wrathplus_ui::script::{ScriptValue, UiScript, WrathTutorialRequest};

use crate::app_core::player::Player;
use crate::app_core::schedule::UiFeed;
use crate::app_core::schedule::UiInput;
use crate::app_core::session::EnteredWorldMessage;
use crate::app_core::session::ObjectStore;
use crate::app_core::session::SelfPlayer;
use crate::app_core::state::ClientState;
use crate::net::{ClientCommand, NetCommands};

/// The popup cue `0x4b5390` plays on both legs (`0x846b68`).
const TUTORIAL_POPUP_SOUND: &str = "TutorialPopup";

#[cfg(test)]
use crate::app_core::tutorial::TARGETING_DELAY_MS;
pub(crate) use crate::app_core::tutorial::{id, WorldEnterCascade};

/// The level-gated arm of the `SMSG_LEVELUP_INFO` handler (`0x5e38c0`): in the handler's own order,
/// each fires when the new level is at least its threshold.
const LEVEL_TRIGGERS: [(u32, u32); 8] = [
    (3, 0x16),  // Chatting
    (4, 0x0d),  // Trainers
    (4, 0x02),  // Cameras
    (5, 0x26),  // Groups
    (7, 0x15),  // Friends
    (7, 0x25),  // Professions
    (8, 0x20),  // Jumping
    (10, 0x0c), // Learning Talents
];

/// The Movement popup's silence window (`0x482ff7`: elapsed - stamp - 90000 >= 0), taken as 90 s
/// after world enter with no movement input; what the stamp (`0x5143e0`) holds is untraced.
const MOVEMENT_SILENCE: Duration = Duration::from_millis(90_000);

pub(crate) use crate::app_core::state::TutorialEvent;

/// A send the drains owe the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TutorialSend {
    Flag(u32),
    Clear,
    Reset,
}

/// A bank bit, byte-wise: the client's `id >> 5` dword indexing on little-endian dwords.
fn bank_bit(bank: &[u8], id: u32) -> Option<bool> {
    bank.get((id >> 3) as usize)
        .map(|b| b & (1 << (id & 7)) != 0)
}

fn set_bank_bit(bank: &mut [u8], id: u32) {
    if let Some(b) = bank.get_mut((id >> 3) as usize) {
        *b |= 1 << (id & 7);
    }
}

/// The two banks, the timers, and what the frame owes the VM and the wire.
#[derive(Resource, Default)]
pub(crate) struct Tutorials {
    /// Bank A (`0xb711b8`…): fire-once. `None` until the packet lands.
    fire_once: Option<Vec<u8>>,
    /// Bank B (`0xb711e4`…): acknowledged.
    acknowledged: Option<Vec<u8>>,
    /// The delayed triggers (`TUTORIALTIMER` nodes): `(id, due)`.
    timers: Vec<(u32, Instant)>,
    /// Published ids (`id + 1`) whose event and cue are owed this frame.
    fired: Vec<u32>,
    sends: Vec<TutorialSend>,
    /// Item pushes for us since the last feed (`0x491a60`'s sites): `(entry, bag, slot)`.
    pushes: Vec<(u32, u8, u32)>,
    /// Packet generation distinguishes an authoritative identical resend from a local echo.
    packet_generation: u64,
    history_appends: Vec<u32>,
}

impl Tutorials {
    /// `SMSG_TUTORIAL_FLAGS`: both banks copied from the payload.
    pub(crate) fn apply_flags(&mut self, bytes: &[u8]) {
        self.packet_generation = self.packet_generation.wrapping_add(1);
        self.fire_once = Some(bytes.to_vec());
        self.acknowledged = Some(bytes.to_vec());
    }

    /// The bring-up `0x4b5330`: both banks zeroed if present (the bit count stays).
    fn world_enter(&mut self) {
        for bank in [&mut self.fire_once, &mut self.acknowledged]
            .into_iter()
            .flatten()
        {
            bank.iter_mut().for_each(|b| *b = 0);
        }
    }

    /// The teardown `0x4b5380`: every pending timer flushed.
    fn world_leave(&mut self) {
        self.timers.clear();
    }

    /// `TriggerTutorial(id, delayMs)` (`0x4b5390`).
    pub(crate) fn trigger(&mut self, id: u32, delay_ms: u32, now: Instant) {
        let Some(bank) = self.fire_once.as_mut() else {
            return; // no bank yet: silent
        };
        match bank_bit(bank, id) {
            Some(false) => set_bank_bit(bank, id),
            _ => return, // already set, or past the bank (the reference would overrun)
        }
        if delay_ms == 0 {
            self.fired.push(id + 1);
        } else {
            self.timers
                .push((id, now + Duration::from_millis(u64::from(delay_ms))));
        }
    }

    /// `SetTutorialFlag(id)` (`0x4b54c0`): bank B's bit gates, both banks set, the id's timer
    /// cancelled, the 0-based id sent. The reference dereferences an absent bank B; here it is a
    /// no-op.
    pub(crate) fn acknowledge(&mut self, id: u32) {
        let Some(bank_b) = self.acknowledged.as_mut() else {
            return;
        };
        if bank_bit(bank_b, id) != Some(false) {
            return;
        }
        self.timers.retain(|(t, _)| *t != id);
        set_bank_bit(bank_b, id);
        if let Some(bank_a) = self.fire_once.as_mut() {
            set_bank_bit(bank_a, id);
        }
        self.sends.push(TutorialSend::Flag(id));
        self.history_appends.push(id);
    }

    /// `ClearTutorials()`: the timers flushed, every bit of both banks set, the empty clear sent.
    fn clear(&mut self) {
        self.timers.clear();
        for bank in [&mut self.fire_once, &mut self.acknowledged]
            .into_iter()
            .flatten()
        {
            bank.iter_mut().for_each(|b| *b = 0xFF);
        }
        self.sends.push(TutorialSend::Clear);
    }

    /// `ResetTutorials()`: every bit of both banks cleared, the timers kept, the empty reset sent.
    fn reset(&mut self) {
        for bank in [&mut self.fire_once, &mut self.acknowledged]
            .into_iter()
            .flatten()
        {
            bank.iter_mut().for_each(|b| *b = 0);
        }
        self.sends.push(TutorialSend::Reset);
        self.history_appends.clear();
    }

    /// `ProcessTutorialTimers`: every due node fires the same cue and event.
    fn tick(&mut self, now: Instant) {
        let mut due = Vec::new();
        self.timers.retain(|&(id, at)| {
            if at <= now {
                due.push(id + 1);
                false
            } else {
                true
            }
        });
        self.fired.extend(due);
    }

    /// `SMSG_ITEM_PUSH_RESULT` for one of ours: the item-received handler's five sites, resolved
    /// on the next feed with the item's template.
    pub(crate) fn item_received(&mut self, entry: u32, bag: u8, slot: u32) {
        self.pushes.push((entry, bag, slot));
    }

    #[cfg(test)]
    fn bits(&self) -> (Option<&[u8]>, Option<&[u8]>) {
        (self.fire_once.as_deref(), self.acknowledged.as_deref())
    }
}

/// The self descriptor's last-seen fields, for the edge-triggered `0x4b5390` sites: the level-up
/// arm, the ghost flag, the resting flag, the PvP flag, the rest state.
#[derive(Default)]
struct SelfWatch {
    level: Option<u32>,
    player_flags: Option<u32>,
    unit_flags: Option<u32>,
    rest_state: Option<u8>,
    swimming: Option<bool>,
}

const PLAYER_FLAGS_GHOST: u32 = 0x10;
const PLAYER_FLAGS_RESTING: u32 = 0x20;
const UNIT_FLAG_PVP_ATTACKABLE: u32 = 0x1000;

/// The hearthstone's item entry (`0x1b24`).
const HEARTHSTONE: u32 = 6948;
const INVTYPE_RANGED: u32 = 15;
const CLASS_HUNTER: u8 = 3;
/// The keyring bag and its slot band (`edi == 0xff`, slot ∈ [0x51, 0x70]).
const KEYRING_BAG: u8 = 0xff;
const KEYRING_SLOTS: std::ops::RangeInclusive<u32> = 0x51..=0x70;

/// Before the script tick: the sites' asks, the descriptor edges, the timers, the cue and the
/// event, and the acknowledged bank's push to whichever VM is live.
#[allow(clippy::type_complexity)] // Bevy system parameters / a closure table, kept inline
fn feed_tutorials(
    script: Option<NonSendMut<UiScript>>,
    mut tutorials: ResMut<Tutorials>,
    mut asks: MessageReader<TutorialEvent>,
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    items: Res<crate::items::Items>,
    mut sounds: ResMut<crate::sound::MessageSounds>,
    mut wrath_told: Local<
        crate::app_core::vm_memo::VmMemo<Option<(u64, Option<Vec<u8>>, Option<Vec<u8>>)>>,
    >,
) {
    let now = Instant::now();
    for ask in asks.read() {
        match *ask {
            TutorialEvent::Trigger { id, delay_ms } => tutorials.trigger(id, delay_ms, now),
            TutorialEvent::Acknowledge { id } => tutorials.acknowledge(id),
        }
    }

    // The item-received sites: Backpack always; Equippable Items for any non-zero inventory type
    // (the reference's 23-bit mask `0x809200` is not decoded); Hearthstones by entry; Ranged
    // Weapons for a ranged type on a non-hunter; Keyrings by the slot band.
    let pushes = std::mem::take(&mut tutorials.pushes);
    let class = self_q.single().ok().and_then(|s| s.0.unit_class());
    for (entry, bag, slot) in pushes {
        tutorials.trigger(id::BACKPACK, 0, now);
        if let Some(info) = items.template_cached(entry) {
            if info.inventory_type != 0 {
                tutorials.trigger(id::EQUIPPABLE_ITEMS, 0, now);
            }
            if info.inventory_type == INVTYPE_RANGED && class != Some(CLASS_HUNTER) {
                tutorials.trigger(id::RANGED_WEAPONS, 0, now);
            }
        }
        if entry == HEARTHSTONE {
            tutorials.trigger(id::HEARTHSTONES, 0, now);
        }
        if bag == KEYRING_BAG && KEYRING_SLOTS.contains(&slot) {
            tutorials.trigger(id::KEYRINGS, 0, now);
        }
    }

    tutorials.tick(now);

    let Some(mut script) = script else {
        return;
    };
    // The bank is owed to every VM: what it has been told is per VM, so a `/reload`'s fresh VM
    // is told again.
    let snapshot = (
        tutorials.packet_generation,
        tutorials.fire_once.clone(),
        tutorials.acknowledged.clone(),
    );
    let told = wrath_told.get(&script);
    if told.as_ref() != Some(&snapshot) {
        script.set_wrath_tutorial_banks(snapshot.1.clone(), snapshot.2.clone());
        *told = Some(snapshot);
    }
    for id in std::mem::take(&mut tutorials.history_appends) {
        script.append_wrath_tutorial_history(id);
    }
    for published in std::mem::take(&mut tutorials.fired) {
        sounds.push_cue(TUTORIAL_POPUP_SOUND);
        script.fire_event(
            "TUTORIAL_TRIGGER",
            vec![ScriptValue::Int(i64::from(published))],
        );
    }
}

/// The self descriptor's `0x4b5390` edge sites: the `SMSG_LEVELUP_INFO` arm on the level rising,
/// the ghost and resting flags, the PvP flag, the rested table's one live row, and the mover
/// entering water. Each fires on a change, never on the login descriptor; the bring-up resets the
/// memory.
fn watch_self(
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    player: Res<Player>,
    mut cascade: ResMut<WorldEnterCascade>,
    mut watch: Local<SelfWatch>,
    mut asks: MessageWriter<TutorialEvent>,
) {
    if std::mem::take(&mut cascade.reset_watch) {
        *watch = SelfWatch::default();
    }
    // The first sight of each field arms silently.
    if let Ok(store) = self_q.single() {
        let level = store.0.unit_level();
        let player_flags = store.0.player_flags();
        let unit_flags = store.0.unit_flags();
        let rest_state = store.0.player_rest_state();
        if let (Some(prev), Some(new)) = (watch.level, level) {
            if new > prev {
                for (threshold, id) in LEVEL_TRIGGERS {
                    if new >= threshold {
                        asks.write(TutorialEvent::trigger(id));
                    }
                }
            }
        }
        if let Some(prev) = watch.player_flags {
            let rose = |bit: u32| prev & bit == 0 && player_flags & bit != 0;
            if rose(PLAYER_FLAGS_GHOST) {
                asks.write(TutorialEvent::trigger(id::DEATH));
            }
            if rose(PLAYER_FLAGS_RESTING) {
                asks.write(TutorialEvent::trigger(id::RESTING));
            }
        }
        if let Some(prev) = watch.unit_flags {
            if prev & UNIT_FLAG_PVP_ATTACKABLE == 0 && unit_flags & UNIT_FLAG_PVP_ATTACKABLE != 0 {
                asks.write(TutorialEvent::trigger(id::PVP_COMBAT));
            }
        }
        if let (Some(prev), Some(new)) = (watch.rest_state, rest_state) {
            // Only rest state 1 names a tutorial; the other rows carry the sentinel.
            if new != prev && new == 1 {
                asks.write(TutorialEvent::trigger(id::RESTED));
            }
        }
        watch.level = level;
        watch.player_flags = Some(player_flags);
        watch.unit_flags = Some(unit_flags);
        watch.rest_state = rest_state;
    }
    if let Some(prev) = watch.swimming {
        if !prev && player.swimming {
            asks.write(TutorialEvent::trigger(id::SWIMMING));
        }
    }
    watch.swimming = Some(player.swimming);
}

/// After the script tick: the four verbs' writes, then every send.
fn drain_tutorials(
    script: Option<NonSendMut<UiScript>>,
    mut tutorials: ResMut<Tutorials>,
    commands: Res<NetCommands>,
) {
    if let Some(mut script) = script {
        for request in script.take_wrath_tutorial_requests() {
            match request {
                WrathTutorialRequest::Flag(id) => {
                    tutorials.acknowledge(id);
                    // Already appended synchronously in this VM; never re-append after a
                    // later Reset in the same ordered batch.
                    tutorials.history_appends.retain(|&pending| pending != id);
                }
                WrathTutorialRequest::Clear => tutorials.clear(),
                WrathTutorialRequest::Reset => tutorials.reset(),
            }
        }
        for id in script.take_tutorial_flag_requests() {
            tutorials.acknowledge(id);
        }
        for _ in 0..script.take_tutorial_clears() {
            tutorials.clear();
        }
        for _ in 0..script.take_tutorial_resets() {
            tutorials.reset();
        }
    }
    for send in std::mem::take(&mut tutorials.sends) {
        let command = match send {
            TutorialSend::Flag(id) => ClientCommand::TutorialFlag { id },
            TutorialSend::Clear => ClientCommand::TutorialClear,
            TutorialSend::Reset => ClientCommand::TutorialReset,
        };
        let _ = commands.0.send(command);
    }
}

/// The world-enter bring-up: the banks zeroed, then the bank captured during the login handshake
/// applied if there was one.
fn on_world_enter(
    mut entered: MessageReader<EnteredWorldMessage>,
    mut tutorials: ResMut<Tutorials>,
    mut cascade: ResMut<WorldEnterCascade>,
) {
    for e in entered.read() {
        tutorials.world_enter();
        if let Some(bytes) = &e.tutorial_flags {
            tutorials.apply_flags(bytes);
        }
        cascade.armed = true;
        cascade.entered_at = Some(Instant::now());
        cascade.moved = false;
        cascade.reset_watch = true;
    }
}

fn run_world_enter_cascade(
    mut cascade: ResMut<WorldEnterCascade>,
    mut tutorials: ResMut<Tutorials>,
    self_q: Query<(), With<SelfPlayer>>,
) {
    let now = Instant::now();
    if cascade.armed && !self_q.is_empty() {
        cascade.armed = false;
        tutorials.trigger(id::WELCOME, 0, now);
        tutorials.trigger(id::QUESTGIVERS, 0, now);
    }
    if let Some(at) = cascade.entered_at {
        if !cascade.moved && now.duration_since(at) >= MOVEMENT_SILENCE {
            cascade.entered_at = None;
            tutorials.trigger(id::MOVEMENT, 0, now);
        }
    }
}

/// The window edges the reference's handlers fire from: the trainer list (Trainers acknowledged,
/// `0x4d74ae`), the vendor list (Vendors, `0x4fad32`), the taxi map (Travel, `0x4dbaa2`), and the
/// group turning raid (Raid Groups, `0x4ba60d`, taken as the raid-type edge).
#[derive(Default)]
struct WindowWatch {
    trainer: Option<u64>,
    vendor: Option<u64>,
    taxi: bool,
    raid: Option<bool>,
}

fn watch_windows(
    trainer: Res<crate::app_core::interaction::TrainerOpen>,
    merchant: Res<crate::app_core::interaction::MerchantOpen>,
    taxi: Res<crate::ui_taxi::TaxiState>,
    group: Res<crate::app_core::group::GroupState>,
    mut watch: Local<WindowWatch>,
    mut asks: MessageWriter<TutorialEvent>,
) {
    if trainer.trainer.is_some() && trainer.trainer != watch.trainer {
        asks.write(TutorialEvent::Acknowledge { id: id::TRAINERS });
    }
    watch.trainer = trainer.trainer;
    if merchant.vendor.is_some() && merchant.vendor != watch.vendor {
        asks.write(TutorialEvent::trigger(id::VENDORS));
    }
    watch.vendor = merchant.vendor;
    let taxi_open = taxi.open.is_some();
    if taxi_open && !watch.taxi {
        asks.write(TutorialEvent::trigger(id::TRAVEL));
    }
    watch.taxi = taxi_open;
    let raid = group.in_group && group.group_type == crate::app_core::state::GROUPTYPE_RAID;
    if raid && watch.raid == Some(false) {
        asks.write(TutorialEvent::trigger(id::RAID_GROUPS));
    }
    watch.raid = Some(raid);
}

fn on_world_leave(mut tutorials: ResMut<Tutorials>, mut cascade: ResMut<WorldEnterCascade>) {
    tutorials.world_leave();
    cascade.armed = false;
    cascade.entered_at = None;
}

pub struct TutorialPlugin;

impl Plugin for TutorialPlugin {
    fn build(&self, app: &mut App) {
        net::register(app);
        app.init_resource::<Tutorials>()
            .init_resource::<WorldEnterCascade>()
            .add_message::<TutorialEvent>()
            .add_systems(
                Update,
                (
                    on_world_enter.before(run_world_enter_cascade),
                    run_world_enter_cascade.before(feed_tutorials),
                    watch_windows.before(feed_tutorials),
                    watch_self.after(on_world_enter).before(feed_tutorials),
                    // Not before the in-game UI exists: `fired` is taken once, with no `VmMemo`
                    // behind it, so the login's Welcome and Questgivers fired into the boot VM
                    // during the one frame it is still live in the world would be lost. `fired`
                    // simply waits for the first frame with an interface.
                    feed_tutorials
                        .in_set(UiFeed)
                        .run_if(crate::app_core::ui_state::ingame_ui_up),
                    drain_tutorials.after(UiInput),
                ),
            )
            .add_systems(OnExit(ClientState::InWorld), on_world_leave);
    }
}

/// The tutorial flags' packet handler.
mod net {
    use bevy::prelude::*;
    use wrathplus_protocol::{SessionEvent, SessionEventKind};

    use super::Tutorials;
    use crate::net::NetHandlerApp;

    /// Register the handler, from [`super::TutorialPlugin`].
    pub(super) fn register(app: &mut App) {
        app.net_handler(SessionEventKind::TutorialFlags, on_flags);
    }

    fn on_flags(In(ev): In<SessionEvent>, mut tutorials: ResMut<Tutorials>) {
        if let SessionEvent::TutorialFlags(bytes) = ev {
            tutorials.apply_flags(&bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn banked() -> Tutorials {
        let mut t = Tutorials::default();
        t.apply_flags(&[0u8; 32]);
        t
    }

    /// No bank: silent. A clear bit: set in A only, the event owed, nothing sent. Twice: once.
    #[test]
    fn the_trigger_fires_once_per_bank_a_and_never_sends() {
        let now = Instant::now();
        let mut t = Tutorials::default();
        t.trigger(id::WELCOME, 0, now);
        assert!(t.fired.is_empty(), "no bank yet: silent");
        let mut t = banked();
        t.trigger(id::WELCOME, 0, now);
        t.trigger(id::WELCOME, 0, now);
        assert_eq!(t.fired, vec![id::WELCOME + 1], "published as id + 1, once");
        let (a, b) = t.bits();
        assert_eq!(bank_bit(a.unwrap(), id::WELCOME), Some(true));
        assert_eq!(
            bank_bit(b.unwrap(), id::WELCOME),
            Some(false),
            "bank B untouched"
        );
        assert!(t.sends.is_empty(), "the trigger sends nothing");
        t.trigger(300, 0, now);
        assert_eq!(t.fired.len(), 1, "past the bank: a no-op");
    }

    /// The delayed leg holds the id until due; an acknowledge cancels it.
    #[test]
    fn a_delayed_trigger_waits_and_an_acknowledge_cancels_it() {
        let now = Instant::now();
        let mut t = banked();
        t.trigger(id::TARGETING, TARGETING_DELAY_MS, now);
        assert!(t.fired.is_empty());
        t.tick(now + Duration::from_millis(9_999));
        assert!(t.fired.is_empty(), "not yet");
        t.tick(now + Duration::from_millis(10_000));
        assert_eq!(t.fired, vec![id::TARGETING + 1]);

        let mut t = banked();
        t.trigger(id::TARGETING, TARGETING_DELAY_MS, now);
        t.acknowledge(id::TARGETING);
        t.tick(now + Duration::from_millis(20_000));
        assert!(t.fired.is_empty(), "the acknowledge cancelled the timer");
        assert_eq!(t.sends, vec![TutorialSend::Flag(id::TARGETING)]);
    }

    /// The acknowledge: bank B gates, both banks set, one send; a second is silent; absent bank
    /// B is a no-op.
    #[test]
    fn the_acknowledge_sets_both_banks_and_sends_once() {
        let mut t = Tutorials::default();
        t.acknowledge(id::CHATTING);
        assert!(t.sends.is_empty(), "no bank: nothing");
        let mut t = banked();
        t.acknowledge(id::CHATTING);
        t.acknowledge(id::CHATTING);
        assert_eq!(t.sends, vec![TutorialSend::Flag(id::CHATTING)]);
        let (a, b) = t.bits();
        assert_eq!(bank_bit(a.unwrap(), id::CHATTING), Some(true));
        assert_eq!(bank_bit(b.unwrap(), id::CHATTING), Some(true));
        // Acknowledged first: the later trigger is silent (A ⊇ B).
        t.trigger(id::CHATTING, 0, Instant::now());
        assert!(t.fired.is_empty());
    }

    /// Clear sets every bit and flushes the timers; Reset clears every bit and keeps them.
    #[test]
    fn clear_and_reset_write_both_banks() {
        let now = Instant::now();
        let mut t = banked();
        t.trigger(id::TARGETING, TARGETING_DELAY_MS, now);
        t.clear();
        assert!(t.timers.is_empty(), "clear flushes the timers");
        let (a, b) = t.bits();
        assert!(a.unwrap().iter().all(|&x| x == 0xFF) && b.unwrap().iter().all(|&x| x == 0xFF));
        t.reset();
        t.trigger(id::TARGETING, TARGETING_DELAY_MS, now);
        assert_eq!(
            t.timers.len(),
            1,
            "a cleared bank lets the trigger through again"
        );
        t.reset();
        assert_eq!(t.timers.len(), 1, "reset does NOT flush the timers");
        let (a, b) = t.bits();
        assert!(a.unwrap().iter().all(|&x| x == 0) && b.unwrap().iter().all(|&x| x == 0));
        assert_eq!(
            t.sends,
            vec![
                TutorialSend::Clear,
                TutorialSend::Reset,
                TutorialSend::Reset
            ]
        );
    }

    #[test]
    fn wrath_ordered_drain_identical_resend_and_fresh_vm() {
        let mut app = feeder();
        let (tx, rx) = crossbeam_channel::unbounded();
        app.insert_resource(NetCommands(tx));
        app.add_systems(Update, drain_tutorials.after(feed_tutorials));
        app.world_mut()
            .resource_mut::<Tutorials>()
            .apply_flags(&[0; 32]);
        app.insert_non_send_resource(UiScript::new().unwrap());
        app.update();
        app.world()
            .non_send_resource::<UiScript>()
            .run("FlagTutorial(3); ClearTutorials(); ResetTutorials(); FlagTutorial(17)")
            .unwrap();
        app.update();
        let commands: Vec<_> = rx.try_iter().collect();
        assert_eq!(commands.len(), 4);
        assert!(matches!(commands[0], ClientCommand::TutorialFlag { id: 2 }));
        assert!(matches!(commands[1], ClientCommand::TutorialClear));
        assert!(matches!(commands[2], ClientCommand::TutorialReset));
        assert!(matches!(
            commands[3],
            ClientCommand::TutorialFlag { id: 16 }
        ));
        app.update();
        app.world()
            .non_send_resource::<UiScript>()
            .run("assert(IsTutorialFlagged(17)==1)")
            .unwrap();
        // Identical authoritative payloads still replace a VM-only optimistic mutation.
        app.world_mut()
            .resource_mut::<Tutorials>()
            .apply_flags(&[0; 32]);
        app.update();
        app.world()
            .non_send_resource::<UiScript>()
            .run("assert(IsTutorialFlagged(17)==nil)")
            .unwrap();
        app.insert_non_send_resource(UiScript::new().unwrap());
        app.update();
        app.world().non_send_resource::<UiScript>().run("assert(IsTutorialFlagged(17)==nil); FlagTutorial(1); assert(IsTutorialFlagged(1)==1)").unwrap();
    }

    #[test]
    fn wrath_session_end_restores_history_order_without_generic_cvar_fold() {
        let mut world = World::new();
        world.insert_resource(wrathplus_assets::ClientDataProfile(
            wrathplus_formats::DataProfile::Wrath {
                locale: "enUS".into(),
            },
        ));
        let mut cvars = crate::cvars::Cvars::default();
        cvars.register_wrath_tutorial_history();
        let mut script = UiScript::new().unwrap();
        script.seed_cvars(cvars.vm_seed());
        script.set_wrath_tutorial_banks(Some(vec![0; 32]), Some(vec![0; 32]));
        script.run("FlagTutorial(42); FlagTutorial(3)").unwrap();
        assert_eq!(cvars.get("flaggedTutorials"), Some(""));
        world.insert_resource(cvars);
        world.insert_non_send_resource(script);
        crate::ui_script::end_ui_session(&mut world);
        let saved = world
            .resource::<crate::cvars::Cvars>()
            .get("flaggedTutorials")
            .unwrap();
        assert!(saved.starts_with("v\x01"));
        let seed = world.resource::<crate::cvars::Cvars>().vm_seed();
        let fresh = world.non_send_resource_mut::<UiScript>();
        fresh.seed_cvars(seed);
        fresh.run("assert(GetNextCompleatedTutorial(42)==3); assert(GetPrevCompleatedTutorial(3)==42)").unwrap();
    }

    /// World enter zeroes present banks and keeps their size; the packet fills both.
    #[test]
    fn world_enter_zeroes_and_the_packet_fills() {
        let mut t = Tutorials::default();
        t.world_enter();
        assert!(t.fire_once.is_none(), "no bank: nothing to zero");
        t.apply_flags(&[0xFFu8; 4]);
        t.world_enter();
        let (a, b) = t.bits();
        assert_eq!(a.unwrap(), &[0u8; 4]);
        assert_eq!(b.unwrap(), &[0u8; 4]);
        assert_eq!(bank_bit(a.unwrap(), 31), Some(false));
        assert_eq!(bank_bit(a.unwrap(), 32), None, "past a 4-byte bank");
    }

    /// A bare app around [`feed_tutorials`], the one system that owes the VM the bank.
    fn feeder() -> App {
        let mut app = App::new();
        app.init_resource::<Tutorials>()
            .init_resource::<crate::items::Items>()
            .init_resource::<crate::sound::MessageSounds>()
            .add_message::<TutorialEvent>()
            .add_systems(Update, feed_tutorials);
        app
    }

    fn enabled(app: &mut App) -> bool {
        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .eval::<bool>("return TutorialsEnabled() ~= nil")
            .unwrap()
    }

    /// A `/reload`'s fresh VM is told the bank again, though no world entry refills it. Untold,
    /// `TutorialsEnabled()` answers nil, the Show Tutorials row loads off, and ticking it passes
    /// `UIOptionsFrame.lua:319`'s `~=` guard into `ResetTutorials()`, re-arming every dismissed
    /// popup account-wide.
    #[test]
    fn a_rebuilt_vm_is_told_the_acknowledged_bank_again() {
        let mut app = feeder();
        // One unacknowledged bit is all `TutorialsEnabled()` scans for.
        let mut bank = vec![0xFFu8; 32];
        bank[0] = 0xFE;
        app.world_mut()
            .resource_mut::<Tutorials>()
            .apply_flags(&bank);

        app.insert_non_send_resource(UiScript::new().expect("VM"));
        app.update();
        assert!(
            enabled(&mut app),
            "the VM that was live when the bank landed"
        );

        // `ReloadUI()`: a fresh VM, and nothing touches the app's banks.
        app.insert_non_send_resource(UiScript::new().expect("VM"));
        assert!(
            !enabled(&mut app),
            "a fresh VM starts knowing no bank — this is the state the row used to load in"
        );
        app.update();
        assert!(enabled(&mut app), "…and must be told it again");
    }

    /// The push is per-VM, not per-frame: a steady session hands the bank over once.
    #[test]
    fn an_unchanged_bank_is_not_re_pushed_every_frame() {
        let mut app = feeder();
        app.world_mut()
            .resource_mut::<Tutorials>()
            .apply_flags(&[0u8; 32]);
        app.insert_non_send_resource(UiScript::new().expect("VM"));
        app.update();
        // Cleared behind the feed's back, a bank the VM was told stays cleared.
        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .set_tutorial_bank(None);
        app.update();
        assert!(!enabled(&mut app), "unchanged ⇒ nothing pushed");
        // A real change is still pushed, on the same VM.
        app.world_mut()
            .resource_mut::<Tutorials>()
            .acknowledge(id::CHATTING);
        app.update();
        assert!(enabled(&mut app), "a moved bank reaches the live VM");
    }

    /// The login's own tutorials wait for an interface, on the real plugin's registration:
    /// `TUTORIAL_TRIGGER` is fired once, so one published to a frameless VM is gone for the
    /// session.
    #[test]
    fn the_world_enter_tutorials_are_not_fired_into_a_ui_less_vm() {
        let mut app = App::new();
        app.add_plugins(TutorialPlugin)
            .init_resource::<crate::items::Items>()
            .init_resource::<crate::sound::MessageSounds>()
            // The other watchers' inputs, so the real plugin can run.
            .init_resource::<crate::app_core::interaction::TrainerOpen>()
            .init_resource::<crate::app_core::interaction::MerchantOpen>()
            .init_resource::<crate::ui_taxi::TaxiState>()
            .init_resource::<crate::app_core::group::GroupState>()
            .init_resource::<Player>()
            .add_message::<EnteredWorldMessage>();
        let (tx, _rx) = crossbeam_channel::unbounded();
        app.insert_resource(crate::net::NetCommands(tx));

        // The deferral window: in the world on the wire, the entry load still owed, and a live
        // boot VM with no frames.
        app.insert_resource(State::new(crate::app_core::state::ClientState::InWorld));
        app.insert_resource(crate::app_core::ui_flags::PendingEntryUiLoad);
        app.insert_non_send_resource(UiScript::new().expect("the boot VM"));

        {
            let mut t = app.world_mut().resource_mut::<Tutorials>();
            t.apply_flags(&[0u8; 32]);
            t.trigger(id::WELCOME, 0, Instant::now());
        }

        app.update();
        assert_eq!(
            app.world().resource::<Tutorials>().fired,
            vec![id::WELCOME + 1],
            "the window must not publish: there is no frame registered for TUTORIAL_TRIGGER, and \
             the take is one-way"
        );

        // The interface comes up; the same frame delivers what was waiting.
        app.world_mut()
            .remove_resource::<crate::app_core::ui_flags::PendingEntryUiLoad>();
        app.update();
        assert!(
            app.world().resource::<Tutorials>().fired.is_empty(),
            "…and once there is an interface, it is published exactly once"
        );
    }
}
