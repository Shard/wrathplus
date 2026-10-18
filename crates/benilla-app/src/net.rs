//! The network-to-ECS bridge. A read thread owns the blocking [`WorldSession`] and streams
//! [`SessionEvent`]s; a write thread drains [`ClientCommand`]s. [`apply_net_updates`] turns the
//! events into real entities keyed by [`Guid`] through [`GuidIndex`]; movement paths become
//! [`Spline`]s, the server clock [`ServerTime`], one-shot directives Bevy [`Message`]s.
//!
//! It runs in [`WorldStage::Net`], before `Input`, so a server teleport snaps, streams and covers
//! in one frame. Coordinates cross from WoW into Bevy space here (`wow_to_bevy`).

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use benilla_protocol::{
    messages::WhoRequest, EntityKind, JumpInfo, MoveMode, MoveSpeeds, ObjectFields, SessionEvent,
    SpeedKind, TransportPose,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};

use benilla_world::schedule::WorldStage;

mod apply;
pub(crate) mod handlers;
pub(crate) mod io;
mod motion;
mod objects;
pub(crate) mod wrath_melee;
pub(crate) use objects::tear_down;
mod session;
mod world;
pub(crate) mod wrath_auras;
mod wrath_exchange_capture;
pub(crate) mod wrath_instance_control_capture;
mod wrath_inventory_capture;
mod wrath_achievements;
mod wrath_equipment_sets;
mod wrath_item_refunds;
mod wrath_sockets;
mod wrath_mail_capture;
mod wrath_mirror_image_capture;
pub(crate) mod wrath_notices;
mod wrath_quests_completed_capture;
mod wrath_achievement_capture;
mod wrath_appearance;
pub(crate) mod wrath_social_lists_capture;
mod wrath_raid_browser_feed;
pub(crate) mod wrath_calendar_capture;
mod wrath_quest_capture;
mod wrath_chat_notice_capture;
mod wrath_battleground_capture;
mod wrath_death_capture;
mod wrath_session_meta_capture;
mod wrath_help_capture;
pub(crate) mod wrath_refer_friend_capture;
mod wrath_world_objects_capture;
mod wrath_session_misc_capture;
pub(crate) mod wrath_inspect_capture;
pub(crate) mod wrath_char_service_capture;
mod wrath_faction_capture;
mod wrath_spell_log_capture;
mod wrath_spell_control_capture;
mod wrath_movement_capture;
mod wrath_bootstrap_capture;
pub(crate) mod wrath_vehicle;
pub(crate) mod wrath_combo_points;
pub(crate) mod wrath_runes;
#[cfg(test)]
mod wrath_stream_tests;
mod wrath_threat;
mod wrath_ui_timer_capture;

pub(crate) use apply::apply_net_updates;
use apply::tag_self_player;
pub(crate) use handlers::NetHandlerApp;

pub(crate) use io::LoginRequest;
use motion::{
    drain_pending_moves, extrapolate_remote_units, ground_clamp_creatures, mark_swimming_creatures,
    sample_splines,
};
// `pub(crate)`: `creature_anim` reads the shuffle latch it produces, and a test runs both.
pub(crate) use motion::drive_display_facing;
pub(crate) use motion::{
    ground_derived, grounded_y, jump_seed, CreatureSwimming, FacingStep, RemoteMotion, Spline,
    SplineStopped, UnitMoveModes,
};
// Only the ground-census probe reads this, so a build without the instruments leaves it unused.
// Not a `cfg`: seam knowledge lives in three fixed places, and this file is not one of them.
#[allow(unused_imports)]
pub(crate) use motion::GroundClamped;

/// The net subsystem: spawns the background IO threads and drives the per-frame event drain.
pub(crate) struct NetPlugin {
    /// `false` in capture mode ([`crate::capture`]): the channels exist, sends are no-ops, and no
    /// IO thread runs.
    pub(crate) connect: bool,
}

/// The wire/snapshot dialect fixed when the app is built. It is copied into every login request,
/// so a changed environment cannot silently cross an asset/network dialect boundary mid-session.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionDialect {
    Vanilla5875,
    Wrath12340,
}

impl SessionDialect {
    fn from_profile(profile: &benilla_formats::DataProfile) -> Self {
        match profile {
            benilla_formats::DataProfile::Vanilla => Self::Vanilla5875,
            benilla_formats::DataProfile::Wrath { .. } => Self::Wrath12340,
        }
    }
}

/// This process has no IO thread ([`NetPlugin::connect`] false); `crate::preflight` announces it.
/// Not [`NetStatus`]'s `connected`, which a dropped connection also clears.
#[derive(Resource)]
pub(crate) struct NetOffline;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        // The release-on-enter for the ask-once caches this plugin owns.
        crate::query_cache::register::<crate::names::NameCache>(app);
        crate::query_cache::register::<crate::go_templates::GameObjectTemplates>(app);
        crate::query_cache::register::<crate::items::Items>(app);
        let profile = benilla_formats::DataProfile::from_env()
            .expect("valid explicit WOW_CLIENT_PROFILE/WOW_LOCALE network configuration");
        let dialect = SessionDialect::from_profile(&profile);
        if let Some(snapshot) = app
            .world()
            .get_resource::<benilla_assets::ClientDataProfile>()
        {
            assert_eq!(
                dialect,
                SessionDialect::from_profile(&snapshot.0),
                "network backend/data snapshot dialect mismatch"
            );
        }
        let handles = io::spawn_net(self.connect, dialect);
        if !self.connect {
            app.insert_resource(NetOffline);
        }
        // In the net stage: the lighting resolve runs after it and reads this frame's clock.
        app.add_systems(
            Update,
            publish_world_time.in_set(benilla_world::schedule::WorldStage::Net),
        );
        world::register(app);
        // This first Connected handler is the ECS publication barrier for the new immutable sender.
        app.net_handler(
            benilla_protocol::SessionEventKind::Connected,
            activate_command_endpoint,
        );
        session::register(app);
        objects::register(app);
        wrath_auras::register(app);
        wrath_notices::register(app);
        wrath_mail_capture::register(app);
        wrath_quests_completed_capture::register(app);
        wrath_ui_timer_capture::register(app);
        wrath_mirror_image_capture::register(app);
        wrath_instance_control_capture::register(app);
        wrath_inventory_capture::register(app);
        wrath_exchange_capture::register(app);
        wrath_achievement_capture::register(app);
        wrath_appearance::register(app);
        wrath_social_lists_capture::register(app);
        wrath_raid_browser_feed::register(app);
        wrath_calendar_capture::register(app);
        wrath_quest_capture::register(app);
        wrath_chat_notice_capture::register(app);
        wrath_battleground_capture::register(app);
        wrath_death_capture::register(app);
        wrath_session_meta_capture::register(app);
        wrath_help_capture::register(app);
        wrath_refer_friend_capture::register(app);
        wrath_world_objects_capture::register(app);
        wrath_session_misc_capture::register(app);
        wrath_inspect_capture::register(app);
        wrath_char_service_capture::register(app);
        wrath_faction_capture::register(app);
        wrath_spell_log_capture::register(app);
        wrath_spell_control_capture::register(app);
        wrath_movement_capture::register(app);
        wrath_bootstrap_capture::register(app);
        wrath_vehicle::register(app);
        wrath_combo_points::register(app);
        wrath_runes::register(app);
        wrath_threat::register(app);
        crate::names::net::register(app);
        app.insert_resource(dialect)
            .insert_resource(NetEvents(handles.events))
            .insert_resource(handles.commands)
            .insert_resource(CommandEndpoints(handles.command_endpoints))
            .insert_resource(CharPick(handles.pick))
            .insert_resource(RealmChoice(handles.realm))
            .insert_resource(LoginSubmit(handles.login))
            .insert_resource(LoginAbandon(handles.login_abandon))
            .insert_resource(PingShared(handles.ping))
            .init_resource::<GuidIndex>()
            .init_resource::<SelfGuid>()
            .init_resource::<AddonInfoReply>()
            .init_resource::<PendingTransfer>()
            .init_resource::<NetStatus>()
            .init_resource::<DroppedOpcodes>()
            .init_resource::<ServerTime>()
            .init_resource::<ServerWallClock>()
            .init_resource::<Reputations>()
            .init_resource::<HomeBind>()
            .init_resource::<PlayedTimeAnswer>()
            .init_resource::<Proficiencies>()
            .init_resource::<crate::names::NameCache>()
            .init_resource::<crate::go_templates::GameObjectTemplates>()
            .init_resource::<crate::items::Items>()
            .init_resource::<crate::world_state::WorldStates>()
            .add_message::<TeleportMessage>()
            .add_message::<FieldChanged>()
            .add_message::<SelfMoveMessage>()
            .add_message::<SpeedChangeMessage>()
            .add_message::<ClientControlMessage>()
            .add_message::<MoveModeMessage>()
            .add_message::<KnockBackMessage>()
            .add_message::<ServerSoundMessage>()
            .add_message::<EmoteMessage>()
            .add_message::<AiReactionMessage>()
            .add_message::<PetTalkMessage>()
            .add_message::<PetDismissSoundMessage>()
            .add_message::<WorldportMessage>()
            .add_message::<RealmListMessage>()
            .add_message::<CharListMessage>()
            .add_message::<CharActionResultMessage>()
            .add_message::<WrathCharActionResultMessage>()
            .add_message::<CharacterLoginFailedMessage>()
            .add_message::<EnteredWorldMessage>()
            .add_message::<CinematicTriggeredMessage>()
            .add_message::<ServerSaidMessage>()
            .add_message::<LoggedOutMessage>()
            .add_message::<LoginStageMessage>()
            .add_message::<LoginQueuedMessage>()
            .add_message::<LoginFailedMessage>()
            .add_message::<DisconnectedMessage>()
            .add_systems(
                Update,
                (
                    apply_net_updates,
                    tag_self_player,
                    sample_splines,
                    // Swim state from the water at the feet (the wire never carries it for
                    // creatures), before the clamp so a swimmer is exempt the same frame.
                    mark_swimming_creatures,
                    // After the raw spline Z: re-ground walkers (the client discards a ground
                    // spline's Z); a swimmer keeps its wire Z.
                    ground_clamp_creatures,
                    // Due relays fire before the extrapolator advances the fresh state.
                    drain_pending_moves,
                    extrapolate_remote_units,
                    // After the movers, so a turn reads the goal's position this frame.
                    drive_display_facing,
                )
                    .chain()
                    .in_set(WorldStage::Net),
                // `ui_session::feed_interact_npc` seats itself inside this chain, after
                // `apply_net_updates` and before `drive_display_facing`, so the `"npc"` token is
                // current when a window's show handler reads it.
            )
            // Not part of the movement chain above: one send on the world-enter message.
            .add_systems(Update, send_query_time.in_set(WorldStage::Net))
            .add_systems(Update, population_pulse.in_set(WorldStage::Net));
    }
}

/// Trace tag `pop`: once a second, the count of entities in the object index, which shows whether
/// the world arrives after a teleport or waits for the server's ~20 s relocation timer.
fn population_pulse(index: Res<GuidIndex>, time: Res<Time>, mut last: Local<f32>) {
    if !benilla_assets::trace::enabled_for("pop") {
        return;
    }
    let now = time.elapsed_secs();
    if now - *last < 1.0 {
        return;
    }
    *last = now;
    benilla_assets::trace::line("pop", &format!("net entities={}", index.0.len()));
}

// ── ECS state: components ────────────────────────────────────────────────────────────────────────

/// The server guid of a streamed entity.
#[derive(Component, Clone, Copy)]
pub(crate) struct Guid(pub(crate) u64);

/// A streamed entity's kind and display id, from which [`crate::entities`] builds its visual.
#[derive(Component)]
pub(crate) struct NetEntity {
    pub(crate) kind: EntityKind,
    pub(crate) display_id: Option<u32>,
    /// `OBJECT_FIELD_SCALE_X`, the complete render scale: the server already folds in the DBC
    /// scale, so it is never multiplied by ours. A live change eases over 2 s with a cosine
    /// smoothstep (`0x614bbf`), through `entities::live_display`.
    pub(crate) scale: f32,
}

/// A unit's movement speeds (yd/s) from its `LIVING` movement block. The animation selector runs
/// above 2x `walk` (`0x5fd224`).
#[derive(Component, Clone, Copy)]
pub(crate) struct UnitSpeeds(pub(crate) MoveSpeeds);

/// A mover's speed from its move flags (`CMovement::GetCurrentSpeed 0x7c4c90`). The order is the
/// fact: no direction bit is 0 (`0x7c4c99`); swimming first (`0x7c4cd9`); then walk mode as
/// `min(walk, run)` even backward (`0x7c4d11`); then run, backward as `min(run_back, run)`
/// (`0x7c4d1d`). Each min is x87 `fcompp`, ties and NaN taking the forward field.
///
/// No zero-`walk` fallback: the ctor's zeroed speeds (`0x7c48e8`) are never observable, since
/// the post-create apply `0x5fad50` applies our own speed block as it does a remote one.
pub(crate) fn current_speed(s: &MoveSpeeds, flags: u32) -> f32 {
    use crate::creature_anim::move_flags as f;
    // A swimmer or flier on its ascend or descend key moves without a direction bit.
    let vertical = flags & (f::SWIMMING | f::FLYING) != 0
        && flags & (f::ASCENDING | f::DESCENDING) != 0;
    if flags & f::ANY_MOVE == 0 && !vertical {
        return 0.0;
    }
    if flags & f::SWIMMING != 0 {
        return if flags & f::BACKWARD != 0 {
            s.swim_back.min(s.swim)
        } else {
            s.swim
        };
    }
    // Build 12340's flight arm, shaped as the swim arm.
    if flags & f::FLYING != 0 {
        return if flags & f::BACKWARD != 0 {
            s.flight_back.min(s.flight)
        } else {
            s.flight
        };
    }
    if flags & f::WALK_MODE != 0 {
        return s.walk.min(s.run);
    }
    if flags & f::BACKWARD != 0 {
        return s.run_back.min(s.run);
    }
    s.run
}

/// A streamed object's descriptor fields, seeded by the create block and merged with each
/// `Values` delta. Speeds and pose are movement-block data, not here.
#[derive(Component, Clone, Default)]
pub(crate) struct ObjectStore(pub(crate) ObjectFields);

impl ObjectStore {
    /// The object manager's unit lookup (`0x468460`, `ecx = 8`): a unit or a player, which share
    /// the unit block; a game object, corpse or item is not one.
    pub(crate) fn is_unit(&self) -> bool {
        use benilla_protocol::messages::ObjectType;
        matches!(
            self.0.object_type(),
            Some(ObjectType::Unit | ObjectType::Player)
        )
    }
}

/// One descriptor dword moved on a streamed object, the reference's `CMirrorHandler` edge: the
/// values notifier (`0x465330`) diffs live against a shadow copy and passes the old value
/// (`0x465570`). A first create fires nothing; a re-create of a live guid fires like a delta.
///
/// `kind` is the store's class from its create block, since a raw index differs per class (`36`
/// is a unit's `BYTES_0` and a corpse's `DYNAMIC_FLAGS`).
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FieldChanged {
    pub entity: Entity,
    pub guid: u64,
    pub kind: benilla_protocol::messages::ObjectType,
    pub field: benilla_protocol::messages::Field,
    pub offset: u16,
    pub layout: benilla_protocol::messages::FieldLayout,
    pub old: u32,
    pub new: u32,
}

impl FieldChanged {
    /// A unit-block edge: units and players share the UNIT block.
    pub fn is_unit(&self) -> bool {
        use benilla_protocol::messages::ObjectType;
        matches!(self.kind, ObjectType::Unit | ObjectType::Player)
    }
    /// Match a semantic scalar (the first dword of a field).
    pub fn named_field(&self, field: benilla_protocol::messages::Field) -> bool {
        self.field == field && self.offset == 0
    }
    /// The dword offset in a semantic unit array, bounded by the caller's watch length.
    pub fn unit_slot(&self, field: benilla_protocol::messages::Field, len: u16) -> Option<u16> {
        (self.is_unit() && self.field == field && self.offset < len).then_some(self.offset)
    }
}

/// The field edges one feed saw this run, by guid and semantic array member: the reference fans
/// a moved field out to every unit token naming the unit (`0x515e50`).
#[derive(Default)]
pub(crate) struct FieldEdges(HashSet<(u64, benilla_protocol::messages::Field, u16)>);

impl FieldEdges {
    pub fn collect(reader: &mut MessageReader<FieldChanged>) -> Self {
        Self(reader.read().map(|e| (e.guid, e.field, e.offset)).collect())
    }
    pub fn moved_field(
        &self,
        guid: u64,
        field: benilla_protocol::messages::Field,
        offset: u16,
    ) -> bool {
        self.0.contains(&(guid, field, offset))
    }
    #[cfg(test)]
    pub fn semantic_of(edges: &[(u64, benilla_protocol::messages::Field, u16)]) -> Self {
        Self(edges.iter().copied().collect())
    }
}

/// Merge a descriptor update into a store and emit its [`FieldChanged`] edges; the drain and the
/// test harness share it.
pub(crate) fn merge_store_fields(
    store: &mut ObjectFields,
    delta: ObjectFields,
    entity: Entity,
    guid: u64,
    mut emit: impl FnMut(FieldChanged),
) {
    let Some(kind) = store.created_as() else {
        // A bare delta with no create: nothing for the reference to notify on; the fields land.
        store.merge(delta);
        return;
    };
    let layout = store.layout();
    store.merge_diff(delta, |index, old, new| {
        let Some((field, offset)) = layout.field_at(kind, index) else {
            return; // Unknown native dwords have no semantic watcher.
        };
        emit(FieldChanged {
            entity,
            guid,
            kind,
            field,
            offset,
            layout,
            old,
            new,
        });
    });
}

/// Apply a descriptor update the way the drain does. The first one must be
/// [`ObjectFields::into_created`], like a real create block.
#[cfg(test)]
pub(crate) fn apply_fields_for_test(world: &mut World, entity: Entity, delta: ObjectFields) {
    let guid = world.get::<Guid>(entity).map_or(0, |g| g.0);
    let mut edges = Vec::new();
    match world.get_mut::<ObjectStore>(entity) {
        Some(mut store) => merge_store_fields(&mut store.0, delta, entity, guid, |e| edges.push(e)),
        None => {
            assert!(
                delta.created_as().is_some(),
                "a fixture's first descriptor is a create block: chain `.into_created(..)`"
            );
            world.entity_mut(entity).insert(ObjectStore(delta));
        }
    }
    world
        .resource_mut::<Messages<FieldChanged>>()
        .write_batch(edges);
}

/// Our own character's entity (guid == [`SelfGuid`]): identity, not what we steer ([`Embodied`]).
#[derive(Component)]
pub(crate) struct SelfPlayer;

/// The body this client is attached to: the reference's camera anchor (`camera+0x88`), written
/// only by the constructor and `SetTarget 0x50d0f0`, never by a control update. Normally our own
/// body; under possession, the possessed unit.
///
/// At most one entity carries it, and a claimed mover not yet streamed is none: outbound
/// `MSG_MOVE_*` carry no guid, so a fallback to our body would write our pose onto the creature.
/// Whether it may move is [`ActiveMover`]. [`crate::player`] owns the placement.
#[derive(Component)]
pub(crate) struct Embodied;

/// The unit this client authors motion for: the reference's active-mover global
/// (`0xc4da98`/`0xc4da9c`, written only by `SetActiveMover 0x6006e0`). A control update that
/// forbids the unit zeroes it (`0x5fa600`), which stops input (`0x514640`) and every movement
/// report (`0x600860`).
///
/// The server-replay lanes filter `Without<ActiveMover>`, not `Without<Embodied>`, so a body we
/// are attached to but lost control of still follows the server.
#[derive(Component)]
pub(crate) struct ActiveMover;

// ── ECS state: resources ─────────────────────────────────────────────────────────────────────────

/// The inbound event channel, drained each frame by [`apply_net_updates`].
#[derive(Resource)]
pub(crate) struct NetEvents(Receiver<SessionEvent>);

/// Fresh producer endpoints published immediately before their ordered [`SessionEvent::Connected`]
/// edge. The connection handler installs one only when that edge reaches ECS.
#[derive(Resource)]
pub(crate) struct CommandEndpoints(Receiver<SessionCommandSender>);

/// One app command stamped with the connection epoch observed by its producer. Stamping happens
/// here, before enqueue, so an old producer backlog can never be relabelled for a new socket.
pub(crate) struct SessionCommand {
    pub(crate) epoch: u64,
    pub(crate) command: ClientCommand,
}

/// The outbound command channel. Its tuple constructor remains source-compatible with fixtures
/// that provide a raw channel; production replaces its immutable endpoint at each ECS connection
/// barrier.
#[derive(Resource, Clone)]
pub(crate) struct NetCommandsRaw(pub(crate) CommandSender);

pub(crate) type NetCommands = NetCommandsRaw;

#[allow(non_snake_case)]
pub(crate) fn NetCommands(sender: Sender<ClientCommand>) -> NetCommands {
    NetCommandsRaw(CommandSender::Raw(sender))
}

/// A source-compatible command endpoint: fixtures can still supply a raw sender, while the live
/// Wrath backend stamps the epoch in this `send` call made by the actual ECS producer.
#[derive(Clone)]
pub(crate) enum CommandSender {
    Raw(Sender<ClientCommand>),
    Session(SessionCommandSender),
}

impl CommandSender {
    pub(crate) fn send(
        &self,
        command: ClientCommand,
    ) -> Result<(), crossbeam_channel::SendError<ClientCommand>> {
        match self {
            Self::Raw(sender) => sender.send(command),
            Self::Session(sender) => sender.send(command),
        }
    }
}

/// One immutable production endpoint. Every clone retains this producer-observed epoch forever;
/// only the ECS `Connected` barrier replaces the endpoint held by [`NetCommands`].
#[derive(Clone)]
pub(crate) struct SessionCommandSender {
    sender: Sender<SessionCommand>,
    epoch: u64,
}

impl SessionCommandSender {
    pub(crate) fn new(sender: Sender<SessionCommand>, epoch: u64) -> Self {
        Self { sender, epoch }
    }

    pub(crate) fn send(
        &self,
        command: ClientCommand,
    ) -> Result<(), crossbeam_channel::SendError<ClientCommand>> {
        self.sender
            .send(SessionCommand {
                epoch: self.epoch,
                command,
            })
            .map_err(|error| crossbeam_channel::SendError(error.0.command))
    }
}

impl NetCommandsRaw {
    pub(crate) fn session_stamped(sender: SessionCommandSender) -> Self {
        Self(CommandSender::Session(sender))
    }

    /// Activates the endpoint paired with the `Connected` event currently crossing the ECS
    /// barrier. Raw fixture/Vanilla senders have no endpoint side channel.
    fn activate_connected(
        &mut self,
        endpoints: &CommandEndpoints,
        player: Option<&mut crate::player::Player>,
    ) {
        if matches!(self.0, CommandSender::Session(_)) {
            // Wrath reconnects in-process. Clear the old mover synchronously, before publishing
            // the new endpoint: the deferred session-end message reader runs later in Input and
            // must not get one frame in which it can stream the old pose to the new socket.
            if let Some(player) = player {
                *player = crate::player::Player::default();
            }
            self.0 = CommandSender::Session(
                endpoints
                    .0
                    .try_recv()
                    .expect("Wrath Connected event must carry a command endpoint"),
            );
        }
    }
}

fn activate_command_endpoint(
    In(event): In<SessionEvent>,
    endpoints: Res<CommandEndpoints>,
    mut commands: ResMut<NetCommands>,
    mut player: Option<ResMut<crate::player::Player>>,
) {
    if matches!(event, SessionEvent::Connected { .. }) {
        commands.activate_connected(&endpoints, player.as_deref_mut());
    }
}

impl NetCommands {
    /// Commands the selected dialect can currently encode without borrowing another build's wire.
    /// Wrath admits its implemented on-foot, lifecycle, item, loot, melee, query, action-bar and
    /// cast codecs.
    pub(crate) fn supports(&self, dialect: SessionDialect, command: &ClientCommand) -> bool {
        match dialect {
            SessionDialect::Vanilla5875 => true,
            SessionDialect::Wrath12340 => {
                if let ClientCommand::Move {
                    kind,
                    flags,
                    pos,
                    orientation,
                    pitch,
                    fall_time,
                    jump,
                    transport,
                } = command
                {
                    return *pitch == 0.0
                        && transport.is_none()
                        && benilla_protocol::wrath_ground::encode_on_foot(
                            u32::from(kind.opcode()),
                            1,
                            *flags,
                            0,
                            *pos,
                            *orientation,
                            *fall_time,
                            jump.map(|jump| benilla_protocol::wrath_ground::FallTail {
                                vertical_speed: jump.zspeed,
                                sin_angle: jump.sin_angle,
                                cos_angle: jump.cos_angle,
                                horizontal_speed: jump.xy_speed,
                            }),
                        )
                        .is_ok();
                }
                // Route ownership is the source of truth. Keep value-level validation separate
                // from admission so adding a native writer cannot silently strand its producer.
                match command {
                    ClientCommand::AutostoreLootItem { slot } if *slot > 17 => false,
                    ClientCommand::SetSheathed { state } if *state > 2 => false,
                    ClientCommand::StandStateChange { state }
                        if !matches!(state, 0 | 1 | 3 | 8) =>
                    {
                        false
                    }
                    _ => io::wrath::has_writer_route(command),
                }
            }
        }
    }
}

/// The character-select channel to the IO thread parked at select: create and delete are served
/// in place, re-emitting the roster, until an [`CharRequest::Enter`] moves into the world.
#[derive(Resource)]
pub(crate) struct CharPick(pub(crate) Sender<CharRequest>);

/// One request to the IO thread parked at character select.
pub(crate) enum CharRequest {
    /// Log in as this character (`CMSG_PLAYER_LOGIN`).
    Enter(u64),
    /// Create a character (`CMSG_CHAR_CREATE`), then stay parked at select with the fresh roster.
    Create(benilla_protocol::CharCreateReq),
    /// Delete a character by guid (`CMSG_CHAR_DELETE`), then stay parked at select.
    Delete(u64),
    /// A build-12340 character-screen service (rename, customize, faction/race change, declined
    /// names, realm split), then stay parked at select. 1.12 has none and ignores it.
    Service(benilla_protocol::wrath_char_services::Command),
    /// Select's Back: drop the session and return to the pre-logon park.
    Abandon,
}

/// The realm channel: the answer to each [`RealmListMessage`]. Two parks listen, the login-side
/// one and the character one, since the reference's realm list is a dialog over either screen
/// (`RealmList.xml` is `frameStrata="DIALOG"`).
#[derive(Resource)]
pub(crate) struct RealmChoice(pub(crate) Sender<RealmRequest>);

/// One request to the IO thread parked at the realm list.
#[derive(Debug)]
pub(crate) enum RealmRequest {
    /// Enter this realm, by name: the list refreshes while open, so an index could shift.
    Enter(String),
    /// Re-request the realm list (`RequestRealmList`, every 5 s from `RealmList_OnUpdate`).
    Refresh,
    /// Cancel (`RealmList_OnCancel` only hides the frame): the screen underneath stays, login or
    /// character select.
    Abandon,
}

/// The credentials channel to the IO thread's pre-logon park, sent by [`crate::login`].
#[derive(Resource)]
pub(crate) struct LoginSubmit(pub(crate) Sender<io::LoginRequest>);

/// The login abandon generation: Cancel bumps it, and the IO thread drops an
/// [`io::LoginRequest`] submitted under an older value.
#[derive(Resource)]
pub(crate) struct LoginAbandon(pub(crate) std::sync::Arc<std::sync::atomic::AtomicU64>);

/// Guid to entity for every kind, items included, like the reference's one `ClntObjMgr` index.
/// Written only by `net::objects`.
#[derive(Resource, Default)]
pub(crate) struct GuidIndex(pub(crate) HashMap<u64, Entity>);

/// The streamed objects, whatever the kind, with their descriptor stores, our own player, and an
/// item's countdown cells. Read-only. The by-guid lookups are `ClntObjMgrObjectPtr` (`0x468460`).
#[derive(SystemParam)]
pub struct Objects<'w, 's> {
    index: Res<'w, GuidIndex>,
    stores: Query<'w, 's, &'static ObjectStore>,
    countdowns: Query<'w, 's, &'static crate::items::Countdowns>,
    me: Query<'w, 's, Entity, With<SelfPlayer>>,
}

impl Objects<'_, '_> {
    /// The entity behind a guid, if streamed (`ClntObjMgrObjectPtr`, `0x468460`).
    pub fn entity(&self, guid: u64) -> Option<Entity> {
        self.index.0.get(&guid).copied()
    }

    /// A streamed object's merged descriptor fields, found as `0x468460` does, by guid.
    pub fn object(&self, guid: u64) -> Option<&ObjectFields> {
        self.index
            .0
            .get(&guid)
            .and_then(|&e| self.stores.get(e).ok())
            .map(|s| &s.0)
    }

    /// Every streamed object, items included.
    pub fn iter(&self) -> impl Iterator<Item = (u64, Entity, &ObjectFields)> + '_ {
        self.index
            .0
            .iter()
            .filter_map(|(&guid, &e)| Some((guid, e, &self.stores.get(e).ok()?.0)))
    }

    /// Our own player, once in the world.
    pub fn player(&self) -> Option<Entity> {
        self.me.single().ok()
    }

    /// An item object's countdown cells.
    pub(crate) fn countdowns(&self, guid: u64) -> Option<&crate::items::Countdowns> {
        self.index
            .0
            .get(&guid)
            .and_then(|&e| self.countdowns.get(e).ok())
    }
}

/// Our own player's guid, once in the world.
#[derive(Resource, Default)]
pub(crate) struct SelfGuid(pub(crate) Option<u64>);

/// Connection status; `last_reason` keeps the most recent failure.
#[derive(Resource, Default)]
pub(crate) struct NetStatus {
    pub(crate) connected: bool,
    pub(crate) last_reason: Option<String>,
}

/// The ping clock and RTT history, shared with both net threads: the write thread stamps each
/// `CMSG_PING`, the read thread times `SMSG_PONG` on arrival. The app only reads and clears it,
/// never times a pong from the drain.
#[derive(Resource)]
pub(crate) struct PingShared(pub(crate) std::sync::Arc<std::sync::Mutex<io::PingClock>>);

/// Server packets the codec dropped, by opcode: `unknown` has no parse arm, `unparseable` failed
/// its parser. Never cleared on reconnect.
#[derive(Resource, Default)]
pub(crate) struct DroppedOpcodes(pub(crate) HashMap<u16, DropTally>);

/// Per-opcode drop counts.
#[derive(Default, Clone, Copy)]
pub(crate) struct DropTally {
    pub(crate) unknown: u64,
    pub(crate) unparseable: u64,
}

/// The in-game clock (`SMSG_LOGIN_SETTIMESPEED`), advanced by its timescale.
#[derive(Resource, Default)]
pub(crate) struct ServerTime(pub(crate) Option<GameTime>);

/// Publish the session clock into [`benilla_world::lighting::WorldTime`] each frame; with no
/// server, its default (noon).
pub(crate) fn publish_world_time(
    server: Res<ServerTime>,
    mut world_time: ResMut<benilla_world::lighting::WorldTime>,
) {
    *world_time = match server.0 {
        Some(gt) => benilla_world::lighting::WorldTime {
            minute: gt.minute_of_day(),
            minute_f: gt.minute_of_day_f32(),
            day: gt.day_continuous(),
            live: true,
        },
        None => benilla_world::lighting::WorldTime::default(),
    };
}

/// The server's unix wall clock (`SMSG_QUERY_TIME_RESPONSE`), advanced monotonically. Descriptor
/// deadlines are absolute server stamps (a timed quest's is `time(nullptr) + limitTime`, vmangos
/// `Player::AddQuest`), so a countdown reads this, never the local clock.
#[derive(Resource, Default)]
pub(crate) struct ServerWallClock(pub(crate) Option<WallClockSample>);

/// One `SMSG_QUERY_TIME_RESPONSE` sample and the monotonic instant it arrived.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WallClockSample {
    /// The server's `time(nullptr)` at `received`.
    base_unix: u32,
    received: Instant,
}

impl ServerWallClock {
    /// Take a fresh sample; it reads about RTT/2 behind, below a countdown's one-second grain.
    pub(crate) fn sample(&mut self, unix_time: u32) {
        self.0 = Some(WallClockSample {
            base_unix: unix_time,
            received: Instant::now(),
        });
    }

    /// Server unix seconds now, fractional.
    pub(crate) fn now_unix(&self) -> Option<f64> {
        self.0
            .map(|s| f64::from(s.base_unix) + s.received.elapsed().as_secs_f64())
    }

    fn stale(&self) -> bool {
        self.0.is_none_or(|s| s.received.elapsed() >= RESYNC_AFTER)
    }
}

/// The wall-clock resync interval: the reference re-asks after an hour (`0x4de836` gated on
/// `[0xbb749c]`, armed `now + 0xe10` at `0x4def11`).
const RESYNC_AFTER: Duration = Duration::from_secs(3600);

/// Ask for the server's wall clock on entering the world (login, worldport, instance transfer)
/// and hourly after, which tracks a server re-clocked under us.
fn send_query_time(
    mut entered: MessageReader<EnteredWorldMessage>,
    commands: Res<NetCommands>,
    dialect: Res<SessionDialect>,
    clock: Res<ServerWallClock>,
    status: Res<NetStatus>,
    mut asked_at: Local<Option<Instant>>,
) {
    let entering = entered.read().next().is_some();
    // Only while connected; the world-enter send covers a reconnect.
    let due =
        status.connected && clock.stale() && asked_at.is_none_or(|t| t.elapsed() >= RESYNC_AFTER);
    if (entering || due) && commands.supports(*dialect, &ClientCommand::QueryTime) {
        *asked_at = Some(Instant::now());
        let _ = commands.0.send(ClientCommand::QueryTime);
    }
}

/// Our `(flags, standing)` per slot, indexed by `Faction.dbc`'s `reputationIndex`. The standing
/// excludes the DBC race/class base, which consumers add before ranking. Filled by
/// `SMSG_INITIALIZE_FACTIONS`, kept current by `SMSG_SET_FACTION_STANDING` (which also reveals)
/// and `SMSG_SET_FACTION_VISIBLE`. Flag bit `0x08` marks a pane header.
#[derive(Resource, Default)]
pub(crate) struct Reputations(pub(crate) Vec<(u8, i32)>);

/// The hearthstone bind point (`SMSG_BINDPOINTUPDATE`): the AreaTable id the tooltip's `$z`
/// token names.
#[derive(Resource, Default)]
pub(crate) struct HomeBind(pub(crate) Option<u32>);

/// The undelivered `/played` answer (`SMSG_PLAYED_TIME`): total seconds and seconds this level.
/// One slot: the server sends one reply per request.
#[derive(Resource, Default)]
pub(crate) struct PlayedTimeAnswer(pub(crate) Option<(u32, u32)>);

/// Equip proficiencies (`SMSG_SET_PROFICIENCY`): item class (2 weapon, 4 armor) to a subclass
/// bitmask, the client's `0xc4d4a0[class]` store.
#[derive(Resource, Default)]
pub(crate) struct Proficiencies(pub(crate) std::collections::HashMap<u32, u32>);

/// The in-game clock from `SMSG_LOGIN_SETTIMESPEED`: the server's `localtime()`, advancing at
/// `timescale` game-minutes per real second (vmangos about `0.01667`, real time).
#[derive(Debug, Clone, Copy)]
pub(crate) struct GameTime {
    /// Minute of the game day (`0..1440`) at `received`.
    base_minute: u32,
    /// Day serial of the packed date, `year*372 + month*31 + day`; only differences matter.
    base_day: u32,
    timescale: f32,
    received: Instant,
}

impl GameTime {
    pub(crate) fn new(hours: u8, minutes: u8, day_serial: u32, timescale: f32) -> Self {
        Self {
            base_minute: hours as u32 * 60 + minutes as u32,
            base_day: day_serial,
            timescale,
            received: Instant::now(),
        }
    }

    /// The packed login date as `(years since 2000, month 1..=12, day 1..=31)`, decoded from
    /// [`Self::base_day`]'s `year·372 + month·31 + day`. No day rollover is applied.
    pub(crate) fn date(&self) -> (u32, u32, u32) {
        let rem = self.base_day % 372;
        (self.base_day / 372, (rem.max(1) - 1) / 31, (rem.max(1) - 1) % 31 + 1)
    }

    /// Current minute of the game day (`0..1440`).
    pub(crate) fn minute_of_day(&self) -> u32 {
        self.minute_of_day_f32() as u32
    }

    /// Current fractional minute of the game day, so the sky bodies move without steps.
    pub(crate) fn minute_of_day_f32(&self) -> f32 {
        let elapsed = self.received.elapsed().as_secs_f32() * self.timescale;
        (self.base_minute as f32 + elapsed).rem_euclid(1440.0)
    }

    /// Days and day fraction since the serial epoch, unwrapped, for the moon phase
    /// `(dayCounter + todPhase) mod 1.7` (`0x6d41b9`); `f64` for sub-minute grain at ~12k days.
    pub(crate) fn day_continuous(&self) -> f64 {
        let elapsed = self.received.elapsed().as_secs_f64() * self.timescale as f64;
        self.base_day as f64 + (self.base_minute as f64 + elapsed) / 1440.0
    }
}

// ── Outbound commands + one-shot inbound events ──────────────────────────────────────────────────

/// One self-movement event, each a `MSG_MOVE_*` opcode: the reference sends each axis transition,
/// a jump, a standing facing change and a heartbeat while moving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MoveKind {
    StartForward,
    StartBackward,
    Stop,
    StartStrafeLeft,
    StartStrafeRight,
    StopStrafe,
    StartTurnLeft,
    StartTurnRight,
    StopTurn,
    Jump,
    FallLand,
    StartSwim,
    StopSwim,
    /// Build 12340's flight edges (`Wow.exe` `0x6ef860`): the ascend and descend keys while flying
    /// (`STOP_ASCEND` ends either) and taking off or landing (`CMSG_MOVE_SET_FLY`).
    StartAscend,
    StopAscend,
    StartDescend,
    SetFly,
    /// The pitch keys while swimming or flying (`0x6ef860` events 0xe/0xf/0x10).
    StartPitchUp,
    StartPitchDown,
    StopPitch,
    /// The walk/run toggle: sent from the toggle handler (`ToggleRun 0x513d50` to `0x617de0`),
    /// not the move broadcaster, so it goes out standing still (`0x61a99d` gates the latter).
    SetWalkMode,
    SetRunMode,
    SetFacing,
    Heartbeat,
}

impl MoveKind {
    pub(crate) fn opcode(self) -> u16 {
        use benilla_protocol::messages::opcode as op;
        use benilla_protocol::wrath_ground;
        match self {
            MoveKind::StartForward => op::MSG_MOVE_START_FORWARD,
            MoveKind::StartBackward => op::MSG_MOVE_START_BACKWARD,
            MoveKind::Stop => op::MSG_MOVE_STOP,
            MoveKind::StartStrafeLeft => op::MSG_MOVE_START_STRAFE_LEFT,
            MoveKind::StartStrafeRight => op::MSG_MOVE_START_STRAFE_RIGHT,
            MoveKind::StopStrafe => op::MSG_MOVE_STOP_STRAFE,
            MoveKind::StartTurnLeft => op::MSG_MOVE_START_TURN_LEFT,
            MoveKind::StartTurnRight => op::MSG_MOVE_START_TURN_RIGHT,
            MoveKind::StopTurn => op::MSG_MOVE_STOP_TURN,
            MoveKind::Jump => op::MSG_MOVE_JUMP,
            MoveKind::FallLand => op::MSG_MOVE_FALL_LAND,
            MoveKind::StartSwim => op::MSG_MOVE_START_SWIM,
            MoveKind::StopSwim => op::MSG_MOVE_STOP_SWIM,
            MoveKind::StartAscend => wrath_ground::MSG_MOVE_START_ASCEND as u16,
            MoveKind::StopAscend => wrath_ground::MSG_MOVE_STOP_ASCEND as u16,
            MoveKind::StartDescend => wrath_ground::MSG_MOVE_START_DESCEND as u16,
            MoveKind::SetFly => wrath_ground::CMSG_MOVE_SET_FLY as u16,
            MoveKind::StartPitchUp => wrath_ground::MSG_MOVE_START_PITCH_UP as u16,
            MoveKind::StartPitchDown => wrath_ground::MSG_MOVE_START_PITCH_DOWN as u16,
            MoveKind::StopPitch => wrath_ground::MSG_MOVE_STOP_PITCH as u16,
            MoveKind::SetWalkMode => op::MSG_MOVE_SET_WALK_MODE,
            MoveKind::SetRunMode => op::MSG_MOVE_SET_RUN_MODE,
            MoveKind::SetFacing => op::MSG_MOVE_SET_FACING,
            MoveKind::Heartbeat => op::MSG_MOVE_HEARTBEAT,
        }
    }
}

/// The `ChatMsg` type an addon broadcast rides: the client's four-lane whitelist
/// (`0x49fa3f`-`0x49fa4e`).
/// [`addon_wire_chat_type`] in build 12340's `ChatMsg` numbering.
pub(crate) fn wrath_addon_chat_type(distribution: benilla_ui::script::AddonDistribution) -> u32 {
    use benilla_ui::script::AddonDistribution as D;
    match distribution {
        D::Party => 0x02,
        D::Raid => 0x03,
        D::Guild => 0x04,
        D::Battleground => 0x2c,
        D::Whisper => 0x07,
    }
}

pub(crate) fn addon_wire_chat_type(distribution: benilla_ui::script::AddonDistribution) -> u32 {
    use benilla_protocol::messages as m;
    use benilla_ui::script::AddonDistribution as D;
    match distribution {
        D::Party => m::CHAT_TYPE_PARTY,
        D::Raid => m::CHAT_TYPE_RAID,
        D::Guild => m::CHAT_TYPE_GUILD,
        D::Battleground => m::CHAT_TYPE_BATTLEGROUND,
        // Only the 12340 binding produces it; 1.12 has no whispered addon message.
        D::Whisper => m::CHAT_TYPE_WHISPER,
    }
}

/// The `ChatMsg` type of a chat-bar line: the full sendable set (vmangos
/// `HandleChatMessageOpcode`). `Whisper` and `Channel` name their target in `target`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ChatKind {
    /// `/say`; GM dot-commands go out this way, parsed after the language gate.
    Say,
    /// `/yell` (`CHAT_MSG_YELL`, `SharedDefines.h:1199`).
    Yell,
    /// A custom `/emote`, shown verbatim as `"PlayerName <text>"`.
    Emote,
    /// `/whisper`: the target's name precedes the text (`Chat.cpp:3-12`).
    Whisper,
    /// `/p`, dropped silently when ungrouped (`ChatHandler.cpp:472-493`).
    Party,
    /// `/ra`; needs a raid (`ChatHandler.cpp:514-536`).
    Raid,
    /// `/rl`, leader only (`ChatHandler.cpp:538-559`).
    RaidLeader,
    /// `/rw`, leader or assistant only (`ChatHandler.cpp:561-576`).
    RaidWarning,
    /// `/g`; needs a guild (`ChatHandler.cpp:494-503`).
    Guild,
    /// `/o`; needs a guild (`ChatHandler.cpp:504-513`).
    Officer,
    /// Needs a battleground group (`ChatHandler.cpp:579-593`).
    Battleground,
    /// Battleground leader only (`ChatHandler.cpp:595-609`).
    BattlegroundLeader,
    /// Toggles AFK, the text the auto-reply if any; it clears DND (`ChatHandler.cpp:611-630`).
    Afk,
    /// Toggles DND, which clears AFK (`ChatHandler.cpp:632-648`).
    Dnd,
    /// A channel line by name, dropped unless we are on it (`ChatHandler.cpp:255-327`).
    Channel,
}

impl ChatKind {
    /// Build 12340's `CMSG_MESSAGECHAT` `type` (AzerothCore `SharedDefines.h` `ChatMsg`).
    pub(crate) fn wrath_chat_type(self) -> u32 {
        match self {
            Self::Say => 0x01,
            Self::Party => 0x02,
            Self::Raid => 0x03,
            Self::Guild => 0x04,
            Self::Officer => 0x05,
            Self::Yell => 0x06,
            Self::Whisper => 0x07,
            Self::Emote => 0x0a,
            Self::Channel => 0x11,
            Self::Afk => 0x17,
            Self::Dnd => 0x18,
            Self::RaidLeader => 0x27,
            Self::RaidWarning => 0x28,
            Self::Battleground => 0x2c,
            Self::BattlegroundLeader => 0x2d,
        }
    }

    /// The `CMSG_MESSAGECHAT` `type` field.
    pub(crate) fn chat_type(self) -> u32 {
        use benilla_protocol::messages as m;
        match self {
            Self::Say => m::CHAT_TYPE_SAY,
            Self::Yell => m::CHAT_TYPE_YELL,
            Self::Emote => m::CHAT_TYPE_EMOTE,
            Self::Whisper => m::CHAT_TYPE_WHISPER,
            Self::Party => m::CHAT_TYPE_PARTY,
            Self::Raid => m::CHAT_TYPE_RAID,
            Self::RaidLeader => m::CHAT_TYPE_RAID_LEADER,
            Self::RaidWarning => m::CHAT_TYPE_RAID_WARNING,
            Self::Guild => m::CHAT_TYPE_GUILD,
            Self::Officer => m::CHAT_TYPE_OFFICER,
            Self::Battleground => m::CHAT_TYPE_BATTLEGROUND,
            Self::BattlegroundLeader => m::CHAT_TYPE_BATTLEGROUND_LEADER,
            Self::Afk => m::CHAT_TYPE_AFK,
            Self::Dnd => m::CHAT_TYPE_DND,
            Self::Channel => m::CHAT_TYPE_CHANNEL,
        }
    }
}

/// `WOW_CAST_TRACE=1`: log our own cast packets and every outbound movement packet. vmangos
/// interrupts a cast on a movement report whose position differs at all (`Player::SetPosition`).
pub(crate) static CAST_TRACE: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::env::var("WOW_CAST_TRACE").is_ok());

/// A message to the server through the write thread, carrying any pose it needs.
#[derive(Debug)]
pub(crate) enum ClientCommand {
    /// A self-movement packet. Each tail is written iff its flag is set (`JUMPING`, `SWIMMING`,
    /// `ON_TRANSPORT`): a flag without its tail desyncs the server's parse.
    Move {
        kind: MoveKind,
        flags: u32,
        pos: [f32; 3],
        orientation: f32,
        /// Swim pitch (radians, up positive).
        pitch: f32,
        fall_time: u32,
        jump: Option<JumpInfo>,
        /// The rider's pose in the transport's local frame.
        transport: Option<TransportPose>,
    },
    /// `CMSG_MOVE_SPLINE_DONE`: a server spline that drove our player ended. The server holds the
    /// mover spline-pending until it arrives.
    MoveSplineDone {
        flags: u32,
        pos: [f32; 3],
        orientation: f32,
        spline_id: u32,
    },
    /// `CMSG_FORCE_*_SPEED_CHANGE_ACK`: echoes guid, counter and exact speed with a live movement
    /// payload. Unacked, the server force-resolves after ~4 s and flags the session.
    ForceSpeedAck {
        kind: SpeedKind,
        guid: u64,
        counter: u32,
        speed: f32,
        flags: u32,
        pos: [f32; 3],
        orientation: f32,
        pitch: f32,
        fall_time: u32,
        jump: Option<JumpInfo>,
        transport: Option<TransportPose>,
    },
    /// `CMSG_MOVE_TIME_SKIPPED`: our mover skipped `lag_ms` without integrating; `guid` must be the
    /// active mover (`0x600be0`). Just after boarding, the server answers with the transport's
    /// create update.
    MoveTimeSkipped {
        guid: u64,
        lag_ms: u32,
    },
    /// `CMSG_SET_ACTIVE_MOVER`: at login and on possession; the server drops `MSG_MOVE_*` for an
    /// unconfirmed mover.
    SetActiveMover {
        guid: u64,
    },
    /// `CMSG_MOVE_NOT_ACTIVE_MOVER`: the server re-broadcasts a stop from this pose.
    NotActiveMover {
        guid: u64,
        flags: u32,
        pos: [f32; 3],
        orientation: f32,
        fall_time: u32,
    },
    /// Build 12340: dismiss the vehicle we drive, or change seats on it. The writer adds the
    /// vehicle's guid and halted pose.
    NativeVehicleControl(benilla_protocol::wrath_authority::VehicleControl),
    /// `CMSG_FAR_SIGHT`: `true` on attach, `false` on release; neither names an object.
    FarSight {
        engage: bool,
    },
    /// `MSG_MOVE_TELEPORT_ACK`: unacked, the server freezes our movement.
    TeleportAck {
        guid: u64,
        counter: u32,
    },
    /// `MSG_MOVE_WORLDPORT_ACK`: unblocks the new map's stream.
    WorldportAck,
    NativeWorldportAck(benilla_protocol::wrath_transfer::Token),
    /// `CMSG_SET_SELECTION`; `guid == 0` clears.
    SetSelection {
        guid: u64,
    },
    /// `CMSG_MESSAGECHAT`. A plain line, `.` GM commands included, goes as `Say`.
    Chat {
        kind: ChatKind,
        target: Option<String>,
        text: String,
        /// The `Languages.dbc` id `SendChatMessage` named; `None` speaks the character's own.
        language: Option<u32>,
    },
    /// `SendAddonMessage`: a `CMSG_MESSAGECHAT` in `LANG_ADDON` (1.12 has no addon opcode and no
    /// whispered addon message). `text` is `prefix` TAB `message`; `target` is the 12340 WHISPER
    /// lane's recipient, `None` on every other lane.
    AddonMessage {
        distribution: benilla_ui::script::AddonDistribution,
        text: String,
        target: Option<String>,
    },
    /// `CMSG_NAME_QUERY`, sent only by [`crate::names::NameCache`]'s ask-once resolve.
    NameQuery {
        guid: u64,
    },
    /// `CMSG_CREATURE_QUERY`, the entry from the guid's bits 24-47.
    CreatureQuery {
        entry: u32,
        guid: u64,
    },
    /// `CMSG_PET_NAME_QUERY`: a pet's bits 24-47 are its pet number, not a creature entry.
    PetNameQuery {
        pet_number: u32,
        guid: u64,
    },
    /// `CMSG_ITEM_QUERY_SINGLE` (`guid` 0 for template-only), sent only by
    /// [`crate::items::Items`]' ask-once resolve.
    ItemQuery {
        entry: u32,
        guid: u64,
    },
    /// `CMSG_USE_ITEM` by wire position: bag 255 and an absolute slot for the player's own array
    /// (equipment, backpack, keyring), else the bag's slot 19-22 and the inner slot.
    /// `spell_index` is the template's spell block ordinal. `target` is the same cast-targets
    /// block as a cast's (`SendCast 0x6e54f0` writes both).
    UseItem {
        bag_index: u8,
        slot: u8,
        spell_index: u8,
        /// Build 12340 names the cast spell and the item GUID; AC checks both.
        spell_id: u32,
        item_guid: u64,
        target: benilla_protocol::messages::UseItemTarget,
    },
    /// `CMSG_OPEN_ITEM`: the server answers `SMSG_LOOT_RESPONSE` on the item's own guid; a wrapped
    /// gift instead changes entry in place with no window.
    OpenItem {
        bag_index: u8,
        slot: u8,
    },
    /// `CMSG_WRAP_ITEM`, the paper first as on the wire. The client filters nothing; the server
    /// refuses with an `ERR_CANT_WRAP_*`, and success is silent.
    WrapItem {
        gift_bag: u8,
        gift_slot: u8,
        item_bag: u8,
        item_slot: u8,
    },
    /// `CMSG_AUTOEQUIP_ITEM`.
    AutoEquipItem {
        bag_index: u8,
        slot: u8,
    },
    /// `CMSG_SET_AMMO`, the auto-equip fork for ammo (`0x5e1480`): by entry, the stack stays in
    /// the bag and `PLAYER_AMMO_ID` names it.
    SetAmmo {
        entry: u32,
    },
    /// `CMSG_SWAP_INV_ITEM`: both slots on the player array; an empty destination is a move.
    SwapInvItem {
        src_slot: u8,
        dst_slot: u8,
    },
    /// `CMSG_SWAP_ITEM`, when either end is in an equipped bag; the body is destination first
    /// (vmangos `Packets/Item.cpp:30-36`).
    SwapItem {
        dst_bag: u8,
        dst_slot: u8,
        src_bag: u8,
        src_slot: u8,
    },
    /// `CMSG_AUTOSTORE_BAG_ITEM`: into `dst_bag` with no destination slot (`0x5e12e0`).
    AutoStoreBagItem {
        src_bag: u8,
        src_slot: u8,
        dst_bag: u8,
    },
    /// `CMSG_SPLIT_ITEM`: part of a stack, either end in any bag.
    SplitItem {
        src_bag: u8,
        src_slot: u8,
        dst_bag: u8,
        dst_slot: u8,
        count: u8,
    },
    /// `CMSG_DESTROYITEM`; `count` 0 is the whole stack.
    DestroyItem {
        bag_index: u8,
        slot: u8,
        count: u8,
    },
    /// `CMSG_TEXT_EMOTE`; our own sound and animation ride the `SMSG_TEXT_EMOTE` echo.
    TextEmote {
        text_id: u32,
        target: u64,
    },
    /// `CMSG_CAST_SPELL`; `None` is a self or implicit target.
    CastSpell {
        spell_id: u32,
        target: Option<u64>,
    },
    /// `CMSG_CAST_SPELL` at a corpse: its bit and packed guid, a resurrection on a released player.
    CastSpellCorpse {
        spell_id: u32,
        target: benilla_protocol::messages::CorpseTarget,
        corpse_guid: u64,
    },
    /// Build 12340's `CMSG_PET_CAST_SPELL`: a cast whose caster is our pet, charm or possessed
    /// unit, with the same targets block (the targeting cursor's pet commit).
    NativePetCastSpell {
        caster: u64,
        spell_id: u32,
        target: benilla_protocol::wrath_session::CastTarget,
    },
    /// `CMSG_CAST_SPELL` with `TARGET_FLAG_DEST_LOCATION`; `dest` in WoW coordinates.
    CastSpellAtDest {
        spell_id: u32,
        dest: [f32; 3],
    },
    /// `CMSG_CAST_SPELL` with `TARGET_FLAG_SOURCE_LOCATION` (`BindLocation 0x6e60f0`, bit 5);
    /// `src` in WoW coordinates.
    CastSpellAtSource {
        spell_id: u32,
        src: [f32; 3],
    },
    /// `CMSG_CANCEL_AURA`, by spell id, not slot; the removal returns as a `UNIT_FIELD_AURA` delta.
    CancelAura {
        spell_id: u32,
    },
    /// `CMSG_SET_ACTION_BUTTON`: `button` is the Lua action id minus 1, `packed` is
    /// `kind<<24 | action` (0 clears). Client-authoritative, no answer.
    SetActionButton {
        button: u8,
        packed: u32,
    },
    /// `CMSG_SET_ACTIONBAR_TOGGLES` (sent by `0x4e76e0`), one per call. The byte is server-owned
    /// (`PLAYER_FIELD_BYTES` byte 2, read at `0x4e768c`; nothing in the client writes it), so it
    /// only changes by the descriptor echo. Disconnected, a silent no-op (`0x5ab637`, `0x5379ab`,
    /// `0x5379b6`).
    SetActionBarToggles {
        toggles: u8,
    },
    /// `CMSG_PET_ACTION`: `packed` is the slot's word as the server sent it, dispatched on its type
    /// byte; `target_guid` is our selection (`0x4bd212`), or the player under the pet book's
    /// `onSelf` (`0x4b4345`). The server does not reply, so the caller applies the change locally
    /// first.
    PetAction {
        pet_guid: u64,
        packed: u32,
        target_guid: u64,
    },
    /// `CMSG_PET_SET_ACTION`: one or two `(position, word)` pairs. The bar's autocast toggle is
    /// one entry with bit 30 flipped.
    PetSetAction {
        pet_guid: u64,
        entries: Vec<(u32, u32)>,
    },
    /// `CMSG_PET_STOP_ATTACK`: the Attack button's second press.
    PetStopAttack {
        pet_guid: u64,
    },
    /// `CMSG_PET_CANCEL_AURA`; the removal returns as a `UNIT_FIELD_AURA` delta on the pet.
    PetCancelAura {
        pet_guid: u64,
        spell_id: u32,
    },
    /// `CMSG_PET_SPELL_AUTOCAST`: the pet spellbook's `ToggleSpellAutocast`, by spell id
    /// (`0x4b4240` to `0x4bccb0`); the bar's toggle is [`Self::PetSetAction`]. No reply.
    PetSpellAutocast {
        pet_guid: u64,
        spell_id: u32,
        enabled: bool,
    },
    /// `CMSG_PET_ABANDON`: the menu's Abandon only. Dismiss (`PetDismiss 0x4be4d0`) is a
    /// [`Self::PetAction`] with word `0x07000003`. Answered by a zero-guid `SMSG_PET_SPELLS`.
    PetAbandon {
        pet_guid: u64,
    },
    /// `CMSG_PET_RENAME`: success arrives as a bumped `UNIT_FIELD_PET_NAME_TIMESTAMP`.
    PetRename {
        pet_guid: u64,
        name: String,
    },
    /// `CMSG_ATTACKSWING`; echoed as `SMSG_ATTACKSTART`.
    AttackSwing {
        guid: u64,
    },
    /// `CMSG_ATTACKSTOP`, sent when the target is lost; the weapons stay drawn.
    AttackStop,
    /// `CMSG_CANCEL_AUTO_REPEAT_SPELL`, sent by every local cancel (`0x6ea0c6` in `0x6ea080`);
    /// harmless when the server cancelled first.
    CancelAutoRepeat,
    /// `CMSG_CANCEL_CAST`: from the wand auto-repeat handoff (`0x6095b8`) and a local cancel
    /// (`AbortCast 0x6e4940`).
    CancelCast {
        spell_id: u32,
    },
    /// `CMSG_CANCEL_CHANNELLING`; the server ignores the spell id the client writes.
    CancelChannelling {
        spell_id: u32,
    },
    /// `CMSG_SETSHEATHED` (0 stowed, 1 melee, 2 ranged); the client sends 1 on starting melee
    /// stowed (`0x5ecb70` to `0x611cf0`). Our own pose follows the `UNIT_FIELD_BYTES_2` echo.
    SetSheathed {
        state: u32,
    },
    /// `CMSG_STANDSTATECHANGE` (0 stand, 1 sit, 3 sleep, 8 kneel); our own pose follows the
    /// `UNIT_FIELD_BYTES_1` echo.
    StandStateChange {
        state: u32,
    },
    /// `CMSG_MOUNTSPECIAL_ANIM`: we play MountSpecial (94) locally and ignore our own echo.
    MountSpecial,
    /// `CMSG_GOSSIP_HELLO`: works on any interactable creature (the server's interact check is
    /// passed `UNIT_NPC_FLAG_NONE`, `NPCHandler.cpp:347`).
    GossipHello {
        guid: u64,
    },
    /// `CMSG_GOSSIP_SELECT_OPTION` with the line's `index`. No code is ever sent: coded options
    /// are not selectable.
    GossipSelectOption {
        guid: u64,
        option: u32,
    },
    /// `CMSG_NPC_TEXT_QUERY`, asked once per `text_id` on menu receipt.
    NpcTextQuery {
        text_id: u32,
        guid: u64,
    },
    /// `CMSG_LIST_INVENTORY`: a vendor-only NPC's direct opener.
    ListInventory {
        guid: u64,
    },
    /// `CMSG_BUY_ITEM` by template entry, not row; `count` stacks go to the first free slot.
    BuyItem {
        vendor: u64,
        entry: u32,
        count: u8,
    },
    /// `CMSG_BUY_ITEM_IN_SLOT`: a vendor row dropped on a slot. `bag_guid` is the container's, or
    /// the player's own for the backpack and equipment.
    BuyItemInSlot {
        vendor: u64,
        entry: u32,
        bag_guid: u64,
        bag_slot: u8,
        count: u8,
    },
    /// `CMSG_SELL_ITEM`; `count` 0 is the whole stack. Success is silent; a refusal is
    /// `SMSG_SELL_ITEM`'s error shape.
    SellItem {
        vendor: u64,
        item_guid: u64,
        count: u8,
    },
    /// `CMSG_BUYBACK_ITEM`: `slot` is the absolute player-array buyback slot 69-80.
    BuybackItem {
        vendor: u64,
        slot: u32,
    },
    /// `CMSG_REPAIR_ITEM`; `item_guid` 0 repairs everything. No answer packet.
    RepairItem {
        vendor: u64,
        item_guid: u64,
    },
    /// `CMSG_GMTICKET_CREATE`: `category` is a `GMTicketCategory.dbc` id (1..10), `map`/`pos`
    /// where we stand. vmangos refuses several cases silently (queue off, under
    /// `GMTickets.MinLevel`, category >= 11).
    GmTicketCreate {
        category: u8,
        map: u32,
        pos: [f32; 3],
        text: String,
    },
    /// `CMSG_GMTICKET_UPDATETEXT`, with the category byte vmangos discards. vmangos kicks past 2
    /// per world tick, so one send per click.
    GmTicketUpdate {
        category: u8,
        text: String,
    },
    /// `CMSG_GMTICKET_GETTICKET`: `GetGMTicket()`.
    GmTicketGet,
    /// `CMSG_GMTICKET_DELETETICKET`.
    GmTicketDelete,
    /// `CMSG_GMTICKET_SYSTEMSTATUS`: `GetGMStatus()`.
    GmTicketSystemStatus,
    /// `CMSG_BINDER_ACTIVATE`, the `CONFIRM_BINDER` Accept: the only packet in the flow that
    /// binds. Declining sends nothing.
    BinderActivate {
        binder: u64,
    },
    /// `CMSG_SUMMON_RESPONSE`, the `CONFIRM_SUMMON` Accept; there is no decline opcode.
    SummonResponse {
        summoner: u64,
    },
    /// `MSG_TALENT_WIPE_CONFIRM`, the `CONFIRM_TALENT_WIPE` Accept: the only packet in the flow
    /// that resets. Declining sends nothing.
    TalentWipeConfirm {
        trainer: u64,
    },
    // ── The dialog engine's verbs ──
    /// `ForceLogout()`: `CMSG_PLAYER_LOGOUT` (`0x4A`), only with a live world session.
    ForceLogout,
    /// `AcceptAreaSpiritHeal()`: `CMSG_AREA_SPIRIT_HEALER_QUEUE` with the cached healer.
    AreaSpiritHealerQueue {
        healer: u64,
    },
    /// `CMSG_AREA_SPIRIT_HEALER_QUERY`, asking a newly adopted healer's wave clock. No Lua verb
    /// sends it: only the set-current-healer routine `0x4921c0`.
    AreaSpiritHealerQuery {
        healer: u64,
    },
    /// `AcceptBattlefieldPort(index, accept)`: `CMSG_BATTLEFIELD_PORT`.
    BattlefieldPort {
        map_id: u32,
        accept: bool,
    },
    /// `RequestBattlefieldScoreData()`: `MSG_PVP_LOG_DATA`.
    RequestBattlefieldScoreData,
    /// `LeaveBattlefield()`: `CMSG_LEAVE_BATTLEFIELD`.
    LeaveBattlefield {
        map_id: u32,
    },
    /// A meeting stone right-click past the client's four refusals: `CMSG 0x292`.
    MeetingStoneJoin {
        go_guid: u64,
    },
    /// `CancelMeetingStoneRequest()`: `CMSG 0x293`.
    MeetingStoneLeave,
    /// The enter-world meeting-stone status query: `CMSG 0x296`.
    MeetingStoneStatusQuery,
    // ── The tutorial system ──
    /// `FlagTutorial` or an auto-acknowledge: `CMSG_TUTORIAL_FLAG`, the 0-based id.
    TutorialFlag {
        id: u32,
    },
    /// `ClearTutorials()`: `CMSG_TUTORIAL_CLEAR`.
    TutorialClear,
    /// `ResetTutorials()`: `CMSG_TUTORIAL_RESET`.
    TutorialReset,
    // ── The battleground list window ──
    /// `ShowBattlefieldList(index)`: `CMSG_BATTLEFIELD_LIST`.
    BattlefieldList {
        map_id: u32,
    },
    /// `JoinBattlefield` when the list came from a battlemaster: `CMSG_BATTLEMASTER_JOIN`.
    BattlemasterJoin {
        battlemaster: u64,
        map_id: u32,
        instance_id: u32,
        as_group: bool,
    },
    /// `RequestBattlefieldPositions()`: `MSG_BATTLEGROUND_PLAYER_POSITIONS`, throttled to the
    /// reference's 5000 ms.
    RequestBattlefieldPositions,
    // ── The tabard designer ──
    /// The NPC-click ladder's TABARDDESIGNER arm: `MSG_TABARDVENDOR_ACTIVATE`.
    TabardVendorActivate {
        npc: u64,
    },
    /// `TabardModel:Save()` past its checks: `MSG_SAVE_GUILD_EMBLEM`.
    SaveGuildEmblem {
        vendor: u64,
        design: [u32; 5],
    },
    /// The ladder's BATTLEMASTER arm: `CMSG_BATTLEMASTER_HELLO`.
    BattlemasterHello {
        npc: u64,
    },
    /// `JoinBattlefield` when the list came without one: `CMSG_BATTLEFIELD_JOIN`.
    BattlefieldJoin {
        map_id: u32,
        instance_id: u32,
        as_group: bool,
    },
    /// The world-enter status request: `CMSG_BATTLEFIELD_STATUS`.
    BattlefieldStatusRequest,
    /// `ConfirmPetUnlearn()`: `CMSG_PET_UNLEARN` with the latched trainer.
    PetUnlearn {
        trainer: u64,
    },
    /// `CMSG_BANKER_ACTIVATE`: a pure banker's direct opener (bit 8 the lowest service bit); a
    /// gossip banker goes through the menu. Answered by `SMSG_SHOW_BANK`.
    BankerActivate {
        guid: u64,
    },
    /// `CMSG_BUY_BANK_SLOT`: success is only the `PLAYER_BYTES_2` count rising; a refusal is
    /// `SMSG_BUY_BANK_SLOT_RESULT`.
    BuyBankSlot {
        guid: u64,
    },
    /// `CMSG_AUTOBANK_ITEM`: deposit into the bank.
    AutoBankItem {
        bag: u8,
        slot: u8,
    },
    /// `CMSG_AUTOSTORE_BANK_ITEM`: withdraw into the bags.
    AutoStoreBankItem {
        bag: u8,
        slot: u8,
    },
    /// `CMSG_TRAINER_LIST`: the refresh after a purchase, which the server does not resend.
    TrainerList {
        trainer: u64,
    },
    /// `CMSG_TRAINER_BUY_SPELL`: success is `SMSG_TRAINER_BUY_SUCCEEDED` and
    /// `SMSG_LEARNED_SPELL`, refusal `SMSG_TRAINER_BUY_FAILED`.
    TrainerBuySpell {
        trainer: u64,
        spell_id: u32,
    },
    /// `MSG_LIST_STABLED_PETS`: the refresh after each change, none of which returns a list.
    ListStabledPets {
        npc: u64,
    },
    /// `CMSG_STABLE_PET`: the server picks the first free slot.
    StablePet {
        npc: u64,
    },
    /// `CMSG_UNSTABLE_PET`, only with no pet out; otherwise [`Self::StableSwapPet`].
    UnstablePet {
        npc: u64,
        pet_number: u32,
    },
    /// `CMSG_STABLE_SWAP_PET`.
    StableSwapPet {
        npc: u64,
        pet_number: u32,
    },
    /// `CMSG_BUY_STABLE_SLOT`; a refusal is `SMSG_STABLE_RESULT`'s `ERR_MONEY`.
    BuyStableSlot {
        npc: u64,
    },
    /// `CMSG_LEARN_TALENT`: a `Talent.dbc` row and the 0-based rank, which is the current rank
    /// count. No reply beyond the learn and `PLAYER_CHARACTER_POINTS1`.
    LearnTalent {
        talent_id: u32,
        rank: u32,
    },
    /// `CMSG_UNLEARN_SKILL`. No ack: the line leaves only by the `PLAYER_SKILL_INFO` update.
    UnlearnSkill {
        skill_id: u32,
    },
    /// `CMSG_SET_FACTION_ATWAR`, by reputation-list slot. No ack; vmangos drops it in combat.
    SetFactionAtWar {
        rep_list_id: u32,
        at_war: bool,
    },
    /// `CMSG_SET_FACTION_INACTIVE`, by reputation-list slot. No ack.
    SetFactionInactive {
        rep_list_id: u32,
        inactive: bool,
    },
    /// `CMSG_SET_WATCHED_FACTION`: `-1` watches nothing (slot 0 is a real faction). Answered by
    /// `PLAYER_FIELD_WATCHED_FACTION_INDEX`.
    SetWatchedFaction {
        rep_list_id: i32,
    },
    /// `CMSG_GAMEOBJ_USE`: the server answers by GameObject type (a chest with loot, a door with a
    /// `GAMEOBJECT_STATE` flip) or refuses silently.
    GameObjUse {
        guid: u64,
    },
    /// `CMSG_AREATRIGGER`: we entered an `AreaTrigger.dbc` volume. A teleport answers with the
    /// ordinary transfer, a refusal with `SMSG_AREA_TRIGGER_MESSAGE`, most with nothing.
    AreaTrigger {
        trigger_id: u32,
    },
    /// `CMSG_GAMEOBJECT_QUERY`, asked once per entry ([`crate::go_templates`]).
    GameObjectQuery {
        entry: u32,
        guid: u64,
    },
    /// `CMSG_PAGE_TEXT_QUERY` (the server discards `guid`); vmangos answers the whole forward
    /// chain, one response per page.
    PageTextQuery {
        page_id: u32,
        guid: u64,
    },
    /// `CMSG_CAST_SPELL` at a GameObject: an OPEN_LOCK cast on a lock, vein or herb.
    CastSpellGameObject {
        spell_id: u32,
        go_guid: u64,
    },
    /// An item-targeted cast (`TARGET_FLAG_ITEM` and a packed guid).
    CastSpellItem {
        spell_id: u32,
        item_guid: u64,
    },
    /// `CMSG_LOOT` on a unit; the server rejects a GameObject guid, which loots by `GameObjUse`.
    Loot {
        guid: u64,
    },
    /// `CMSG_AUTOSTORE_LOOT_ITEM` by the 0-based wire loot slot.
    AutostoreLootItem {
        slot: u8,
    },
    /// `CMSG_LOOT_MONEY`: answered by `SMSG_LOOT_CLEAR_MONEY`; solo, no `SMSG_LOOT_MONEY_NOTIFY`.
    LootMoney,
    /// `CMSG_LOOT_RELEASE`: the server ignores `guid` and releases whatever loot it holds for us.
    LootRelease {
        guid: u64,
    },
    /// `CMSG_LOOT_ROLL`, by the `(looted_target, item_slot)` pair; the Lua `rollID` is client-only.
    LootRoll {
        looted_target: u64,
        item_slot: u32,
        roll_type: u8,
    },
    /// `CMSG_LOOT_MASTER_GIVE`: `slot` is the wire slot. Success is `SMSG_LOOT_REMOVED`, a refusal
    /// a `MASTER_*` loot error.
    LootMasterGive {
        guid: u64,
        slot: u8,
        target: u64,
    },
    /// `CMSG_QUESTGIVER_QUERY_QUEST`; answered by `SMSG_QUESTGIVER_QUEST_DETAILS`.
    QuestgiverQuery {
        npc: u64,
        quest: u32,
    },
    /// `CMSG_QUESTGIVER_ACCEPT_QUEST`; the server closes gossip with `SMSG_GOSSIP_COMPLETE`.
    QuestgiverAccept {
        npc: u64,
        quest: u32,
    },
    /// `CMSG_QUESTGIVER_COMPLETE_QUEST`; answered by `SMSG_QUESTGIVER_REQUEST_ITEMS`, or
    /// `OFFER_REWARD` with no required items.
    QuestgiverComplete {
        npc: u64,
        quest: u32,
    },
    /// `CMSG_QUESTGIVER_REQUEST_REWARD`; answered by `SMSG_QUESTGIVER_OFFER_REWARD`.
    QuestgiverRequestReward {
        npc: u64,
        quest: u32,
    },
    /// `CMSG_QUESTGIVER_CHOOSE_REWARD` with the 0-based choice.
    QuestgiverChooseReward {
        npc: u64,
        quest: u32,
        choice: u32,
    },
    /// `CMSG_QUEST_QUERY`: the quest log's ask-once template, needing no NPC.
    QuestQuery {
        quest: u32,
    },
    /// `CMSG_QUESTGIVER_STATUS_QUERY`: the overhead `!`/`?` marker.
    QuestgiverStatusQuery {
        npc: u64,
    },
    /// `CMSG_QUESTGIVER_HELLO`: re-opens a non-gossip NPC's quest list after a decline.
    QuestgiverHello {
        npc: u64,
    },
    /// `CMSG_QUESTLOG_REMOVE_QUEST`. No ack: the `PLAYER_QUEST_LOG` slot fields clear.
    QuestlogRemove {
        slot: u8,
    },
    // ── The party quest share ──
    /// `CMSG_PUSHQUESTTOPARTY`; answered by `MSG_QUEST_PUSH_RESULT`s per member.
    PushQuestToParty {
        quest: u32,
    },
    /// `CMSG_QUEST_CONFIRM_ACCEPT`, the escort confirm's Yes; dismissing sends nothing.
    QuestConfirmAccept {
        quest: u32,
    },
    /// `MSG_QUEST_PUSH_RESULT`: our decline of a shared quest, to `sharer`.
    QuestPushResult {
        sharer: u64,
        /// The shared quest; build 12340 sends it and AC ignores it. 0 when no panel shows one.
        quest: u32,
        msg: benilla_protocol::messages::QuestShareMsg,
    },
    // ── Mail ──
    /// `CMSG_GET_MAIL_LIST`: the window's open and refresh.
    GetMailList {
        mailbox: u64,
    },
    /// `CMSG_SEND_MAIL`; `item_guid` 0 is no attachment.
    SendMail {
        mailbox: u64,
        receiver: String,
        subject: String,
        body: String,
        /// The `Stationery.dbc` id, the sixth field.
        stationery: u32,
        /// The `Package.dbc` id, the seventh field, 0 without an item.
        package: u32,
        item_guid: u64,
        money: u32,
        cod: u32,
    },
    /// `CMSG_MAIL_TAKE_MONEY`.
    MailTakeMoney {
        mailbox: u64,
        mail_id: u32,
    },
    /// `CMSG_MAIL_TAKE_ITEM`.
    MailTakeItem {
        mailbox: u64,
        mail_id: u32,
    },
    /// `CMSG_MAIL_MARK_AS_READ`, when a letter opens. No response.
    MailMarkAsRead {
        mailbox: u64,
        mail_id: u32,
    },
    /// `CMSG_MAIL_RETURN_TO_SENDER`.
    MailReturnToSender {
        mailbox: u64,
        mail_id: u32,
    },
    /// `CMSG_MAIL_DELETE`.
    MailDelete {
        mailbox: u64,
        mail_id: u32,
    },
    /// `CMSG_MAIL_CREATE_TEXT_ITEM`; answered by `SMSG_SEND_MAIL_RESULT` with `MADE_PERMANENT`.
    MailCreateTextItem {
        mailbox: u64,
        mail_id: u32,
    },
    /// `CMSG_ITEM_TEXT_QUERY`, once per mail with a nonzero `item_text_id`.
    ItemTextQuery {
        text_id: u32,
        mail_id: u32,
    },
    /// `MSG_QUERY_NEXT_MAIL_TIME`, on entering the world, to seed `HasNewMail()`.
    QueryNextMailTime,
    // ── The auction house ──
    // Every verb carries the auctioneer guid, range-checked (5 yd) each time. Several refusals
    // get no packet: a zero bid or duration, an unaffordable bid, a cancel whose cut cannot be
    // paid, a list request in flight.
    /// `MSG_AUCTION_HELLO`: the reply, with the `AuctionHouse.dbc` id, opens the window.
    AuctionHello {
        auctioneer: u64,
    },
    /// `CMSG_AUCTION_LIST_ITEMS`: unset filters use `auction_filter`'s sentinels. No sort rides
    /// the wire; `list_from` pages by 50.
    AuctionListItems {
        auctioneer: u64,
        list_from: u32,
        searched_name: String,
        level_min: u8,
        level_max: u8,
        slot_id: u32,
        main_category: u32,
        sub_category: u32,
        quality: u32,
        usable: u8,
    },
    /// `CMSG_AUCTION_LIST_OWNER_ITEMS`: our own listings.
    AuctionListOwnerItems {
        auctioneer: u64,
        list_from: u32,
    },
    /// `CMSG_AUCTION_LIST_BIDDER_ITEMS`: `auction_ids` are listed first, then every auction we
    /// hold the bid on; not a filter.
    AuctionListBidderItems {
        auctioneer: u64,
        list_from: u32,
        auction_ids: Vec<u32>,
    },
    /// `CMSG_AUCTION_SELL_ITEM`: `etime_minutes` is 120, 480 or 1440; the deposit is taken at once.
    AuctionSellItem {
        auctioneer: u64,
        item_guid: u64,
        bid: u32,
        buyout: u32,
        etime_minutes: u32,
        /// The stack size sold; build 12340 sends `(guid, count)` pairs and AC refuses a count
        /// above the stack.
        count: u32,
    },
    /// `CMSG_AUCTION_PLACE_BID`: a `price` at or above a nonzero buyout is the buyout.
    AuctionPlaceBid {
        auctioneer: u64,
        auction_id: u32,
        price: u32,
    },
    /// `CMSG_AUCTION_REMOVE_ITEM`: the deposit is forfeit; with a bid, the 5% cut is charged.
    AuctionRemoveItem {
        auctioneer: u64,
        auction_id: u32,
    },
    /// `CMSG_QUERY_TIME`, answered into [`ServerWallClock`].
    QueryTime,
    /// `CMSG_INSPECT`: the window paints from the streamed `PLAYER_VISIBLE_ITEM_*` fields; the
    /// send also sets our selection server-side (`MiscHandler.cpp:945`).
    Inspect {
        target: u64,
    },
    /// `MSG_INSPECT_HONOR_STATS`: the only source of another player's honor, whose fields are
    /// private. A refusal is silence.
    InspectHonorStats {
        target: u64,
    },
    // ── Player trade ──
    /// `CMSG_INITIATE_TRADE`: a refusal answers us; success sends the target `BEGIN_TRADE`.
    InitiateTrade {
        target: u64,
    },
    /// `CMSG_BEGIN_TRADE`: accept a request; the server sends `OPEN_WINDOW` to both. Sent only by
    /// [`crate::ui_trade::answer_trade_request`].
    BeginTrade,
    /// `CMSG_IGNORE_TRADE` (`0x119`): the refusal ladder's ignore leg (`0x4bf759` to `0x5d41c0`);
    /// vmangos answers the initiator `TRADE_STATUS_IGNORE_YOU`.
    IgnoreTrade,
    /// `CMSG_BUSY_TRADE` (`0x118`, sent by `0x5d4150`): five of the ladder's eight refusal legs;
    /// vmangos sends `TRADE_STATUS_BUSY` to both. The player's own No is [`Self::CancelTrade`].
    BusyTrade,
    /// `CMSG_ACCEPT_TRADE`: the Trade button.
    AcceptTrade,
    /// `CMSG_UNACCEPT_TRADE`: drop our accept, stay in the trade.
    UnacceptTrade,
    /// `CMSG_CANCEL_TRADE`.
    CancelTrade,
    /// `CMSG_SET_TRADE_GOLD`: clears both accepts and re-arms the server's 200 ms delay; echoed
    /// in `SMSG_TRADE_STATUS_EXTENDED`.
    SetTradeGold {
        copper: u32,
    },
    /// `CMSG_SET_TRADE_ITEM`: `trade_slot` 0..=6, 6 the not-traded enchant slot.
    SetTradeItem {
        trade_slot: u8,
        bag: u8,
        slot: u8,
    },
    /// `CMSG_CLEAR_TRADE_ITEM`, 0-based slot.
    ClearTradeItem {
        trade_slot: u8,
    },
    /// `CMSG_LOGOUT_REQUEST`: `SMSG_LOGOUT_COMPLETE` becomes a [`LoggedOutMessage`] and a
    /// reconnect to character select.
    Logout,
    /// `CMSG_LOGOUT_CANCEL`; acked by `SMSG_LOGOUT_CANCEL_ACK`.
    LogoutCancel,
    /// `CMSG_JOIN_CHANNEL`: `/join` and the zone auto-join.
    JoinChannel {
        name: String,
        password: String,
        /// The `ChatChannels.dbc` row of a zone channel, 0 for a custom one; build 12340 sends it
        /// and AC joins the constant channel by it.
        channel_id: u32,
    },
    /// `CMSG_LEAVE_CHANNEL`.
    LeaveChannel {
        name: String,
    },
    /// `CMSG_CHANNEL_LIST`.
    ChannelList {
        name: String,
    },
    /// Native Wrath `CMSG_SET_CHANNEL_WATCH`; empty name clears the watch.
    SetChannelWatch {
        name: String,
    },
    /// Native Wrath `CMSG_CHANNEL_DISPLAY_LIST`, distinct from the ordinary chat list.
    ChannelDisplayList {
        name: String,
    },
    /// Native LFD lock queries; never emitted automatically by capture.
    LfgRequestPlayerLockInfo,
    LfgRequestPartyLockInfo,
    /// Raw Wrath L2/L3 transport. No UI producer until App owns the guarded lifecycle.
    LfgCommand(benilla_protocol::wrath_lfg::commands::Command),
    /// `/random [min] [max]` (`MSG_RANDOM_ROLL`).
    RandomRoll {
        min: u32,
        max: u32,
    },
    /// `DisplayChannelOwner`: `CMSG_CHANNEL_OWNER`.
    ChannelOwner {
        name: String,
    },
    /// `SetChannelOwner`: `CMSG_CHANNEL_SET_OWNER`.
    ChannelSetOwner {
        name: String,
        player: String,
    },
    /// `SetChannelPassword`: `CMSG_CHANNEL_PASSWORD`; empty clears.
    ChannelPassword {
        name: String,
        password: String,
    },
    ChannelModerator {
        name: String,
        player: String,
    },
    ChannelUnmoderator {
        name: String,
        player: String,
    },
    ChannelMute {
        name: String,
        player: String,
    },
    ChannelUnmute {
        name: String,
        player: String,
    },
    ChannelInvite {
        name: String,
        player: String,
    },
    ChannelKick {
        name: String,
        player: String,
    },
    ChannelBan {
        name: String,
        player: String,
    },
    ChannelUnban {
        name: String,
        player: String,
    },
    /// `ChannelToggleAnnouncements`: `CMSG_CHANNEL_ANNOUNCEMENTS`.
    ChannelAnnouncements {
        name: String,
    },
    /// `ChannelModerate`: `CMSG_CHANNEL_MODERATE`.
    ChannelModerate {
        name: String,
    },
    /// `/played` (`CMSG_PLAYED_TIME`).
    PlayedTime,
    /// `OpeningCinematic()`: `CMSG_OPENING_CINEMATIC`, empty.
    OpeningCinematic,
    /// `CMSG_COMPLETE_CINEMATIC`, at the end or skip, or at once for an unresolvable trigger.
    /// Unacked, vmangos keeps visibility on the cinematic camera and nearby NPCs despawn.
    CompleteCinematic,
    /// `CMSG_NEXT_CINEMATIC_CAMERA`, for every shot including the first: the send is in the shot
    /// arm (`0x48ef11`), so a single-camera intro sends one.
    NextCinematicCamera,
    /// Acknowledge a root, water-walk, feather-fall or hover on [`MoveMode::ack_opcode`];
    /// unacked, the server never applies it. `flags` must carry the mode bit: vmangos kicks a
    /// root ack without `MOVEFLAG_ROOT` and takes the word as the new flags for the others.
    MoveModeAck {
        guid: u64,
        counter: u32,
        mode: MoveMode,
        apply: bool,
        flags: u32,
        pos: [f32; 3],
        orientation: f32,
    },
    /// `CMSG_MOVE_KNOCK_BACK_ACK`, `launch` echoed as the jump tail. Sent on the frame the mover
    /// takes off, never on receipt: the reference applies (`0x61624d call 0x6179c0`) then sends
    /// (`0x616261`) in one call, so the wire pose is post-launch.
    KnockBackAck {
        guid: u64,
        counter: u32,
        launch: JumpInfo,
        flags: u32,
        pos: [f32; 3],
        orientation: f32,
        transport: Option<TransportPose>,
    },
    /// `CMSG_REPOP_REQUEST`: Release Spirit.
    RepopRequest,
    /// `MSG_CORPSE_QUERY`, on becoming a ghost and on login dead.
    CorpseQuery,
    /// `CMSG_RECLAIM_CORPSE`: the server requires a ghost, the delay elapsed and 39 yd.
    ReclaimCorpse {
        corpse: u64,
    },
    /// `CMSG_SELF_RES`: the server casts `PLAYER_SELF_RES_SPELL`. No answer packet.
    SelfRes,
    /// `CMSG_SPIRIT_HEALER_ACTIVATE`: 50% res, 25% durability, sickness from level 11.
    SpiritHealerActivate {
        npc: u64,
    },
    /// `CMSG_RESURRECT_RESPONSE`.
    ResurrectResponse {
        caster: u64,
        accept: bool,
    },
    // ── Group ──
    /// `CMSG_GROUP_INVITE` by name; acked by `SMSG_PARTY_COMMAND_RESULT`.
    GroupInvite {
        name: String,
    },
    /// `CMSG_GROUP_ACCEPT`.
    GroupAccept,
    /// `CMSG_GROUP_DECLINE`.
    GroupDecline,
    /// `CMSG_GROUP_UNINVITE` by name, leader only.
    GroupUninvite {
        name: String,
    },
    /// `CMSG_GROUP_SET_LEADER`, leader only.
    GroupSetLeader {
        guid: u64,
    },
    /// `CMSG_GROUP_DISBAND`: leave the group, despite the name.
    GroupLeave,
    /// Wrath MSG_PARTY_ASSIGNMENT, full guid; never translated to a Vanilla packet.
    PartyAssignment {
        assignment: u8,
        apply: bool,
        guid: u64,
    },
    /// `CMSG_GROUP_RAID_CONVERT`, leader only.
    GroupRaidConvert,
    /// `CMSG_REQUEST_PARTY_MEMBER_STATS`, only on the reference's two edges: a member's object
    /// leaves (`0x4e8646`), or a new roster member's object is not held (`0x4e83f1`). No timer.
    RequestPartyMemberStats {
        guid: u64,
    },
    /// `CMSG_LOOT_METHOD`: `method` 0..4, `threshold` quality 2..4. Leader only.
    LootMethod {
        method: u32,
        master: u64,
        threshold: u32,
    },
    /// `MSG_RAID_TARGET_UPDATE`: `icon` 0..7, `guid` 0 clears it. Leader or assistant only.
    SetRaidTarget {
        icon: u8,
        guid: u64,
    },
    /// `MSG_MINIMAP_PING`, raw world `(x, y)`. The server relays it to the rest of the group and
    /// ignores it solo, so our own marker is drawn locally.
    MinimapPing {
        x: f32,
        y: f32,
    },
    // ── Raid management ──
    /// `CMSG_GROUP_CHANGE_SUB_GROUP`: by name, 0-based subgroup. Leader or assistant only.
    GroupChangeSubGroup {
        name: String,
        group: u8,
    },
    /// `CMSG_GROUP_SWAP_SUB_GROUP`, two names.
    GroupSwapSubGroup {
        name: String,
        other: String,
    },
    /// `CMSG_GROUP_ASSISTANT_LEADER`, leader only.
    GroupAssistantLeader {
        guid: u64,
        grant: bool,
    },
    /// `MSG_RAID_READY_CHECK` with an empty body, leader only.
    ReadyCheckStart,
    /// `MSG_RAID_READY_CHECK` with one byte.
    ReadyCheckAnswer {
        ready: bool,
    },
    /// Native arena-team transport only; no UI producer or inferred authority.
    NativeChatCommand(benilla_protocol::wrath_chat::Command),
    /// Explicit transport only; no shared WHO UI producer or permission inference.
    NativeWhoCommand(benilla_protocol::wrath_who::Command),
    /// Transport-only native mail; no legacy mail conversion or UI producer.
    NativeQuestsCompletedQuery,
    NativeUiTimerQuery,
    NativeMirrorImageQuery {
        guid: u64,
    },
    /// Native instance controls; transport does not infer leadership, locks or permission.
    NativeInstanceControl(benilla_protocol::wrath_instance_control::Command),
    /// Native item/inventory commands with no Vanilla twin; transport does not claim acceptance.
    NativeInventory(benilla_protocol::wrath_inventory::Command),
    /// Native auction/loot commands with no Vanilla twin; transport does not claim acceptance.
    NativeExchange(benilla_protocol::wrath_exchange::Command),
    /// Native spell-control and pet requests with no Vanilla twin (spell click, totem destroy,
    /// cancel mount or growth aura, pet info, pet talents); transport does not claim acceptance.
    NativeSpellControl(benilla_protocol::wrath_spell_control::Command),
    /// Native movement requests with no Vanilla twin (`/timetest`); transport does not claim acceptance.
    NativeMovementMisc(benilla_protocol::wrath_movement_misc::Command),
    /// `CMSG_QUERY_INSPECT_ACHIEVEMENTS`; no UI producer yet, the reply is capture-only.
    NativeInspectAchievements { player: u64 },
    /// Native calendar commands; no UI producer yet, replies are capture-only.
    NativeCalendar(benilla_protocol::wrath_calendar::Command),
    /// Native quest commands with no Vanilla twin; transport does not claim acceptance.
    NativeQuest(benilla_protocol::wrath_quest::Command),
    /// Native party commands with no Vanilla twin; transport does not claim leadership.
    NativeParty(benilla_protocol::wrath_party::Command),
    /// Native battleground/arena/battlefield commands keyed by this build's bg type; no UI producer.
    NativeBattleground(benilla_protocol::wrath_battleground::Command),
    /// Native death/corpse commands with no shared producer (hearth-and-resurrect, map position).
    NativeDeath(benilla_protocol::wrath_death::Command),
    /// Native zone-update and account-data commands with no shared producer.
    NativeSessionMeta(benilla_protocol::wrath_session_meta::Command),
    /// Native Help Request commands with no shared producer: GM survey, response resolve,
    /// complaints and bug reports.
    NativeHelp(benilla_protocol::wrath_help::Command),
    /// Native refer-a-friend level grant and accept; no UI producer yet.
    NativeReferFriend(benilla_protocol::wrath_refer_friend::Command),
    /// Native game-object report-use; `GameObjUse` is the shared use command.
    NativeWorldObjects(benilla_protocol::wrath_world_objects::Command),
    /// Native player-logout and emote commands with no shared producer.
    NativeSessionMisc(benilla_protocol::wrath_session_misc::Command),
    /// Native preview-talent learning and glyph removal.
    NativeInspect(benilla_protocol::wrath_inspect::Command),
    /// Native guild-bank mutations: banker activate, buy tab, deposit and withdraw money.
    NativeGuildBank(benilla_protocol::wrath_guild_bank_commands::Command),
    /// `CMSG_GUILD_BANK_SWAP_ITEMS`: a deposit, move or withdrawal.
    NativeGuildBankSwap(benilla_protocol::wrath_guild_bank_swap::Swap),
    /// `CMSG_GUILD_BANK_UPDATE_TAB`: a tab's name and icon.
    NativeGuildBankTab(benilla_protocol::wrath_guild_bank_tab::UpdateTab),
    /// The tab text and log queries and the text save.
    NativeGuildBankText(benilla_protocol::wrath_guild_bank_text::Command),
    /// `CMSG_SHOWING_HELM`/`CMSG_SHOWING_CLOAK` (the desired state) and `CMSG_ALTER_APPEARANCE`.
    NativeAppearance(benilla_protocol::wrath_appearance::Command),
    /// `CMSG_GET_CHANNEL_MEMBER_COUNT`.
    GetChannelMemberCount { name: String },
    /// `CMSG_CLEAR_CHANNEL_WATCH`: stop a channel's roster deltas.
    ClearChannelWatch { name: String },
    /// `CMSG_DECLINE_CHANNEL_INVITE` (AC reads nothing from it).
    DeclineChannelInvite { name: String },
    /// The raid browser's search join and leave.
    NativeRaidBrowser(benilla_protocol::wrath_raid_browser::Command),
    /// `CMSG_SET_CONTACT_NOTES`: a friend's note.
    SetContactNotes { guid: u64, note: String },
    NativeMailMutation(benilla_protocol::wrath_mail_mutations::Command),
    ArenaTeamCommand(benilla_protocol::wrath_arena_team::Command),
    /// Native Wrath guild-bank read request; no UI producer until owner guards exist.
    GuildBankQuery(benilla_protocol::wrath_guild_bank_query::Query),
    /// `CMSG_REQUEST_RAID_INFO`.
    RequestRaidInfo,
    /// `CMSG_RESET_INSTANCES`; answered per instance, `SMSG_INSTANCE_RESET` or `_FAILED`.
    ResetInstances,
    // ── Duel ── (a challenge is a `CastSpell` of the duel spell)
    /// `CMSG_DUEL_ACCEPTED`, also the challenger's own auto-accept.
    DuelAccepted {
        arbiter: u64,
    },
    /// `CMSG_DUEL_CANCELLED`: decline, cancel and forfeit alike.
    DuelCancelled {
        arbiter: u64,
    },
    // ── Social ── (added by name, removed by guid)
    /// `CMSG_FRIEND_LIST`: `ShowFriends()`.
    FriendListRequest,
    /// `CMSG_ADD_FRIEND`.
    AddFriend {
        name: String,
    },
    /// `CMSG_SET_LOOKING_FOR_GROUP`, only when the commit changed something.
    SetLookingForGroup {
        slots: [u32; 3],
        comment: String,
    },
    /// `CMSG_DEL_FRIEND`.
    DelFriend {
        guid: u64,
    },
    /// `CMSG_ADD_IGNORE`.
    AddIgnore {
        name: String,
    },
    /// `CMSG_DEL_IGNORE`.
    DelIgnore {
        guid: u64,
    },
    /// `CMSG_CHAT_IGNORED`: the server still delivers ignored chat, so the client drops it and
    /// reports, and the sender hears they are ignored.
    ChatIgnored {
        guid: u64,
    },
    /// `CMSG_WHO`, parsed into wire fields by `ui_social::who_query`.
    Who {
        request: Box<WhoRequest>,
    },
    /// `CMSG_TOGGLE_PVP`: nothing changes locally; the answer is the PvP bit, and flagging off
    /// waits out the server's 300 s timer.
    TogglePvp,
    /// `CMSG_TOGGLE_HELM`: nothing changes locally; the answer is `PLAYER_FLAGS`' `HIDE_HELM`.
    ToggleHelm,
    /// `CMSG_TOGGLE_CLOAK`, as [`Self::ToggleHelm`].
    ToggleCloak,
    // ── Guild ──
    // Members by name; rank 0 is the guild master. A change is answered by a fresh
    // `SMSG_GUILD_ROSTER`, never applied at the send; a refusal by `SMSG_GUILD_COMMAND_RESULT`.
    /// `CMSG_GUILD_QUERY`, the ask-once guild name cache.
    GuildQuery {
        guild_id: u32,
    },
    /// `CMSG_GUILD_CREATE`: vmangos registers it `STATUS_NEVER` (founding is the charter flow).
    #[allow(dead_code)]
    GuildCreate {
        name: String,
    },
    /// `CMSG_GUILD_INVITE`.
    GuildInvite {
        name: String,
    },
    /// `CMSG_GUILD_ACCEPT`: the server holds which invite.
    GuildAccept,
    /// `CMSG_GUILD_DECLINE`.
    GuildDecline,
    /// `CMSG_GUILD_INFO`: founding date and counts.
    GuildInfoRequest,
    /// `CMSG_GUILD_ROSTER`.
    GuildRosterRequest,
    /// `CMSG_GUILD_PROMOTE`.
    GuildPromote {
        name: String,
    },
    /// `CMSG_GUILD_DEMOTE`.
    GuildDemote {
        name: String,
    },
    /// `CMSG_GUILD_LEAVE`: refused to a guild master while others remain.
    GuildLeave,
    /// `CMSG_GUILD_REMOVE`.
    GuildRemove {
        name: String,
    },
    /// `CMSG_GUILD_DISBAND`, guild master only.
    GuildDisband,
    /// `CMSG_GUILD_LEADER`.
    GuildLeader {
        name: String,
    },
    /// `CMSG_GUILD_MOTD`; empty clears.
    GuildMotd {
        motd: String,
    },
    /// `CMSG_GUILD_RANK`: name and rights always together. Rank 0's rights are forced to all; a
    /// name over `messages::GUILD_RANK_MAX_LENGTH` gets the session kicked.
    GuildRank {
        rank_id: u32,
        rights: u32,
        name: String,
    },
    /// `CMSG_GUILD_ADD_RANK`: appended at the bottom.
    GuildAddRank {
        name: String,
    },
    /// `CMSG_GUILD_DEL_RANK`: always the lowest rank; no id on the wire.
    GuildDelRank,
    /// `CMSG_GUILD_SET_PUBLIC_NOTE`.
    GuildSetPublicNote {
        name: String,
        note: String,
    },
    /// `CMSG_GUILD_SET_OFFICER_NOTE`; editing and viewing officer notes are separate rights.
    GuildSetOfficerNote {
        name: String,
        note: String,
    },
    /// `CMSG_GUILD_INFO_TEXT`; returns as the roster's `info` field.
    GuildInfoText {
        text: String,
    },
    /// `QueryGuildEventLog()`: `MSG_GUILD_EVENT_LOG_QUERY`.
    GuildEventLogQuery,
    /// `GuildControlSaveRank` in 3.3.5: the rank with its bank money and tab rights.
    NativeGuildRank(benilla_protocol::wrath_guild_rank::SaveRank),
    // ── Petitions (founding a guild) ──
    // A charter is addressed by its item guid except in the record query. A refusal is
    // `SMSG_GUILD_COMMAND_RESULT`; almost nothing is acked, so nothing is applied at the send.
    /// `CMSG_PETITION_SHOWLIST`.
    PetitionShowList {
        npc: u64,
    },
    /// `CMSG_PETITION_BUY`: success is only the new item.
    PetitionBuy {
        npc: u64,
        name: String,
    },
    /// `CMSG_PETITION_SHOW_SIGNATURES`, on use and after a signature lands.
    PetitionShowSignatures {
        item: u64,
    },
    /// `CMSG_PETITION_SIGN`: `byte` is the optional Lua argument (default 1), which vmangos drops.
    PetitionSign {
        item: u64,
        byte: i8,
    },
    /// `CMSG_OFFER_PETITION`: success answers the target, not us.
    OfferPetition {
        item: u64,
        player: u64,
    },
    /// `CMSG_TURN_IN_PETITION`, owner only; a name collision answers a guild command result only.
    TurnInPetition {
        item: u64,
    },
    /// `CMSG_PETITION_QUERY`: the only source of the guild name and signature requirement.
    PetitionQuery {
        petition_id: u32,
        item: u64,
    },
    /// `MSG_PETITION_RENAME`; echoed only on success.
    PetitionRename {
        item: u64,
        name: String,
    },
    /// `MSG_PETITION_DECLINE`: we send the item guid; the owner receives ours.
    #[allow(dead_code)]
    PetitionDecline {
        item: u64,
    },
    // ── Taxi ──
    /// `CMSG_TAXINODE_STATUS_QUERY` with the flight master's guid. Not sent yet.
    #[allow(dead_code)]
    TaxiNodeStatusQuery {
        guid: u64,
    },
    /// `CMSG_TAXIQUERYAVAILABLENODES`: a pure flight master's opener (gossip wins on a gossip NPC).
    /// A known node answers `SMSG_SHOWTAXINODES`; a new one the learn pair, and opens nothing.
    TaxiQueryNodes {
        guid: u64,
    },
    /// `CMSG_ACTIVATETAXI`: one hop; answered by `SMSG_ACTIVATETAXIREPLY`.
    ActivateTaxi {
        guid: u64,
        source_node: u32,
        dest_node: u32,
    },
    /// `CMSG_ACTIVATETAXIEXPRESS`: sent when no direct `TaxiPath` edge joins the two nodes, not by
    /// hop count; the combined fare and the node chain in order.
    ActivateTaxiExpress {
        guid: u64,
        total_cost: u32,
        nodes: Vec<u32>,
    },
}

/// The realm list (`CMD_REALM_LIST`); the IO thread waits for [`crate::realm_select`] to pick.
#[derive(Message)]
pub(crate) struct RealmListMessage {
    pub(crate) realms: Vec<benilla_protocol::RealmInfo>,
}

/// The character roster (`SMSG_CHAR_ENUM`); the IO thread waits for [`crate::char_select`] to
/// pick. `realm` is the realm this session connected to.
#[derive(Message)]
pub(crate) struct CharListMessage {
    pub(crate) characters: Vec<benilla_protocol::Character>,
    pub(crate) realm: Option<benilla_protocol::RealmInfo>,
}

/// A character create or delete result; a refreshed [`CharListMessage`] precedes a success.
/// `code` is the raw `WorldResult` byte.
#[derive(Message, Clone, Copy)]
pub(crate) struct CharActionResultMessage {
    pub(crate) action: benilla_protocol::CharAction,
    pub(crate) code: u8,
}

/// Native build-12340 result; successful operations publish the refreshed roster first.
/// Deliberately separate from Vanilla's result-byte namespace.
#[derive(Message, Clone, Copy)]
pub(crate) struct WrathCharActionResultMessage {
    pub(crate) action: benilla_protocol::CharAction,
    pub(crate) code: u8,
}

/// `SMSG_CHARACTER_LOGIN_FAILED`: the picked character was refused. `result` is the raw reason,
/// read by [`crate::char_select::char_login_refusal_text`].
#[derive(Message, Clone, Copy)]
pub(crate) struct CharacterLoginFailedMessage {
    pub(crate) result: u8,
}

/// We entered the world.
#[derive(Message)]
pub(crate) struct EnteredWorldMessage {
    /// Rested billing minutes from `SMSG_AUTH_RESPONSE`, the only time they arrive.
    pub(crate) billing_time_rested: u32,
    /// The tutorial flags, if `SMSG_TUTORIAL_FLAGS` came during the login handshake.
    pub(crate) tutorial_flags: Option<Vec<u8>>,
}

/// `SMSG_ADDON_INFO`: the addons the server hid from the Lua index space, or `None` if it did
/// not answer. A resource, since it must exist before the world-entry UI load runs any addon;
/// rewritten on every login.
#[derive(Resource, Default)]
pub(crate) struct AddonInfoReply(pub(crate) Option<Vec<String>>);

/// `SMSG_TRIGGER_CINEMATIC`: a `CinematicSequences.dbc` id; [`crate::cinematic`] plays and acks it.
#[derive(Message)]
pub(crate) struct CinematicTriggeredMessage {
    pub(crate) cinematic_id: u32,
}

/// One `CHAT_MSG_SYSTEM` line, such as a GM command's answer: the only evidence of server state
/// with no field, like vmangos god mode (`Unit::m_invincibilityHpThreshold`).
#[derive(Message, Clone)]
pub(crate) struct ServerSaidMessage {
    pub(crate) text: String,
}

/// `SMSG_LOGOUT_COMPLETE`: back to character select. The drain tears down our own entity.
#[derive(Message)]
pub(crate) struct LoggedOutMessage;

/// A login stage, shown as its `LOGIN_STATE_*` string.
#[derive(Message, Clone, Copy)]
pub(crate) struct LoginStageMessage {
    pub(crate) stage: benilla_protocol::LoginStage,
}

/// Queued for a full realm, one per `AUTH_WAIT_QUEUE`; the attempt is still live.
#[derive(Message, Clone)]
pub(crate) struct LoginQueuedMessage {
    pub(crate) position: Option<u32>,
    pub(crate) realm: Option<String>,
}

/// The session ended; the IO thread is back at its pre-logon park.
#[derive(Message, Clone)]
pub(crate) struct DisconnectedMessage {
    pub(crate) reason: String,
    pub(crate) end: benilla_protocol::SessionEnd,
    /// The reference's `DISCONNECTED_FROM_SERVER`: back at the account screen, nothing retried.
    /// Decided once in [`Self::new`], so its four readers (teardown, screen, cover, credentials)
    /// cannot disagree.
    pub(crate) session_over: bool,
}

impl DisconnectedMessage {
    /// `session_over` is false after a logout (the relist is the select the player asked for) and
    /// in a run declared unattended ([`crate::run_mode::unattended`]), which reconnects. Not the
    /// login env vars: a person's launch line sets them too, and a reconnect would take the
    /// account back from whoever displaced us.
    pub(crate) fn new(reason: String, end: benilla_protocol::SessionEnd) -> Self {
        let session_over =
            end == benilla_protocol::SessionEnd::Lost && !crate::run_mode::unattended();
        Self {
            reason,
            end,
            session_over,
        }
    }
}

/// A login attempt failed before the roster; the IO thread is back at its pre-logon park.
/// `terminal` is a failure retrying cannot fix (the server requires Warden, say): never resubmit.
#[derive(Message, Clone)]
pub(crate) struct LoginFailedMessage {
    /// Which server refused us, with which byte; `None` on a transport failure.
    pub(crate) refusal: Option<benilla_protocol::LoginRefusal>,
    pub(crate) reason: String,
    pub(crate) terminal: bool,
    /// The dial that never opened a socket, so the screen can tell an unknown name from a host
    /// that does not answer.
    pub(crate) dial: Option<benilla_protocol::DialFailure>,
}

/// A same-map teleport of our player: snap, then ack. Read by the controller in `Input`.
#[derive(Message, Clone, Copy)]
pub(crate) struct TeleportMessage {
    pub(crate) guid: u64,
    pub(crate) counter: u32,
    pub(crate) position: [f32; 3],
    pub(crate) orientation: f32,
}

/// An inbound `MSG_MOVE_*` for our own mover: always server-authored (vmangos
/// `MovementInfo::SetAsServerSide`), from GM moves and the anticheat snap-back, with no ack owed.
/// The reference applies it like a remote's, having no mover-guid gate (`0x603bb0`); ours lands
/// in `player::wire_in`.
#[derive(Message, Clone, Copy)]
pub(crate) struct SelfMoveMessage {
    pub(crate) position: [f32; 3],
    pub(crate) orientation: f32,
    /// Merged under [`crate::creature_anim::move_flags::SERVER_AUTHORED`], never assigned.
    pub(crate) flags: u32,
    pub(crate) pitch: f32,
    pub(crate) fall_time: u32,
    pub(crate) jump: Option<benilla_protocol::JumpInfo>,
    /// The server's deck-local pose for us. Never re-derive it from the boat's transform, which
    /// [`crate::transport::tick_transports`] holds at the far pose across a map seam.
    pub(crate) transport: Option<benilla_protocol::TransportPose>,
}

/// `SMSG_CLIENT_CONTROL_UPDATE`, verbatim; the controller decides. `mover` is the unit spoken
/// about, not the new mover: the server revokes by naming us (`allow_move` false) and grants by
/// naming another unit.
#[derive(Message, Clone, Copy)]
pub(crate) struct ClientControlMessage {
    pub(crate) mover: u64,
    pub(crate) allow_move: bool,
}

/// `SMSG_FORCE_*_SPEED_CHANGE` for our mover; the controller answers
/// [`ClientCommand::ForceSpeedAck`] with its live pose.
#[derive(Message, Clone, Copy)]
pub(crate) struct SpeedChangeMessage {
    pub(crate) guid: u64,
    pub(crate) kind: SpeedKind,
    pub(crate) counter: u32,
    pub(crate) speed: f32,
}

/// Root, water-walk, feather-fall or hover granted or revoked on our mover: the server's
/// `IsFlagAckOpcode` set. The controller acks with its live pose.
#[derive(Message, Clone, Copy)]
pub(crate) struct MoveModeMessage {
    /// Our mover's guid, echoed in the ack.
    pub(crate) guid: u64,
    pub(crate) counter: u32,
    pub(crate) mode: MoveMode,
    pub(crate) apply: bool,
}

/// `SMSG_MOVE_KNOCK_BACK` at our mover; the controller launches and acks.
#[derive(Message, Clone, Copy)]
pub(crate) struct KnockBackMessage {
    /// Our mover's guid, echoed in the ack as a full u64.
    pub(crate) guid: u64,
    pub(crate) counter: u32,
    /// The launch, in the jump tail's convention (`zspeed` down-positive).
    pub(crate) launch: JumpInfo,
}

/// A cross-map worldport: snap, swap the map, ack if required.
#[derive(Message, Clone, Copy)]
pub(crate) struct WorldportMessage {
    pub(crate) map_id: u32,
    /// Boat-local when `transport_entry` is set (vmangos `SendNewWorld` sends
    /// `GetTransportPos()`); world otherwise.
    pub(crate) position: [f32; 3],
    pub(crate) orientation: f32,
    pub(crate) needs_ack: bool,
    pub(crate) native_token: Option<benilla_protocol::wrath_transfer::Token>,
    /// The transport carrying us, from `SMSG_TRANSFER_PENDING` ([`PendingTransfer`]). `None`
    /// means the server detached us and any ride is stale.
    pub(crate) transport_entry: Option<u32>,
}

/// The `SMSG_TRANSFER_PENDING` latch: whether the coming worldport rides a transport. Cleared
/// on abort and disconnect.
#[derive(Resource, Default)]
pub(crate) struct PendingTransfer(pub(crate) Option<PendingTransferInfo>);

#[derive(Clone, Copy)]
pub(crate) struct PendingTransferInfo {
    pub(crate) map_id: u32,
    pub(crate) transport_entry: Option<u32>,
}

/// `SMSG_PLAY_SOUND`, `PLAY_MUSIC` or `PLAY_OBJECT_SOUND`: a SoundEntries id. An unstreamed
/// `source` plays 2D.
#[derive(Message, Clone, Copy)]
pub(crate) struct ServerSoundMessage {
    pub(crate) kind: ServerSoundKind,
    pub(crate) sound_id: u32,
    pub(crate) source: Option<Entity>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerSoundKind {
    Sound2d,
    Music,
    ObjectSound,
}

/// `SMSG_TEXT_EMOTE` or `SMSG_EMOTE`; an unstreamed performer is dropped (voice needs its race
/// and sex).
#[derive(Message, Clone, Copy)]
pub(crate) struct EmoteMessage {
    /// The performer's wire guid, known even while its entity has not streamed.
    pub(crate) guid: u64,
    pub(crate) source: Option<Entity>,
    pub(crate) kind: EmoteKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmoteKind {
    /// An `EmotesText.dbc` id.
    Text(u32),
    /// An `Emotes.dbc` id (its `EventSoundID`).
    Anim(u32),
}

/// `SMSG_AI_REACTION`: `hostile` is reaction 2 (each melee start), else 0 (the stealth alert).
/// Audio only in the client (`0x6056e0`).
#[derive(Message, Clone, Copy)]
pub(crate) struct AiReactionMessage {
    pub(crate) unit: Entity,
    pub(crate) hostile: bool,
}

/// `SMSG_PET_ACTION_SOUND`: `talk` is `PET_TALK_ORDER` or `PET_TALK_ATTACK`, resolved through
/// the pet's `CreatureSoundData`. Audio only (`0x6040c0`).
#[derive(Message, Clone, Copy)]
pub(crate) struct PetTalkMessage {
    pub(crate) unit: Entity,
    pub(crate) talk: u32,
}

/// `SMSG_PET_DISMISS_SOUND`: a `CreatureModelData` id and a point in Bevy space; no guid, the
/// pet is already gone.
#[derive(Message, Clone, Copy)]
pub(crate) struct PetDismissSoundMessage {
    pub(crate) model_id: u32,
    pub(crate) pos: Vec3,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrath_capabilities_admit_only_verified_melee_posture_states() {
        let (tx, _rx) = crossbeam_channel::unbounded();
        let net = NetCommands(tx);
        for command in [
            ClientCommand::SetSelection { guid: 0 },
            ClientCommand::AttackSwing { guid: 7 },
            ClientCommand::AttackStop,
            ClientCommand::SetSheathed { state: 2 },
            ClientCommand::StandStateChange { state: 8 },
            ClientCommand::Loot { guid: 7 },
            ClientCommand::AutostoreLootItem { slot: 17 },
            ClientCommand::LootMoney,
            ClientCommand::LootRelease { guid: 7 },
            ClientCommand::CancelCast { spell_id: 78 },
            ClientCommand::CastSpell {
                spell_id: 78,
                target: Some(7),
            },
            ClientCommand::CreatureQuery { entry: 69, guid: 7 },
            ClientCommand::OpenItem {
                bag_index: 255,
                slot: 0,
            },
            ClientCommand::CancelAutoRepeat,
            // `CMSG_QUERY_TIME` has a native route (session misc).
            ClientCommand::QueryTime,
            // `CMSG_MOVE_TIME_SKIPPED` has a native route (movement M3).
            ClientCommand::MoveTimeSkipped {
                guid: 1057,
                lag_ms: 2_210,
            },
        ] {
            assert!(net.supports(SessionDialect::Wrath12340, &command));
        }
        for command in [
            ClientCommand::SetSheathed { state: 3 },
            ClientCommand::StandStateChange { state: 2 },
            ClientCommand::AutostoreLootItem { slot: 18 },
        ] {
            assert!(!net.supports(SessionDialect::Wrath12340, &command));
            assert!(net.supports(SessionDialect::Vanilla5875, &command));
        }
    }

    fn connected() -> SessionEvent {
        SessionEvent::Connected {
            self_guid: 0x1234,
            name: "New".into(),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: None,
        }
    }

    fn old_pose_move(pos: Vec3) -> ClientCommand {
        ClientCommand::Move {
            kind: MoveKind::Heartbeat,
            flags: 0,
            pos: [pos.x, pos.y, pos.z],
            orientation: 0.0,
            pitch: 0.0,
            fall_time: 0,
            jump: None,
            transport: None,
        }
    }

    #[test]
    fn wrath_connected_clears_old_pose_before_publishing_the_new_endpoint() {
        let mut app = crate::game_plugins::schedule_tests::headless_client();
        let (wire_tx, wire_rx) = crossbeam_channel::unbounded();
        let old = SessionCommandSender::new(wire_tx.clone(), 7);
        let old_clone = old.clone();
        let new = SessionCommandSender::new(wire_tx, 8);
        let (endpoint_tx, endpoint_rx) = crossbeam_channel::unbounded();
        endpoint_tx.send(new).unwrap();
        app.insert_resource(NetCommands::session_stamped(old))
            .insert_resource(CommandEndpoints(endpoint_rx))
            .init_resource::<crate::player::Player>();
        {
            let mut player = app.world_mut().resource_mut::<crate::player::Player>();
            player.active = true;
            player.pos = Vec3::new(11.0, 22.0, 33.0);
        }

        // The prior session edge and the new connection are drained in one ECS frame.
        handlers::dispatch(app.world_mut(), vec![SessionEvent::LoggedOut, connected()]);

        let player = app.world().resource::<crate::player::Player>();
        assert!(*player == crate::player::Player::default());
        // Even a producer retaining the old clone remains stamped old. The normal movement gate
        // sees inactive and therefore cannot copy the old pose through the newly active endpoint.
        old_clone
            .send(old_pose_move(Vec3::new(11.0, 22.0, 33.0)))
            .unwrap();
        if player.active {
            app.world()
                .resource::<NetCommands>()
                .0
                .send(old_pose_move(player.pos))
                .unwrap();
        }
        app.world()
            .resource::<NetCommands>()
            .0
            .send(ClientCommand::Logout)
            .unwrap();
        let queued: Vec<_> = wire_rx.try_iter().collect();
        assert_eq!(queued.len(), 2);
        assert_eq!(queued[0].epoch, 7);
        assert!(matches!(queued[0].command, ClientCommand::Move { .. }));
        assert_eq!(queued[1].epoch, 8);
        assert!(matches!(queued[1].command, ClientCommand::Logout));
    }

    #[test]
    fn raw_offline_connected_neither_waits_for_a_sidechannel_nor_resets_vanilla_player() {
        let mut app = crate::game_plugins::schedule_tests::headless_client();
        let (raw_tx, _raw_rx) = crossbeam_channel::unbounded();
        let (_endpoint_tx, endpoint_rx) = crossbeam_channel::unbounded();
        app.insert_resource(NetCommands(raw_tx))
            .insert_resource(CommandEndpoints(endpoint_rx))
            .init_resource::<crate::player::Player>();
        app.world_mut()
            .resource_mut::<crate::player::Player>()
            .active = true;

        handlers::dispatch(app.world_mut(), vec![connected()]);

        assert!(app.world().resource::<crate::player::Player>().active);
        assert!(matches!(
            app.world().resource::<NetCommands>().0,
            CommandSender::Raw(_)
        ));
    }

    #[test]
    fn session_backend_selection_is_explicit_and_vanilla_default_stays_vanilla() {
        assert_eq!(
            SessionDialect::from_profile(&benilla_formats::DataProfile::default()),
            SessionDialect::Vanilla5875
        );
        assert_eq!(
            SessionDialect::from_profile(&benilla_formats::DataProfile::Wrath {
                locale: "enUS".into()
            }),
            SessionDialect::Wrath12340
        );
    }

    /// A re-create of a live object notifies like a delta.
    #[test]
    fn the_merge_emits_one_edge_per_moved_dword_and_none_for_a_first_create() {
        const FIELD_UNIT_HEALTH: u16 = benilla_protocol::messages::FieldLayout::Vanilla5875
            .index_of(benilla_protocol::messages::Field::UnitHealth);
        const FIELD_UNIT_LEVEL: u16 = benilla_protocol::messages::FieldLayout::Vanilla5875
            .index_of(benilla_protocol::messages::Field::UnitLevel);
        use benilla_protocol::messages::ObjectType;

        let mut world = World::new();
        world.init_resource::<Messages<FieldChanged>>();
        let unit = world.spawn(Guid(0xF130_0000_0000_0042)).id();
        let drain = |world: &mut World| -> Vec<FieldChanged> {
            world
                .resource_mut::<Messages<FieldChanged>>()
                .drain()
                .collect()
        };

        apply_fields_for_test(
            &mut world,
            unit,
            ObjectFields::from_pairs(&[(FIELD_UNIT_HEALTH, 100), (FIELD_UNIT_LEVEL, 9)])
                .into_created(ObjectType::Unit),
        );
        assert!(drain(&mut world).is_empty(), "a create is not an edge");

        // The level re-sent unchanged, the health moved.
        apply_fields_for_test(
            &mut world,
            unit,
            ObjectFields::from_pairs(&[(FIELD_UNIT_LEVEL, 9), (FIELD_UNIT_HEALTH, 0)]),
        );
        assert_eq!(
            drain(&mut world),
            vec![FieldChanged {
                entity: unit,
                guid: 0xF130_0000_0000_0042,
                kind: ObjectType::Unit,
                field: benilla_protocol::messages::Field::UnitHealth,
                offset: 0,
                layout: benilla_protocol::messages::FieldLayout::Vanilla5875,
                old: 100,
                new: 0,
            }],
            "one edge for the dword that moved, carrying the old value; the resend is silent"
        );

        // A re-create of the live guid: health was already 0, so the level is the only edge.
        apply_fields_for_test(
            &mut world,
            unit,
            ObjectFields::from_pairs(&[(FIELD_UNIT_LEVEL, 10)]).into_created(ObjectType::Unit),
        );
        let edges = drain(&mut world);
        assert_eq!(edges.len(), 1);
        assert!(
            edges[0].named_field(benilla_protocol::messages::Field::UnitLevel)
                && edges[0].old == 9
                && edges[0].new == 10
        );
        assert_eq!(
            world.get::<ObjectStore>(unit).unwrap().0.unit_level(),
            Some(10)
        );
    }

    /// All existing watcher roots retain their class and member after native reverse lookup.
    #[test]
    fn semantic_edges_preserve_existing_scalar_and_array_watchers() {
        use benilla_protocol::messages::{Field, FieldLayout, ObjectType};
        let unit_fields = [
            Field::UnitHealth,
            Field::UnitMaxHealth,
            Field::UnitLevel,
            Field::UnitDynamicFlags,
            Field::UnitMountDisplayId,
            Field::UnitBytes1,
            Field::UnitFlags,
        ];
        let player_fields = [
            Field::PlayerFlags,
            Field::PlayerCoinage,
            Field::PlayerSkillInfo11,
        ];
        let entity = Entity::PLACEHOLDER;
        for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
            for (kind, fields) in [
                (ObjectType::Unit, unit_fields.as_slice()),
                (ObjectType::Player, player_fields.as_slice()),
                (ObjectType::GameObject, [Field::GameobjectState].as_slice()),
                (ObjectType::Corpse, [Field::CorpseDynamicFlags].as_slice()),
            ] {
                for &field in fields {
                    let Some(span) = layout.resolve(field) else {
                        continue;
                    };
                    let mut store = ObjectFields::from_native_pairs(layout, &[(span.index, 1)])
                        .into_created(kind);
                    let delta = ObjectFields::from_native_pairs(layout, &[(span.index, 2)]);
                    let mut edges = Vec::new();
                    merge_store_fields(&mut store, delta, entity, 42, |edge| edges.push(edge));
                    assert_eq!(edges.len(), 1, "{layout:?}/{field:?}");
                    let edge = edges[0];
                    assert!(edge.named_field(field));
                    assert_eq!((edge.field, edge.offset), (field, 0));
                    assert_eq!((edge.old, edge.new), (1, 2));
                    assert!(FieldEdges::semantic_of(&[(42, field, 0)]).moved_field(42, field, 0));
                }
            }
            for field in [
                Field::UnitAura,
                Field::UnitAuraFlags,
                Field::UnitAuraApplications,
            ] {
                let Some(span) = layout.resolve(field) else {
                    continue;
                };
                for offset in [0, span.width - 1] {
                    let (resolved, member) = layout
                        .field_at(ObjectType::Player, span.index + offset)
                        .unwrap();
                    let edge = FieldChanged {
                        entity,
                        guid: 42,
                        kind: ObjectType::Player,
                        field: resolved,
                        offset: member,
                        layout,
                        old: 1,
                        new: 2,
                    };
                    assert_eq!(edge.unit_slot(field, span.width), Some(offset));
                    assert_eq!(edge.unit_slot(field, offset), None);
                    assert!(!edge.named_field(Field::UnitHealth));
                }
            }
        }
    }

    #[test]
    fn semantic_edge_keys_disambiguate_overlapping_classes_and_array_members() {
        use benilla_protocol::messages::{Field, FieldLayout, ObjectType};
        // Vanilla corpse flags overlap a unit word: semantic keys must not alias them.
        let layout = FieldLayout::Vanilla5875;
        let index = layout.resolve(Field::CorpseDynamicFlags).unwrap().index;
        let (unit_field, _) = layout.field_at(ObjectType::Unit, index).unwrap();
        assert_ne!(unit_field, Field::CorpseDynamicFlags);
        let edges = FieldEdges::semantic_of(&[
            (42, Field::CorpseDynamicFlags, 0),
            (42, Field::UnitAura, 47),
        ]);
        assert!(!edges.moved_field(42, unit_field, 0));
        assert!(!edges.moved_field(43, Field::CorpseDynamicFlags, 0));
        assert!(edges.moved_field(42, Field::UnitAura, 47));
        assert!(!edges.moved_field(42, Field::UnitAura, 46));
        assert!(!edges.moved_field(42, Field::UnitAuraFlags, 0));
    }

    /// Login env vars alone do not make a run unattended: a person's launch line sets them too.
    #[test]
    fn a_kick_ends_the_session_unless_the_run_declared_itself_driverless() {
        use crate::local_state::test_env::{EnvGuard, ENV_LOCK};
        let _lock = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let lost = || {
            DisconnectedMessage::new("kicked".into(), benilla_protocol::SessionEnd::Lost)
                .session_over
        };

        // Credentials in the environment and nothing declared: a person, so the session is over.
        let _user = EnvGuard::set("WOW_USER", "player");
        let _pass = EnvGuard::set("WOW_PASS", "secret");
        let _char = EnvGuard::set("WOW_CHAR", "Hero");
        let _decl = EnvGuard::unset("WOW_UNATTENDED");
        let _cap = EnvGuard::unset("WOW_CAPTURE");
        let _rig = EnvGuard::unset("WOW_RIG");
        assert!(lost(), "the reported regression: this must be `true`");

        assert!(
            !DisconnectedMessage::new("bye".into(), benilla_protocol::SessionEnd::LoggedOut)
                .session_over
        );

        // A declared unattended run, a rig or a capture reconnects.
        {
            let _d = EnvGuard::set("WOW_UNATTENDED", "1");
            assert!(!lost(), "a declared probe still reconnects");
        }
        {
            let _r = EnvGuard::set("WOW_RIG", "at:229,0,0,0");
            assert!(!lost(), "a rig drives the body; it cannot be a person");
        }
        {
            let _c = EnvGuard::set("WOW_CAPTURE", "some-scenario");
            assert!(
                !lost(),
                "a capture authors the camera; it cannot be a person"
            );
        }
        assert!(lost(), "and the guards put every one of those back");
    }

    /// The player speeds, vmangos `baseMoveSpeed`.
    fn vanilla() -> MoveSpeeds {
        MoveSpeeds {
            walk: 2.5,
            run: 7.0,
            run_back: 4.5,
            swim: 4.722,
            swim_back: 2.5,
            turn_rate: std::f32::consts::PI,
            flight: 7.0,
            flight_back: 4.5,
            pitch_rate: std::f32::consts::PI,
        }
    }

    /// The walk arm (`0x7c4d11`) precedes the backward min (`0x7c4d1d`).
    #[test]
    fn the_walk_arm_outranks_the_backward_min() {
        use crate::creature_anim::move_flags as f;
        let s = vanilla();
        assert_eq!(current_speed(&s, f::FORWARD), 7.0, "plain run");
        assert_eq!(current_speed(&s, f::BACKWARD), 4.5, "the backward min");
        assert_eq!(current_speed(&s, f::FORWARD | f::WALK_MODE), 2.5, "walk");
        assert_eq!(
            current_speed(&s, f::BACKWARD | f::WALK_MODE),
            2.5,
            "walking backwards is a WALK, not a run-back: the walk arm is taken first"
        );
        // A strafing walker likewise.
        assert_eq!(current_speed(&s, f::STRAFE_LEFT | f::WALK_MODE), 2.5);
    }

    /// `0x7c4cd9` precedes `0x7c4d11`.
    #[test]
    fn swimming_is_tested_first_so_there_is_no_swim_walk() {
        use crate::creature_anim::move_flags as f;
        let s = vanilla();
        assert_eq!(
            current_speed(&s, f::FORWARD | f::SWIMMING | f::WALK_MODE),
            s.swim
        );
        assert_eq!(
            current_speed(&s, f::BACKWARD | f::SWIMMING | f::WALK_MODE),
            2.5,
            "the swim arm's own backward min — reached, and unaffected by the walk bit"
        );
    }

    /// x87 `fcompp`, `test ah,0x41`, `jp`: visible only when a server sets a slow field above a
    /// fast one.
    #[test]
    fn every_arm_that_looks_like_a_select_is_really_a_min() {
        use crate::creature_anim::move_flags as f;
        let s = MoveSpeeds {
            walk: 12.0, // a walk-speed buff above run
            run: 7.0,
            run_back: 9.0, // a run-back above run
            swim: 4.0,
            swim_back: 9.0, // a swim-back above swim
            turn_rate: std::f32::consts::PI,
            flight: 7.0,
            flight_back: 4.5,
            pitch_rate: std::f32::consts::PI,
        };
        assert_eq!(current_speed(&s, f::FORWARD | f::WALK_MODE), 7.0);
        assert_eq!(current_speed(&s, f::BACKWARD), 7.0);
        assert_eq!(current_speed(&s, f::FORWARD | f::SWIMMING), 4.0);
        assert_eq!(current_speed(&s, f::BACKWARD | f::SWIMMING), 4.0);
    }

    /// No direction bit is 0 (`0x7c4c99`). No zero-`walk` fallback: our own create applies the
    /// speed block like any other (`0x5fad50`).
    #[test]
    fn a_mode_is_not_a_motion_and_a_zero_walk_speed_is_a_zero() {
        use crate::creature_anim::move_flags as f;
        let s = vanilla();
        assert_eq!(current_speed(&s, 0), 0.0);
        assert_eq!(
            current_speed(&s, f::WALK_MODE),
            0.0,
            "a mode is not a motion"
        );
        assert_eq!(current_speed(&s, f::TURN_LEFT | f::ROOT | f::SWIMMING), 0.0);

        let unfilled = MoveSpeeds {
            run: 7.0,
            ..Default::default()
        };
        assert_eq!(
            current_speed(&unfilled, f::FORWARD | f::WALK_MODE),
            0.0,
            "no fallback: `min(walk, run)` with walk 0 is 0"
        );
        assert_eq!(
            current_speed(&unfilled, f::FORWARD),
            7.0,
            "…and the run arm is untouched by it"
        );
    }

    /// The whitelist at `0x49fa3f`-`0x49fa4e`. vmangos would accept `LANG_ADDON` on the refused
    /// lanes too; the client never sends on them.
    #[test]
    fn addon_distributions_map_to_the_clients_own_four_wire_types() {
        use benilla_protocol::messages as m;
        use benilla_ui::script::AddonDistribution as D;

        assert_eq!(addon_wire_chat_type(D::Party), 0x01, "CHAT_MSG_PARTY");
        assert_eq!(addon_wire_chat_type(D::Raid), 0x02, "CHAT_MSG_RAID");
        assert_eq!(addon_wire_chat_type(D::Guild), 0x03, "CHAT_MSG_GUILD");
        assert_eq!(
            addon_wire_chat_type(D::Battleground),
            0x5C,
            "CHAT_MSG_BATTLEGROUND"
        );

        // No distribution reaches a lane the reference refuses.
        let image: Vec<u32> = [D::Party, D::Raid, D::Guild, D::Battleground]
            .into_iter()
            .map(addon_wire_chat_type)
            .collect();
        for refused in [
            m::CHAT_TYPE_SAY,
            m::CHAT_TYPE_YELL,
            m::CHAT_TYPE_WHISPER,
            m::CHAT_TYPE_EMOTE,
            m::CHAT_TYPE_OFFICER,
            m::CHAT_TYPE_CHANNEL,
            m::CHAT_TYPE_RAID_LEADER,
            m::CHAT_TYPE_RAID_WARNING,
            m::CHAT_TYPE_BATTLEGROUND_LEADER,
        ] {
            assert!(
                !image.contains(&refused),
                "{refused:#04x} is a lane the client never sends addon data on"
            );
        }
    }

    #[test]
    fn objects_lists_every_streamed_object_and_names_our_own() {
        use bevy::ecs::system::RunSystemOnce;
        const ME: u64 = 0x5E1F;
        const BOAR: u64 = 0xF130_0000_0000_0042;
        let mut world = World::new();
        world.init_resource::<GuidIndex>();
        let fields = |health| {
            ObjectStore(ObjectFields::from_pairs(&[(
                benilla_protocol::field::FIELD_UNIT_HEALTH,
                health,
            )]))
        };
        let me = world.spawn((SelfPlayer, Guid(ME), fields(256))).id();
        let boar = world.spawn((Guid(BOAR), fields(40))).id();
        world
            .resource_mut::<GuidIndex>()
            .0
            .extend([(ME, me), (BOAR, boar)]);
        let (listed, player) = world
            .run_system_once(|objects: Objects| {
                let mut listed: Vec<_> = objects
                    .iter()
                    .map(|(guid, e, f)| (guid, e, f.unit_health()))
                    .collect();
                listed.sort_by_key(|&(guid, ..)| guid);
                (listed, objects.player())
            })
            .unwrap();
        assert_eq!(listed, [(ME, me, Some(256)), (BOAR, boar, Some(40))]);
        assert_eq!(player, Some(me));
    }
}
