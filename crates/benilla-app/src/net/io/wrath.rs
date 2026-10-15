//! Wrath 3.3.5a app-session backend.
//!
//! This backend keeps the build-12340 cipher writer on one thread. Adapter replies and app
//! commands share that thread, so a time-sync response can never race a ping or lifecycle packet.

mod recorder;

use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use benilla_protocol::wrath_authority::{
    self, ForceRootOrder, GroundPose, OnFootPose, UnsetCanFlyOrder,
};
use benilla_protocol::wrath_combat;
use benilla_protocol::wrath_events::{
    WrathDispatch, WrathEvent, WrathEventAdapter, WrathEventError, WrathReply,
};
use benilla_protocol::wrath_gameplay::{MovementPosition, ObjectUpdate, UPDATE_FLAG_SELF};
use benilla_protocol::wrath_ground::{self, SessionClock};
use benilla_protocol::wrath_loot;
use benilla_protocol::wrath_world::{
    WrathCharacter, WrathPartialFrameRead, WrathWardenRequired, WrathWorldAuthReject,
    WrathWorldSession, WrathWorldWriter,
};
use benilla_protocol::{CharEnumItem, Character, LoginStage, SessionEnd, SessionEvent};
use crossbeam_channel::{Receiver, Sender};

use super::{note_inbound, Parks, PingClock, PING_INTERVAL, SEND_WARN_CAP};
use crate::net::{
    CharRequest, ClientCommand, RealmRequest, SessionCommand, SessionCommandSender, SessionDialect,
};

// AzerothCore 3.3.5a Opcodes.h, also used by the validated `wrath_objects` probe.
const CMSG_LOGOUT_REQUEST: u32 = 0x04b;
const CMSG_ITEM_QUERY_SINGLE: u32 = 0x056;
const SMSG_CHARACTER_LOGIN_FAILED: u16 = 0x041;
const SMSG_LOGOUT_RESPONSE: u16 = 0x04c;
const SMSG_LOGOUT_COMPLETE: u16 = 0x04d;
const CMSG_PING: u32 = 0x01dc;
const SMSG_PONG: u16 = 0x01dd;
const CMSG_SET_ACTIVE_MOVER: u32 = 0x026a;
const SMSG_CLIENT_CONTROL_UPDATE: u16 = 0x0159;
const MAX_PENDING_PRE_SELF_ORDERS: usize = 8;
const PRE_SELF_ORDER_TIMEOUT: Duration = Duration::from_secs(2);
const READ_SLICE: Duration = Duration::from_millis(250);
const FRAME_TIMEOUT: Duration = Duration::from_secs(10);
const REALM_REFRESH_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn spawn_threads(
    events_tx: Sender<SessionEvent>,
    cmd_rx: Receiver<SessionCommand>,
    parks: Parks,
    abandon: Arc<AtomicU64>,
    ping_clock: Arc<Mutex<PingClock>>,
    command_tx: Sender<SessionCommand>,
    command_endpoints: Sender<SessionCommandSender>,
) {
    let (control_tx, control_rx) = crossbeam_channel::unbounded::<WrathWriterControl>();
    let (fatal_tx, fatal_rx) = crossbeam_channel::unbounded::<EpochFatal>();
    let authority = Arc::new(MovementAuthority::default());
    let writer_clock = Arc::clone(&ping_clock);
    let writer_control = control_tx.clone();
    thread::Builder::new()
        .name("wow-net-write-wrath".into())
        .spawn(move || {
            benilla_world::thread_qos::promote_current_thread(
                benilla_world::thread_qos::QosClass::UserInitiated,
            );
            writer_loop(
                &cmd_rx,
                control_rx,
                &writer_control,
                &writer_clock,
                &fatal_tx,
            );
        })
        .expect("spawn Wrath network writer");

    thread::Builder::new()
        .name("wow-net-wrath".into())
        .spawn(move || {
            benilla_world::thread_qos::promote_current_thread(
                benilla_world::thread_qos::QosClass::UserInitiated,
            );
            let mut next_epoch = 0;
            loop {
                ping_clock.lock().unwrap_or_else(|p| p.into_inner()).clear();
                match run(
                    &events_tx,
                    &control_tx,
                    &fatal_rx,
                    &parks,
                    &abandon,
                    &ping_clock,
                    &command_tx,
                    &command_endpoints,
                    &mut next_epoch,
                    &authority,
                ) {
                    Ok(WrathCycle::Exit) => return,
                    Ok(WrathCycle::Repark) => {}
                    Ok(WrathCycle::LoggedOut) => {
                        if events_tx
                            .send(SessionEvent::Disconnected {
                                reason: "logged out".into(),
                                end: SessionEnd::LoggedOut,
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(error) => {
                        bevy::log::error!("net(wrath): {error:#}");
                        if events_tx
                            .send(SessionEvent::Disconnected {
                                reason: format!("Wrath session disconnected: {error:#}"),
                                end: SessionEnd::Lost,
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
        })
        .expect("spawn Wrath network reader");
}

#[derive(Clone, Copy)]
enum WrathCycle {
    Exit,
    Repark,
    LoggedOut,
}

fn run(
    events: &Sender<SessionEvent>,
    control: &Sender<WrathWriterControl>,
    fatal: &Receiver<EpochFatal>,
    parks: &Parks,
    abandon: &AtomicU64,
    ping_clock: &Mutex<PingClock>,
    command_tx: &Sender<SessionCommand>,
    command_endpoints: &Sender<SessionCommandSender>,
    next_epoch: &mut u64,
    authority: &Arc<MovementAuthority>,
) -> Result<WrathCycle> {
    let request = match parks.login_rx.recv() {
        Ok(request) => request,
        Err(_) => return Ok(WrathCycle::Exit),
    };
    let generation = request.generation;
    let canceled = || abandon.load(Ordering::SeqCst) != generation;
    if request.dialect != SessionDialect::Wrath12340 {
        send_login_failure(
            events,
            "network backend/data snapshot dialect mismatch: Wrath backend requires wrath-12340",
            true,
        );
        return Ok(WrathCycle::Repark);
    }

    send(
        events,
        SessionEvent::LoginStage {
            stage: LoginStage::Connecting,
        },
    )?;
    send(
        events,
        SessionEvent::LoginStage {
            stage: LoginStage::Authenticating,
        },
    )?;
    let mut logon =
        match benilla_protocol::wrath_auth::logon(&request.host, &request.user, &request.pass) {
            Ok(logon) => logon,
            Err(error) => {
                if canceled() {
                    return Ok(WrathCycle::Repark);
                }
                let refusal = error
                    .downcast_ref::<benilla_protocol::AuthReject>()
                    .map(|reject| benilla_protocol::LoginRefusal::Logon(reject.code));
                send(
                    events,
                    SessionEvent::LoginFailed {
                        refusal,
                        reason: format!("Wrath authentication: {error:#}"),
                        terminal: false,
                        dial: None,
                    },
                )?;
                return Ok(WrathCycle::Repark);
            }
        };
    if canceled() {
        return Ok(WrathCycle::Repark);
    }

    let mut realms = normalize_realms(&logon.realms);
    let mut selected = if realms.is_empty() {
        None
    } else {
        drain(&parks.realm_rx);
        loop {
            send(
                events,
                SessionEvent::RealmList {
                    realms: realms.clone(),
                },
            )?;
            match parks.realm_rx.recv() {
                Err(_) => return Ok(WrathCycle::Exit),
                Ok(_) if canceled() => return Ok(WrathCycle::Repark),
                Ok(RealmRequest::Abandon) => return Ok(WrathCycle::Repark),
                Ok(RealmRequest::Refresh) => {
                    logon.refresh_realms(REALM_REFRESH_TIMEOUT);
                    realms = normalize_realms(&logon.realms);
                }
                Ok(RealmRequest::Enter(name)) => {
                    if let Some(index) = realms.iter().position(|realm| realm.name == name) {
                        break Some(index);
                    }
                }
            }
        }
    };

    'realm: loop {
        let (world_addr, realm_id, realm_record) = match selected {
            Some(index) => {
                let raw = &logon.realms[index];
                (
                    raw.address.clone(),
                    u32::from(raw.id),
                    Some(realms[index].clone()),
                )
            }
            None => {
                let (host, _) = benilla_protocol::host_port(
                    &request.host,
                    benilla_protocol::wrath_world::WRATH_WORLD_PORT,
                );
                (
                    format!("{host}:{}", benilla_protocol::wrath_world::WRATH_WORLD_PORT),
                    0,
                    None,
                )
            }
        };
        send(
            events,
            SessionEvent::LoginStage {
                stage: LoginStage::Handshaking,
            },
        )?;
        let mut session = match WrathWorldSession::connect(
            &world_addr,
            &request.user,
            logon.session_key,
            realm_id,
        ) {
            Ok(session) => session,
            Err(error) => {
                if canceled() {
                    return Ok(WrathCycle::Repark);
                }
                if error.downcast_ref::<WrathWardenRequired>().is_some() {
                    send_login_failure(events, "Wrath server requires unsupported Warden", true);
                } else {
                    let refusal = error
                        .downcast_ref::<WrathWorldAuthReject>()
                        .map(|reject| benilla_protocol::LoginRefusal::World(reject.code));
                    send(
                        events,
                        SessionEvent::LoginFailed {
                            refusal,
                            reason: format!("Wrath world handshake with {world_addr}: {error:#}"),
                            terminal: false,
                            dial: None,
                        },
                    )?;
                }
                return Ok(WrathCycle::Repark);
            }
        };
        if canceled() {
            return Ok(WrathCycle::Repark);
        }
        let characters = normalize_characters(session.char_enum().context("Wrath roster")?);
        if canceled() {
            return Ok(WrathCycle::Repark);
        }
        drain(&parks.pick_rx);
        drain(&parks.realm_rx);
        send(
            events,
            SessionEvent::CharacterList {
                characters: characters.clone(),
                realm: realm_record.clone(),
            },
        )?;

        let guid = loop {
            crossbeam_channel::select! {
                recv(parks.realm_rx) -> choice => match choice {
                    Err(_) => return Ok(WrathCycle::Exit),
                    Ok(RealmRequest::Abandon) => {}
                    Ok(RealmRequest::Refresh) => {
                        logon.refresh_realms(REALM_REFRESH_TIMEOUT);
                        realms = normalize_realms(&logon.realms);
                        send(events, SessionEvent::RealmList { realms: realms.clone() })?;
                    }
                    Ok(RealmRequest::Enter(name)) => {
                        if let Some(index) = realms.iter().position(|realm| realm.name == name) {
                            selected = Some(index);
                            continue 'realm;
                        }
                    }
                },
                recv(parks.pick_rx) -> pick => match pick {
                    Err(_) => return Ok(WrathCycle::Exit),
                    Ok(CharRequest::Enter(guid)) => break guid,
                    Ok(CharRequest::Abandon) => return Ok(WrathCycle::Repark),
                    Ok(CharRequest::Create(_)) | Ok(CharRequest::Delete(_)) => {
                        bail!("Wrath character mutation is not integrated; reconnecting without sending a Vanilla packet")
                    }
                }
            }
        };
        let name = characters
            .iter()
            .find(|character| character.guid == guid)
            .map_or_else(String::new, |character| character.name.clone());

        // Lossless handoff: send login, split immediately, announce the session, then consume every
        // queued packet through the Wrath adapter.
        session.begin_player_login(guid)?;
        let (mut reader, writer) = session.into_split()?;
        let session_clock = Arc::new(SessionClock::new());
        *next_epoch = next_epoch
            .checked_add(1)
            .expect("Wrath command epoch overflow");
        let epoch = *next_epoch;
        let authority = authority.install(epoch, guid);
        let (installed_tx, installed_rx) = crossbeam_channel::bounded(0);
        control
            .send(WrathWriterControl::Install {
                epoch,
                session: Box::new(WrathWriterSwap {
                    writer,
                    clock: Arc::clone(&session_clock),
                    active_mover: guid,
                    authority: authority.clone(),
                }),
                installed: installed_tx,
            })
            .map_err(|_| anyhow!("Wrath writer stopped"))?;
        installed_rx
            .recv()
            .map_err(|_| anyhow!("Wrath writer stopped before install acknowledgement"))?
            .map_err(|reason| anyhow!(reason))?;
        let _retire = WriterRetireGuard {
            epoch,
            control: control.clone(),
            authority: authority.clone(),
        };
        command_endpoints
            .send(SessionCommandSender::new(command_tx.clone(), epoch))
            .map_err(|_| anyhow!("app exited before Wrath command endpoint publication"))?;
        send(
            events,
            SessionEvent::Connected {
                self_guid: guid,
                name,
                billing_time_rested: 0,
                tutorial_flags: None,
                // `SMSG_ADDON_INFO` is not decoded on this route; the realm hides no addon, and
                // without a reply the client's addon array stays empty and nothing can load.
                addon_info: Some(Vec::new()),
            },
        )?;
        reader.set_read_timeout(Some(READ_SLICE))?;
        let mut adapter = WrathEventAdapter::new();
        let mut packet_recorder = recorder::Recorder::from_env();
        loop {
            if let Ok(failure) = fatal.try_recv() {
                if failure.epoch == epoch {
                    bail!("{}", failure.reason);
                }
            }
            let packet = match reader.recv_raw_with_frame_timeout(FRAME_TIMEOUT) {
                Ok(packet) => packet,
                Err(error) if retryable_timeout(&error) => continue,
                Err(error) => return Err(error),
            };
            recorder::record(&mut packet_recorder, &packet);
            note_inbound();
            match packet.opcode {
                SMSG_CHARACTER_LOGIN_FAILED => {
                    let result = one_byte(&packet.body, "SMSG_CHARACTER_LOGIN_FAILED")?;
                    send(events, SessionEvent::CharacterLoginFailed { result })?;
                    return Ok(WrathCycle::LoggedOut);
                }
                SMSG_LOGOUT_RESPONSE => {
                    if packet.body.len() != 5 {
                        bail!(
                            "Wrath SMSG_LOGOUT_RESPONSE has {} bytes, expected 5",
                            packet.body.len()
                        );
                    }
                    send(
                        events,
                        SessionEvent::LogoutResponse {
                            reason: u32::from_le_bytes(packet.body[..4].try_into().unwrap()),
                            instant: packet.body[4] != 0,
                        },
                    )?;
                }
                SMSG_LOGOUT_COMPLETE => {
                    send(events, SessionEvent::LoggedOut)?;
                    return Ok(WrathCycle::LoggedOut);
                }
                SMSG_PONG => {
                    let sequence = le_u32_exact(&packet.body, "SMSG_PONG")?;
                    let _ = ping_clock
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .record_pong(sequence);
                }
                SMSG_CLIENT_CONTROL_UPDATE => {
                    let (mover, allow_move) = client_control_update(&packet.body)?;
                    authority.observe_control(mover, allow_move)?;
                    send(events, SessionEvent::ClientControl { mover, allow_move })?;
                }
                wrath_authority::SMSG_MOVE_UNSET_CAN_FLY => {
                    let order = match wrath_authority::decode_unset_can_fly(&packet.body) {
                        Ok(order) => order,
                        Err(error) => {
                            authority.revoke();
                            return Err(error).context("Wrath SMSG_MOVE_UNSET_CAN_FLY");
                        }
                    };
                    authority.raise_unset_barrier(order)?;
                    control
                        .send(WrathWriterControl::Internal {
                            epoch,
                            command: WrathWriterCommand::UnsetCanFly(order),
                        })
                        .map_err(|_| anyhow!("Wrath writer stopped"))?;
                }
                wrath_authority::SMSG_FORCE_MOVE_ROOT | wrath_authority::SMSG_FORCE_MOVE_UNROOT => {
                    let rooted = packet.opcode == wrath_authority::SMSG_FORCE_MOVE_ROOT;
                    let order = match wrath_authority::decode_force_root(&packet.body) {
                        Ok(order) => order,
                        Err(error) => {
                            authority.revoke();
                            return Err(error).context("Wrath force root order");
                        }
                    };
                    // The reader raises the barrier before enqueue, so no later app movement can
                    // overtake receipt of this server authority order.
                    authority.raise_root_barrier(order, rooted)?;
                    control
                        .send(WrathWriterControl::Internal {
                            epoch,
                            command: WrathWriterCommand::ForceRoot { order, rooted },
                        })
                        .map_err(|_| anyhow!("Wrath writer stopped"))?;
                }
                _ => {
                    reject_unsupported_movement(&authority, packet.opcode)?;
                    dispatch_packet(
                        events,
                        control,
                        epoch,
                        &mut adapter,
                        &packet,
                        &session_clock,
                        &authority,
                    )?;
                }
            }
        }
    }
}

fn dispatch_packet(
    events: &Sender<SessionEvent>,
    control: &Sender<WrathWriterControl>,
    epoch: u64,
    adapter: &mut WrathEventAdapter,
    packet: &benilla_protocol::wrath_world::WrathServerPacket,
    clock: &SessionClock,
    authority: &MovementAuthorityHandle,
) -> Result<()> {
    let now = clock.now_ms();
    if benilla_protocol::wrath_relay::handles(packet.opcode) {
        let event = benilla_protocol::wrath_relay::decode(packet.opcode, &packet.body)
            .context("Wrath on-foot movement relay")?;
        if let SessionEvent::UnitMove { guid, .. } = &event {
            let raw = benilla_protocol::wrath_gameplay::OutboundMovement::from_bytes(&packet.body)?;
            authority.observe_server_pose(*guid, &raw.movement)?;
        }
        return send(events, event);
    }
    match adapter.dispatch_packet(packet, now) {
        Ok(WrathDispatch::Handled {
            session_events,
            wrath_events,
            replies,
        }) => {
            for event in &wrath_events {
                if let WrathEvent::Object(object) = event {
                    if let ObjectUpdate::Create { guid, movement, .. } = object.as_ref() {
                        if movement.update_flags & UPDATE_FLAG_SELF != 0 {
                            authority.observe_self_create(*guid, &movement.position)?;
                            control
                                .send(WrathWriterControl::Internal {
                                    epoch,
                                    command: WrathWriterCommand::PoseReady,
                                })
                                .map_err(|_| anyhow!("Wrath writer stopped"))?;
                        }
                    }
                }
            }
            // The adapter's vectors preserve packet/block order. Replies use the sole cipher writer.
            for event in session_events {
                authority.observe_taxi_spline(&event);
                if *crate::net::CAST_TRACE {
                    bevy::log::info!("net(wrath) RECV {event:?}");
                }
                send(events, event)?;
            }
            for reply in replies {
                control
                    .send(WrathWriterControl::Internal {
                        epoch,
                        command: WrathWriterCommand::Reply(reply),
                    })
                    .map_err(|_| anyhow!("Wrath writer stopped"))?;
            }
        }
        Ok(WrathDispatch::Unhandled { opcode }) => {
            if *crate::net::CAST_TRACE {
                bevy::log::info!("net(wrath) RECV unhandled opcode 0x{opcode:04x}");
            }
            send(
                events,
                SessionEvent::PacketDropped {
                    opcode,
                    unparseable: false,
                },
            )?;
        }
        Err(WrathEventError::WardenRequired) => bail!("Wrath server requires unsupported Warden"),
        Err(error) => bail!("{error}"),
    }
    Ok(())
}

#[derive(Debug)]
enum WrathWriterCommand {
    Reply(WrathReply),
    UnsetCanFly(UnsetCanFlyOrder),
    ForceRoot { order: ForceRootOrder, rooted: bool },
    PoseReady,
}

enum WrathWriterControl {
    Install {
        epoch: u64,
        session: Box<WrathWriterSwap>,
        installed: Sender<Result<(), String>>,
    },
    Internal {
        epoch: u64,
        command: WrathWriterCommand,
    },
    Retire {
        epoch: u64,
        retired: Sender<()>,
    },
    Fatal {
        epoch: u64,
        reason: String,
    },
}

struct WriterRetireGuard {
    epoch: u64,
    control: Sender<WrathWriterControl>,
    authority: MovementAuthorityHandle,
}

impl Drop for WriterRetireGuard {
    fn drop(&mut self) {
        // Reader exit is the authority boundary. Revoke under the movement-send mutex before
        // retirement is queued, so a delayed writer cannot send after the session has ended.
        self.authority.revoke();
        let (retired_tx, retired_rx) = crossbeam_channel::bounded(0);
        if self
            .control
            .send(WrathWriterControl::Retire {
                epoch: self.epoch,
                retired: retired_tx,
            })
            .is_ok()
        {
            // No next install can be issued until the sole writer has retired this epoch.
            let _ = retired_rx.recv();
        }
    }
}

struct EpochFatal {
    epoch: u64,
    reason: String,
}

struct WrathWriterSwap {
    writer: WrathWorldWriter,
    clock: Arc<SessionClock>,
    active_mover: u64,
    authority: MovementAuthorityHandle,
}

#[derive(Default)]
struct MovementAuthority {
    state: Mutex<MovementAuthorityState>,
}

#[derive(Default)]
struct MovementAuthorityState {
    epoch: u64,
    self_guid: u64,
    self_created: bool,
    self_control: bool,
    control_decided: bool,
    ground_pose: Option<OnFootPose>,
    pending_orders: std::collections::VecDeque<PendingAuthorityOrder>,
    last_order_counter: Option<u32>,
    rooted: bool,
    taxi_spline: Option<(u32, bool)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingAuthorityOrder {
    UnsetCanFly(UnsetCanFlyOrder),
    ForceRoot { order: ForceRootOrder, rooted: bool },
}

#[derive(Clone)]
struct MovementAuthorityHandle {
    shared: Arc<MovementAuthority>,
    epoch: u64,
    self_guid: u64,
}

impl MovementAuthority {
    fn install(self: &Arc<Self>, epoch: u64, self_guid: u64) -> MovementAuthorityHandle {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        *state = MovementAuthorityState {
            epoch,
            self_guid,
            self_created: false,
            self_control: false,
            control_decided: false,
            ground_pose: None,
            pending_orders: std::collections::VecDeque::new(),
            last_order_counter: None,
            rooted: false,
            taxi_spline: None,
        };
        MovementAuthorityHandle {
            shared: Arc::clone(self),
            epoch,
            self_guid,
        }
    }
}

impl MovementAuthorityHandle {
    fn observe_self_create(&self, guid: u64, movement: &MovementPosition) -> Result<()> {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch || state.self_guid != self.self_guid {
            return Ok(());
        }
        if guid != self.self_guid {
            state.self_control = false;
            state.control_decided = true;
            bail!("Wrath self create named foreign mover {guid:#018x}");
        }
        state.ground_pose = Some(
            match movement {
                MovementPosition::Living(movement) => OnFootPose::from_movement(movement),
                MovementPosition::Stationary(position) => {
                    GroundPose::stationary(position).map(Into::into)
                }
                _ => Err(wrath_authority::AuthorityCodecError::UnsupportedGroundState),
            }
            .context("Wrath self create on-foot pose")?,
        );
        state.self_created = true;
        // AzerothCore calls SetMover(this) before UpdateVisibilityForPlayer emits this player's
        // CREATE_OBJECT with UPDATEFLAG_SELF (Player.cpp:11816; Object.cpp:197). It does not send
        // an initial SMSG_CLIENT_CONTROL_UPDATE, so that self flag is the server's normal-login
        // confirmation of both self creation and initial self control unless an explicit control
        // decision for this epoch arrived first.
        if !state.control_decided {
            state.self_control = true;
        }
        Ok(())
    }

    fn observe_server_pose(
        &self,
        guid: u64,
        movement: &benilla_protocol::wrath_gameplay::MovementInfo,
    ) -> Result<()> {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch
            || state.self_guid != self.self_guid
            || guid != self.self_guid
            || !state.self_created
        {
            return Ok(());
        }
        let mut movement = movement.clone();
        // ROOT is an authority bit, not locomotion. Its command/FIFO owns permission; an echo
        // updates current XYZO only and must never grant control or clear pending orders.
        movement.flags &= !wrath_authority::MOVEMENT_FLAG_ROOT;
        state.ground_pose = Some(OnFootPose::from_movement(&movement)?);
        Ok(())
    }

    fn observe_control(&self, mover: u64, allow_move: bool) -> Result<()> {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch || state.self_guid != self.self_guid {
            return Ok(());
        }
        if mover != self.self_guid {
            state.self_control = false;
            state.control_decided = true;
            bail!("unsupported Wrath control transfer to non-player mover {mover:#018x}");
        }
        state.self_control = allow_move;
        state.control_decided = true;
        Ok(())
    }

    fn revoke(&self) {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch == self.epoch && state.self_guid == self.self_guid {
            state.self_control = false;
            state.control_decided = true;
            state.taxi_spline = None;
        }
    }

    fn with_movement_send<T>(&self, send: impl FnOnce() -> T) -> Option<T> {
        let state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch
            || state.self_guid != self.self_guid
            || !state.self_created
            || !state.self_control
            || !state.pending_orders.is_empty()
            || state.rooted
        {
            return None;
        }
        // Hold the same mutex used by reader-side revocation through the socket write. Once a
        // revoke acquires it, no later movement packet can pass an earlier authority check.
        Some(send())
    }

    /// Targeting/melee require this session's observed self and control, but root does not
    /// prevent a player from selecting or swinging. Serialize against reader-side retirement.
    fn with_player_action<T>(&self, send: impl FnOnce() -> T) -> Option<T> {
        let state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch
            || state.self_guid != self.self_guid
            || !state.self_created
            || !state.self_control
        {
            return None;
        }
        Some(send())
    }

    #[cfg(test)]
    fn with_ground_send(
        &self,
        pose: GroundPose,
        send: impl FnOnce() -> Result<()>,
    ) -> Option<Result<()>> {
        self.with_on_foot_send(pose.into(), send)
    }

    fn with_on_foot_send(
        &self,
        pose: OnFootPose,
        send: impl FnOnce() -> Result<()>,
    ) -> Option<Result<()>> {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch
            || state.self_guid != self.self_guid
            || !state.self_created
            || !state.self_control
            || !state.pending_orders.is_empty()
            || state.rooted
        {
            return None;
        }
        let result = send();
        if result.is_ok() {
            state.ground_pose = Some(pose);
        }
        Some(result)
    }

    fn raise_unset_barrier(&self, order: UnsetCanFlyOrder) -> Result<()> {
        self.raise_authority_order(PendingAuthorityOrder::UnsetCanFly(order))
    }

    fn raise_root_barrier(&self, order: ForceRootOrder, rooted: bool) -> Result<()> {
        self.raise_authority_order(PendingAuthorityOrder::ForceRoot { order, rooted })
    }

    fn raise_authority_order(&self, pending: PendingAuthorityOrder) -> Result<()> {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch || state.self_guid != self.self_guid {
            bail!("stale Wrath movement-authority order");
        }
        let order = match pending {
            PendingAuthorityOrder::UnsetCanFly(order)
            | PendingAuthorityOrder::ForceRoot { order, .. } => order,
        };
        if order.mover_guid != self.self_guid {
            state.self_control = false;
            state.control_decided = true;
            bail!(
                "Wrath movement-authority order named foreign mover {:#018x}",
                order.mover_guid
            );
        }
        if state.last_order_counter.is_some_and(|previous| {
            let advance = order.counter.wrapping_sub(previous);
            advance == 0 || advance >= 0x8000_0000
        }) {
            state.self_control = false;
            state.control_decided = true;
            bail!(
                "duplicate or stale Wrath movement-authority counter {} after {:?}",
                order.counter,
                state.last_order_counter
            );
        }
        // AzerothCore reconnect login may send an initial UNROOT before initial self packets.
        state.last_order_counter = Some(order.counter);
        state.pending_orders.push_back(pending);
        Ok(())
    }

    fn send_root_ack(
        &self,
        order: ForceRootOrder,
        rooted: bool,
        timestamp: u32,
        mut send: impl FnMut(u32, &[u8]) -> Result<()>,
    ) -> Result<UnsetAckResult> {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch || state.self_guid != self.self_guid {
            return Ok(UnsetAckResult::Stale);
        }
        let expected = PendingAuthorityOrder::ForceRoot { order, rooted };
        if order.mover_guid != self.self_guid || state.pending_orders.front() != Some(&expected) {
            state.self_control = false;
            state.control_decided = true;
            bail!("Wrath force-root ACK did not match the pending order head");
        }
        let Some(mut pose) = state.ground_pose else {
            return Ok(UnsetAckResult::PendingSelfPose);
        };
        if rooted && pose.flags != 0 {
            pose = pose.halted();
            let stop = wrath_ground::encode_on_foot(
                wrath_ground::MSG_MOVE_STOP,
                self.self_guid,
                0,
                timestamp,
                pose.position,
                pose.orientation,
                pose.fall_time,
                pose.fall,
            )?;
            send(wrath_ground::MSG_MOVE_STOP, &stop)?;
            // STOP was successfully written and became the last pose reported to the server. Keep
            // that honest progress if the following ACK write fatally fails; this epoch is retired
            // rather than retried through a socket whose delivery state is no longer trustworthy.
            state.ground_pose = Some(pose);
        }
        let body = wrath_authority::encode_force_root_on_foot_ack(
            self.self_guid,
            order.counter,
            pose,
            timestamp,
            rooted,
        )?;
        send(
            if rooted {
                wrath_authority::CMSG_FORCE_MOVE_ROOT_ACK
            } else {
                wrath_authority::CMSG_FORCE_MOVE_UNROOT_ACK
            },
            &body,
        )?;
        let retired = state.pending_orders.pop_front();
        debug_assert_eq!(retired, Some(expected));
        state.rooted = rooted;
        Ok(UnsetAckResult::Sent)
    }

    fn send_unset_ack(
        &self,
        order: UnsetCanFlyOrder,
        timestamp: u32,
        send: impl FnOnce(&[u8]) -> Result<()>,
    ) -> Result<UnsetAckResult> {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch || state.self_guid != self.self_guid {
            return Ok(UnsetAckResult::Stale);
        }
        let expected = PendingAuthorityOrder::UnsetCanFly(order);
        if order.mover_guid != self.self_guid || state.pending_orders.front() != Some(&expected) {
            state.self_control = false;
            state.control_decided = true;
            bail!("Wrath UNSET_CAN_FLY ACK did not match the pending order head");
        }
        let Some(pose) = state.ground_pose else {
            return Ok(UnsetAckResult::PendingSelfPose);
        };
        let body = wrath_authority::encode_unset_can_fly_on_foot_ack(
            self.self_guid,
            order.counter,
            pose,
            timestamp,
        )?;
        send(&body)?;
        let retired = state.pending_orders.pop_front();
        debug_assert_eq!(retired, Some(expected));
        Ok(UnsetAckResult::Sent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnsetAckResult {
    Sent,
    PendingSelfPose,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WrathRoute {
    GroundMovement,
    TaxiSplineDone,
    SetActiveMover,
    Logout,
    NativeItemQuery,
    NativeMelee,
    NativeLoot,
    NativeSession,
    DisabledMovement,
    DisabledGameAction,
    DeferredReadOnlyQuery,
}

fn route(command: &ClientCommand) -> WrathRoute {
    match command {
        ClientCommand::SetActiveMover { .. } => WrathRoute::SetActiveMover,
        ClientCommand::Logout | ClientCommand::ForceLogout => WrathRoute::Logout,
        ClientCommand::Move { .. } => WrathRoute::GroundMovement,
        ClientCommand::ItemQuery { .. } => WrathRoute::NativeItemQuery,
        ClientCommand::Loot { .. }
        | ClientCommand::AutostoreLootItem { .. }
        | ClientCommand::LootMoney
        | ClientCommand::LootRelease { .. } => WrathRoute::NativeLoot,
        ClientCommand::SetSelection { .. }
        | ClientCommand::AttackSwing { .. }
        | ClientCommand::AttackStop
        | ClientCommand::StandStateChange { .. }
        | ClientCommand::SetSheathed { .. } => WrathRoute::NativeMelee,
        ClientCommand::GroupInvite { .. }
        | ClientCommand::GroupAccept
        | ClientCommand::GroupDecline
        | ClientCommand::PartyAssignment { .. }
        | ClientCommand::GroupLeave
        | ClientCommand::GroupRaidConvert
        | ClientCommand::LootMethod { .. }
        | ClientCommand::SetRaidTarget { .. }
        | ClientCommand::GroupChangeSubGroup { .. }
        | ClientCommand::GroupSwapSubGroup { .. }
        | ClientCommand::GroupAssistantLeader { .. }
        | ClientCommand::ReadyCheckStart
        | ClientCommand::ReadyCheckAnswer { .. }
        | ClientCommand::RequestPartyMemberStats { .. }
        | ClientCommand::NameQuery { .. }
        | ClientCommand::CreatureQuery { .. }
        | ClientCommand::GameObjectQuery { .. }
        | ClientCommand::SetActionButton { .. }
        | ClientCommand::CastSpell { .. }
        | ClientCommand::CastSpellAtDest { .. }
        | ClientCommand::CastSpellAtSource { .. }
        | ClientCommand::CastSpellGameObject { .. }
        | ClientCommand::CastSpellItem { .. }
        | ClientCommand::CancelCast { .. }
        | ClientCommand::CancelAura { .. }
        | ClientCommand::GossipHello { .. }
        | ClientCommand::GossipSelectOption { .. }
        | ClientCommand::NpcTextQuery { .. }
        | ClientCommand::QuestgiverStatusQuery { .. }
        | ClientCommand::QuestgiverHello { .. }
        | ClientCommand::QuestgiverQuery { .. }
        | ClientCommand::QuestgiverAccept { .. }
        | ClientCommand::QuestgiverComplete { .. }
        | ClientCommand::QuestgiverRequestReward { .. }
        | ClientCommand::QuestgiverChooseReward { .. }
        | ClientCommand::QuestQuery { .. }
        | ClientCommand::QuestlogRemove { .. }
        | ClientCommand::PushQuestToParty { .. }
        | ClientCommand::QuestConfirmAccept { .. }
        | ClientCommand::ListInventory { .. }
        | ClientCommand::BuyItem { .. }
        | ClientCommand::BuyItemInSlot { .. }
        | ClientCommand::SellItem { .. }
        | ClientCommand::BuybackItem { .. }
        | ClientCommand::RepairItem { .. }
        | ClientCommand::TaxiNodeStatusQuery { .. }
        | ClientCommand::TaxiQueryNodes { .. }
        | ClientCommand::ActivateTaxi { .. }
        | ClientCommand::ActivateTaxiExpress { .. }
        | ClientCommand::BinderActivate { .. }
        | ClientCommand::TrainerList { .. }
        | ClientCommand::TrainerBuySpell { .. }
        | ClientCommand::GetMailList { .. }
        | ClientCommand::SetChannelWatch { .. }
        | ClientCommand::ChannelDisplayList { .. }
        | ClientCommand::QueryNextMailTime => WrathRoute::NativeSession,
        // Deferred Wrath feature queries have no wire effect and never fabricate server state.
        ClientCommand::BattlefieldStatusRequest => {
            WrathRoute::DeferredReadOnlyQuery
        }
        ClientCommand::MoveSplineDone { .. } => WrathRoute::TaxiSplineDone,
        ClientCommand::MoveTimeSkipped { .. }
        | ClientCommand::ForceSpeedAck { .. }
        | ClientCommand::NotActiveMover { .. }
        | ClientCommand::TeleportAck { .. }
        | ClientCommand::WorldportAck
        | ClientCommand::MoveModeAck { .. }
        | ClientCommand::KnockBackAck { .. } => WrathRoute::DisabledMovement,
        _ => WrathRoute::DisabledGameAction,
    }
}

fn writer_loop(
    commands: &Receiver<SessionCommand>,
    control: Receiver<WrathWriterControl>,
    control_tx: &Sender<WrathWriterControl>,
    clock: &Mutex<PingClock>,
    fatal: &Sender<EpochFatal>,
) {
    let mut writer: Option<(u64, WrathWriterSwap)> = None;
    let mut ping_tick = crossbeam_channel::never();
    let mut pre_self_timeout = crossbeam_channel::never();
    let mut warned = 0;
    let mut pending_authority = std::collections::VecDeque::new();
    loop {
        crossbeam_channel::select! {
            recv(control) -> message => match message {
                Ok(WrathWriterControl::Install { epoch, session, installed }) => {
                    // Install is ordered with retire/internal/fatal traffic. Movement remains
                    // disabled until the reader confirms this session's self create/control.
                    writer = Some((epoch, *session));
                    pending_authority.clear();
                    pre_self_timeout = crossbeam_channel::never();
                    warned = 0;
                    clock.lock().unwrap_or_else(|p| p.into_inner()).clear();
                    ping_tick = crossbeam_channel::after(PING_INTERVAL);
                    let _ = installed.send(Ok(()));
                }
                Ok(WrathWriterControl::Internal { epoch, command }) => {
                    let result = match writer.as_mut() {
                        Some((active_epoch, active)) if *active_epoch == epoch => match command {
                            WrathWriterCommand::Reply(reply) => {
                                active.writer.send_raw(reply.opcode, &reply.body)
                            }
                            WrathWriterCommand::UnsetCanFly(order) => queue_or_send_authority(
                                active,
                                PendingAuthorityOrder::UnsetCanFly(order),
                                &mut pending_authority,
                                &mut pre_self_timeout,
                            ),
                            WrathWriterCommand::ForceRoot { order, rooted } => queue_or_send_authority(
                                active,
                                PendingAuthorityOrder::ForceRoot { order, rooted },
                                &mut pending_authority,
                                &mut pre_self_timeout,
                            ),
                            WrathWriterCommand::PoseReady => {
                                let mut result = Ok(());
                                while let Some(pending) = pending_authority.pop_front() {
                                    match send_pending_authority(active, pending) {
                                        Ok(UnsetAckResult::Sent | UnsetAckResult::Stale) => {}
                                        Ok(UnsetAckResult::PendingSelfPose) => {
                                            pending_authority.push_front(pending);
                                            break;
                                        }
                                        Err(error) => {
                                            result = Err(error);
                                            break;
                                        }
                                    }
                                }
                                if pending_authority.is_empty() {
                                    pre_self_timeout = crossbeam_channel::never();
                                }
                                result
                            }
                        },
                        _ => continue,
                    };
                    if let Err(error) = result {
                        let _ = control_tx.send(WrathWriterControl::Fatal {
                            epoch,
                            reason: format!("Wrath protocol reply failed: {error:#}"),
                        });
                    }
                }
                Ok(WrathWriterControl::Retire { epoch, retired }) => {
                    if writer.as_ref().is_some_and(|(active, _)| *active == epoch) {
                        // Disable timers in the same ordered retirement step, before any later install.
                        ping_tick = crossbeam_channel::never();
                        pre_self_timeout = crossbeam_channel::never();
                        writer = None;
                        pending_authority.clear();
                    }
                    let _ = retired.send(());
                }
                Ok(WrathWriterControl::Fatal { epoch, reason }) => {
                    if writer.as_ref().is_some_and(|(active, _)| *active == epoch) {
                        ping_tick = crossbeam_channel::never();
                        pre_self_timeout = crossbeam_channel::never();
                        writer = None;
                        pending_authority.clear();
                        let _ = fatal.send(EpochFatal { epoch, reason });
                    }
                }
                Err(_) => return,
            },
            recv(ping_tick) -> _ => {
                ping_tick = crossbeam_channel::never();
                let mut failed = None;
                if let Some((_epoch, active)) = writer.as_mut() {
                    ping_tick = crossbeam_channel::after(PING_INTERVAL);
                    let (sequence, last_rtt) = {
                        let mut clock = clock.lock().unwrap_or_else(|p| p.into_inner());
                        clock.sequence += 1;
                        clock.sent_at = Some(Instant::now());
                        (clock.sequence, clock.last_rtt_ms.unwrap_or(0))
                    };
                    let (opcode, body) = ping_packet(sequence, last_rtt);
                    failed = active.writer.send_raw(opcode, &body).err();
                }
                if let Some(error) = failed {
                    let epoch = writer.as_ref().map_or(0, |(epoch, _)| *epoch);
                    let _ = control_tx.send(WrathWriterControl::Fatal {
                        epoch,
                        reason: format!("Wrath ping failed: {error:#}"),
                    });
                }
            },
            recv(pre_self_timeout) -> _ => {
                pre_self_timeout = crossbeam_channel::never();
                if !pending_authority.is_empty() {
                    if let Some((epoch, active)) = writer.as_ref() {
                        expire_pre_self_orders(
                            &active.authority,
                            *epoch,
                            pending_authority.len(),
                            control_tx,
                        );
                    }
                }
            },
            recv(commands) -> command => {
                let Ok(SessionCommand { epoch, command }) = command else { return };
                let Some((active_epoch, active)) = writer.as_mut() else {
                    if warned < SEND_WARN_CAP {
                        bevy::log::warn!("net(wrath): dropping command without live session: {command:?}");
                        warned += 1;
                    }
                    continue;
                };
                if !accepts_epoch(epoch, *active_epoch) {
                    continue;
                }
                let result = match route(&command) {
                    WrathRoute::SetActiveMover => {
                        let ClientCommand::SetActiveMover { guid } = command else { unreachable!() };
                        if guid != active.authority.self_guid {
                            active.authority.revoke();
                            let _ = control_tx.send(WrathWriterControl::Fatal {
                                epoch,
                                reason: format!("unsupported Wrath active-mover transfer to {guid:#018x}"),
                            });
                            continue;
                        }
                        let authority = active.authority.clone();
                        authority.with_movement_send(|| {
                            let (opcode, body) = active_mover_packet(guid);
                            active.writer.send_raw(opcode, &body)
                        }).unwrap_or(Ok(()))
                    }
                    WrathRoute::Logout => {
                        let authority = active.authority.clone();
                        authority.send_logout(|| {
                            let (opcode, body) = logout_packet();
                            active.writer.send_raw(opcode, &body)
                        })
                    }
                    WrathRoute::GroundMovement => send_ground_movement(active, command),
                    WrathRoute::TaxiSplineDone => {
                        let ClientCommand::MoveSplineDone { flags, pos, orientation, spline_id } = command else { unreachable!() };
                        active.authority.send_taxi_done(flags, pos, orientation, spline_id, active.clock.now_ms(), |opcode, body| active.writer.send_raw(opcode, body))
                    }
                    WrathRoute::NativeItemQuery => {
                        let ClientCommand::ItemQuery { entry, guid } = command else {
                            unreachable!()
                        };
                        let (opcode, body) = item_query_packet(entry, guid);
                        active.writer.send_raw(opcode, &body)
                    }
                    WrathRoute::NativeMelee => send_melee(active, command),
                    WrathRoute::NativeLoot => send_loot(active, command),
                    WrathRoute::NativeSession => {
                        let (opcode, body) = session_packet(command);
                        if *crate::net::CAST_TRACE {
                            bevy::log::info!("net(wrath) SEND 0x{opcode:04x} {body:02x?}");
                        }
                        active.writer.send_raw(opcode, &body)
                    }
                    WrathRoute::DeferredReadOnlyQuery => {
                        if warned < SEND_WARN_CAP {
                            bevy::log::warn!("net(wrath): query unavailable in this client; no request sent or result synthesized: {command:?}");
                            warned += 1;
                        }
                        continue;
                    }
                    // Not yet encoded for build 12340: dropped, never sent in Vanilla's format, and
                    // never fatal: the stock UI issues many of these routinely.
                    WrathRoute::DisabledMovement | WrathRoute::DisabledGameAction => {
                        if warned < SEND_WARN_CAP {
                            bevy::log::warn!("net(wrath): command not yet encoded for build 12340; dropped: {command:?}");
                            warned += 1;
                        }
                        continue;
                    }
                };
                if let Err(error) = result {
                    let _ = control_tx.send(WrathWriterControl::Fatal {
                        epoch,
                        reason: format!("Wrath lifecycle send failed: {error:#}"),
                    });
                }
            }
        }
    }
}

fn melee_packet(command: ClientCommand) -> Result<(u32, Vec<u8>)> {
    Ok(match command {
        ClientCommand::SetSelection { guid } => (
            wrath_combat::CMSG_SET_SELECTION,
            wrath_combat::encode_set_selection(guid),
        ),
        ClientCommand::AttackSwing { guid } => (
            wrath_combat::CMSG_ATTACK_SWING,
            wrath_combat::encode_attack_swing(guid),
        ),
        ClientCommand::AttackStop => (
            wrath_combat::CMSG_ATTACK_STOP,
            wrath_combat::encode_attack_stop(),
        ),
        ClientCommand::StandStateChange { state } => (
            wrath_combat::CMSG_STAND_STATE_CHANGE,
            wrath_combat::encode_stand_state_change(state)?,
        ),
        ClientCommand::SetSheathed { state } => {
            let state = match state {
                0 => wrath_combat::SheathState::Unarmed,
                1 => wrath_combat::SheathState::Melee,
                2 => wrath_combat::SheathState::Ranged,
                _ => bail!("invalid Wrath sheath state {state}"),
            };
            (
                wrath_combat::CMSG_SET_SHEATHED,
                wrath_combat::encode_set_sheathed(state),
            )
        }
        _ => bail!("not a native Wrath melee command"),
    })
}

fn send_melee(active: &mut WrathWriterSwap, command: ClientCommand) -> Result<()> {
    let (opcode, body) = melee_packet(command)?;
    let authority = active.authority.clone();
    authority
        .with_player_action(|| active.writer.send_raw(opcode, &body))
        .unwrap_or(Ok(()))
}

fn loot_packet(command: ClientCommand) -> Result<(u32, Vec<u8>)> {
    Ok(match command {
        ClientCommand::Loot { guid } if guid != 0 => {
            (wrath_loot::CMSG_LOOT, wrath_loot::encode_loot(guid))
        }
        ClientCommand::AutostoreLootItem { slot } if slot < wrath_loot::MAX_LOOT_ITEMS => (
            wrath_loot::CMSG_AUTOSTORE_LOOT_ITEM,
            wrath_loot::encode_autostore_loot_item(slot),
        ),
        ClientCommand::LootMoney => (wrath_loot::CMSG_LOOT_MONEY, wrath_loot::encode_loot_money()),
        ClientCommand::LootRelease { guid } if guid != 0 => (
            wrath_loot::CMSG_LOOT_RELEASE,
            wrath_loot::encode_loot_release(guid),
        ),
        _ => bail!("invalid or unsupported native Wrath solo corpse-loot command"),
    })
}

/// Queries, the action bar and casting, in build 12340's layouts.
fn session_packet(command: ClientCommand) -> (u32, Vec<u8>) {
    use benilla_protocol::wrath_group as g;
    use benilla_protocol::wrath_npc as n;
    use benilla_protocol::wrath_vendor as v;
    use benilla_protocol::wrath_trainer as t;
    use benilla_protocol::wrath_session::{self as s, CastTarget};
    // The client's cast counter, echoed by the server's cast results.
    static CAST_COUNT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
    let cast = |spell_id: u32, target: CastTarget| {
        let count = CAST_COUNT
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .wrapping_add(1);
        (s::CMSG_CAST_SPELL, s::encode_cast_spell(count, spell_id, &target))
    };
    match command {
        ClientCommand::NameQuery { guid } => (s::CMSG_NAME_QUERY, s::encode_name_query(guid)),
        ClientCommand::CreatureQuery { entry, guid } => {
            (s::CMSG_CREATURE_QUERY, s::encode_template_query(entry, guid))
        }
        ClientCommand::GameObjectQuery { entry, guid } => {
            (s::CMSG_GAMEOBJECT_QUERY, s::encode_template_query(entry, guid))
        }
        ClientCommand::SetActionButton { button, packed } => (
            s::CMSG_SET_ACTION_BUTTON,
            s::encode_set_action_button(button, packed),
        ),
        ClientCommand::CastSpell { spell_id, target } => cast(
            spell_id,
            target.map_or(CastTarget::None, CastTarget::Unit),
        ),
        ClientCommand::CastSpellAtDest { spell_id, dest } => cast(spell_id, CastTarget::Dest(dest)),
        ClientCommand::CastSpellAtSource { spell_id, src } => {
            cast(spell_id, CastTarget::Source(src))
        }
        ClientCommand::CastSpellGameObject { spell_id, go_guid } => {
            cast(spell_id, CastTarget::GameObject(go_guid))
        }
        ClientCommand::CastSpellItem { spell_id, item_guid } => {
            cast(spell_id, CastTarget::Item(item_guid))
        }
        ClientCommand::CancelCast { spell_id } => {
            (s::CMSG_CANCEL_CAST, s::encode_cancel_cast(spell_id))
        }
        ClientCommand::CancelAura { spell_id } => {
            (s::CMSG_CANCEL_AURA, s::encode_cancel_aura(spell_id))
        }
        ClientCommand::GossipHello { guid } => (n::CMSG_GOSSIP_HELLO, guid.to_le_bytes().to_vec()),
        ClientCommand::GossipSelectOption { guid, option } => (
            n::CMSG_GOSSIP_SELECT_OPTION,
            n::encode_gossip_select_option(guid, option),
        ),
        ClientCommand::NpcTextQuery { text_id, guid } => {
            (n::CMSG_NPC_TEXT_QUERY, n::encode_npc_text_query(text_id, guid))
        }
        ClientCommand::QuestgiverStatusQuery { npc } => {
            (n::CMSG_QUESTGIVER_STATUS_QUERY, npc.to_le_bytes().to_vec())
        }
        ClientCommand::QuestgiverHello { npc } => {
            (n::CMSG_QUESTGIVER_HELLO, npc.to_le_bytes().to_vec())
        }
        ClientCommand::QuestgiverQuery { npc, quest } => (
            n::CMSG_QUESTGIVER_QUERY_QUEST,
            n::encode_questgiver_query_quest(npc, quest),
        ),
        ClientCommand::QuestgiverAccept { npc, quest } => (
            n::CMSG_QUESTGIVER_ACCEPT_QUEST,
            n::encode_questgiver_accept_quest(npc, quest),
        ),
        ClientCommand::QuestgiverComplete { npc, quest } => (
            n::CMSG_QUESTGIVER_COMPLETE_QUEST,
            n::encode_guid_quest(npc, quest),
        ),
        ClientCommand::QuestgiverRequestReward { npc, quest } => (
            n::CMSG_QUESTGIVER_REQUEST_REWARD,
            n::encode_guid_quest(npc, quest),
        ),
        ClientCommand::QuestgiverChooseReward { npc, quest, choice } => (
            n::CMSG_QUESTGIVER_CHOOSE_REWARD,
            n::encode_questgiver_choose_reward(npc, quest, choice),
        ),
        ClientCommand::QuestQuery { quest } => (n::CMSG_QUEST_QUERY, quest.to_le_bytes().to_vec()),
        ClientCommand::QuestlogRemove { slot } => (n::CMSG_QUESTLOG_REMOVE_QUEST, vec![slot]),
        ClientCommand::PushQuestToParty { quest } => {
            (n::CMSG_PUSHQUESTTOPARTY, quest.to_le_bytes().to_vec())
        }
        ClientCommand::QuestConfirmAccept { quest } => {
            (n::CMSG_QUEST_CONFIRM_ACCEPT, quest.to_le_bytes().to_vec())
        }
        ClientCommand::ListInventory { guid } => (v::CMSG_LIST_INVENTORY, guid.to_le_bytes().to_vec()),
        ClientCommand::BuyItem { vendor, entry, count } => {
            (v::CMSG_BUY_ITEM, v::encode_buy_item(vendor, entry, count))
        }
        ClientCommand::BuyItemInSlot { vendor, entry, bag_guid, bag_slot, count } => (
            v::CMSG_BUY_ITEM_IN_SLOT,
            v::encode_buy_item_in_slot(vendor, entry, bag_guid, bag_slot, count),
        ),
        ClientCommand::SellItem { vendor, item_guid, count } => {
            (v::CMSG_SELL_ITEM, v::encode_sell_item(vendor, item_guid, count))
        }
        ClientCommand::BuybackItem { vendor, slot } => {
            (v::CMSG_BUYBACK_ITEM, v::encode_buyback_item(vendor, slot))
        }
        ClientCommand::RepairItem { vendor, item_guid } => {
            (v::CMSG_REPAIR_ITEM, v::encode_repair_item(vendor, item_guid))
        }
        ClientCommand::TaxiNodeStatusQuery { guid } => (benilla_protocol::wrath_taxi::CMSG_TAXINODE_STATUS_QUERY, benilla_protocol::wrath_taxi::encode_query(guid)),
        ClientCommand::TaxiQueryNodes { guid } => (benilla_protocol::wrath_taxi::CMSG_TAXIQUERYAVAILABLENODES, benilla_protocol::wrath_taxi::encode_query(guid)),
        ClientCommand::ActivateTaxi { guid, source_node, dest_node } => (benilla_protocol::wrath_taxi::CMSG_ACTIVATETAXI, benilla_protocol::wrath_taxi::encode_activate(guid, source_node, dest_node)),
        ClientCommand::ActivateTaxiExpress { guid, nodes, .. } => (benilla_protocol::wrath_taxi::CMSG_ACTIVATETAXIEXPRESS, benilla_protocol::wrath_taxi::encode_express(guid, &nodes)),
        ClientCommand::GroupInvite { name } => (
            g::CMSG_GROUP_INVITE,
            g::encode_invite(name.split('\0').next().unwrap_or("")).expect("NUL-free name"),
        ),
        ClientCommand::GroupAccept => (g::CMSG_GROUP_ACCEPT, g::encode_accept()),
        ClientCommand::GroupDecline => (g::CMSG_GROUP_DECLINE, g::encode_decline()),
        ClientCommand::PartyAssignment { assignment, apply, guid } => (g::MSG_PARTY_ASSIGNMENT, g::encode_assignment(assignment, apply, guid)),
        ClientCommand::GroupLeave => (g::CMSG_GROUP_DISBAND, g::encode_leave()),
        ClientCommand::GroupRaidConvert => (g::commands::CMSG_GROUP_RAID_CONVERT, Vec::new()),
        ClientCommand::LootMethod { method, master, threshold } =>
            (g::commands::CMSG_LOOT_METHOD, g::commands::loot_method(method, master, threshold)),
        ClientCommand::SetRaidTarget { icon, guid } =>
            (g::commands::MSG_RAID_TARGET_UPDATE, g::commands::raid_target(icon, guid)),
        ClientCommand::GroupChangeSubGroup { name, group } =>
            (g::commands::CMSG_GROUP_CHANGE_SUB_GROUP, g::commands::change_subgroup(name.split('\0').next().unwrap_or(""), group).expect("NUL-free name")),
        ClientCommand::GroupSwapSubGroup { name, other } =>
            (g::commands::CMSG_GROUP_SWAP_SUB_GROUP, g::commands::swap_subgroup(name.split('\0').next().unwrap_or(""), other.split('\0').next().unwrap_or("")).expect("NUL-free names")),
        ClientCommand::GroupAssistantLeader { guid, grant } =>
            (g::commands::CMSG_GROUP_ASSISTANT_LEADER, g::commands::assistant(guid, grant)),
        ClientCommand::ReadyCheckStart => (g::commands::MSG_RAID_READY_CHECK, Vec::new()),
        ClientCommand::ReadyCheckAnswer { ready } =>
            (g::commands::MSG_RAID_READY_CHECK, vec![u8::from(ready)]),
        ClientCommand::RequestPartyMemberStats { guid } => (
            g::CMSG_REQUEST_PARTY_MEMBER_STATS,
            g::encode_request_stats(guid),
        ),
        ClientCommand::BinderActivate { binder } => (
            benilla_protocol::wrath_bind::CMSG_BINDER_ACTIVATE,
            benilla_protocol::wrath_bind::encode_activate(binder),
        ),
        ClientCommand::TrainerList { trainer } => (t::CMSG_TRAINER_LIST, t::encode_list(trainer)),
        ClientCommand::TrainerBuySpell { trainer, spell_id } => {
            (t::CMSG_TRAINER_BUY_SPELL, t::encode_buy(trainer, spell_id))
        }
        ClientCommand::GetMailList { mailbox } => (
            benilla_protocol::wrath_mail::CMSG_GET_MAIL_LIST,
            mailbox.to_le_bytes().to_vec(),
        ),
        ClientCommand::QueryNextMailTime => (
            benilla_protocol::wrath_mail::MSG_QUERY_NEXT_MAIL_TIME as u32,
            benilla_protocol::wrath_mail::encode_query_next_mail_time(),
        ),
        ClientCommand::SetChannelWatch { name } => (
            benilla_protocol::wrath_channel::CMSG_SET_CHANNEL_WATCH,
            benilla_protocol::wrath_channel::encode_channel_name(&name),
        ),
        ClientCommand::ChannelDisplayList { name } => (
            benilla_protocol::wrath_channel::CMSG_CHANNEL_DISPLAY_LIST,
            benilla_protocol::wrath_channel::encode_channel_name(&name),
        ),
        other => unreachable!("not a Wrath session command: {other:?}"),
    }
}

fn send_loot(active: &mut WrathWriterSwap, command: ClientCommand) -> Result<()> {
    let (opcode, body) = loot_packet(command)?;
    let authority = active.authority.clone();
    authority
        .with_player_action(|| active.writer.send_raw(opcode, &body))
        .unwrap_or_else(|| {
            Err(anyhow!(
                "Wrath loot action lost session/control authority before send"
            ))
        })
}

fn expire_pre_self_orders(
    authority: &MovementAuthorityHandle,
    epoch: u64,
    pending: usize,
    control: &Sender<WrathWriterControl>,
) {
    if pending == 0 {
        return;
    }
    authority.revoke();
    let _ = control.send(WrathWriterControl::Fatal {
        epoch,
        reason: format!(
            "Wrath self pose did not arrive within {} ms with {pending} pending movement order(s)",
            PRE_SELF_ORDER_TIMEOUT.as_millis()
        ),
    });
}

fn accepts_epoch(command_epoch: u64, active_epoch: u64) -> bool {
    command_epoch == active_epoch
}

fn send_ground_movement(active: &mut WrathWriterSwap, command: ClientCommand) -> Result<()> {
    let ClientCommand::Move {
        kind,
        flags,
        pos,
        orientation,
        pitch,
        fall_time,
        jump,
        transport,
    } = command
    else {
        unreachable!()
    };
    if pitch != 0.0 || transport.is_some() {
        bail!("Wrath on-foot movement refuses pitch/transport tails");
    }
    let fall = jump.map(|jump| wrath_ground::FallTail {
        vertical_speed: jump.zspeed,
        sin_angle: jump.sin_angle,
        cos_angle: jump.cos_angle,
        horizontal_speed: jump.xy_speed,
    });
    let pose = OnFootPose {
        flags,
        position: pos,
        orientation,
        fall_time,
        fall,
    };
    let authority = active.authority.clone();
    authority
        .with_on_foot_send(pose, || {
            let opcode = u32::from(kind.opcode());
            let body = wrath_ground::encode_on_foot(
                opcode,
                active.active_mover,
                flags,
                active.clock.now_ms(),
                pos,
                orientation,
                fall_time,
                fall,
            )?;
            active.writer.send_raw(opcode, &body)
        })
        .unwrap_or(Ok(()))
}

fn send_pending_authority(
    active: &mut WrathWriterSwap,
    pending: PendingAuthorityOrder,
) -> Result<UnsetAckResult> {
    match pending {
        PendingAuthorityOrder::UnsetCanFly(order) => send_unset_can_fly(active, order),
        PendingAuthorityOrder::ForceRoot { order, rooted } => {
            send_force_root(active, order, rooted)
        }
    }
}

fn queue_or_send_authority(
    active: &mut WrathWriterSwap,
    pending: PendingAuthorityOrder,
    queue: &mut std::collections::VecDeque<PendingAuthorityOrder>,
    timeout: &mut Receiver<Instant>,
) -> Result<()> {
    // Once one pre-self order is waiting, preserve receipt order without probing later orders.
    if !queue.is_empty() {
        if queue.len() == MAX_PENDING_PRE_SELF_ORDERS {
            bail!("too many Wrath movement orders before self pose");
        }
        queue.push_back(pending);
        return Ok(());
    }
    match send_pending_authority(active, pending)? {
        UnsetAckResult::PendingSelfPose => {
            *timeout = crossbeam_channel::after(PRE_SELF_ORDER_TIMEOUT);
            queue.push_back(pending);
        }
        UnsetAckResult::Sent | UnsetAckResult::Stale => {}
    }
    Ok(())
}

fn send_unset_can_fly(
    active: &mut WrathWriterSwap,
    order: UnsetCanFlyOrder,
) -> Result<UnsetAckResult> {
    let authority = active.authority.clone();
    let timestamp = active.clock.now_ms();
    authority.send_unset_ack(order, timestamp, |body| {
        active
            .writer
            .send_raw(wrath_authority::CMSG_MOVE_SET_CAN_FLY_ACK, body)
    })
}

fn send_force_root(
    active: &mut WrathWriterSwap,
    order: ForceRootOrder,
    rooted: bool,
) -> Result<UnsetAckResult> {
    let authority = active.authority.clone();
    let timestamp = active.clock.now_ms();
    authority.send_root_ack(order, rooted, timestamp, |opcode, body| {
        active.writer.send_raw(opcode, body)
    })
}

fn reject_unsupported_movement(authority: &MovementAuthorityHandle, opcode: u16) -> Result<()> {
    if mandatory_movement_order(opcode) {
        authority.revoke();
        bail!("unsupported mandatory Wrath movement/control order {opcode:#05x}");
    }
    Ok(())
}

fn mandatory_movement_order(opcode: u16) -> bool {
    matches!(
        opcode,
        0x00c5
            | 0x00de
            | 0x00df
            | 0x00e2
            | 0x00e4
            | 0x00e6
            | 0x00ef
            | 0x00f2
            | 0x00f3
            | 0x00f4
            | 0x00f5
            | 0x02da
            | 0x02dc
            | 0x02de
            | 0x033e
            | 0x033f
            | 0x0343
            | 0x0381
            | 0x0383
            | 0x045c
            | 0x04ce
            | 0x04d0
            | 0x0516
    )
}

fn client_control_update(body: &[u8]) -> Result<(u64, bool)> {
    let mut rest = body;
    let guid = benilla_protocol::wire::read_packed_guid(&mut rest)
        .context("Wrath SMSG_CLIENT_CONTROL_UPDATE packed guid")?;
    if rest.len() != 1 || rest[0] > 1 {
        bail!("Wrath SMSG_CLIENT_CONTROL_UPDATE has invalid allowMove/body length");
    }
    Ok((guid, rest[0] != 0))
}

pub(crate) fn active_mover_packet(guid: u64) -> (u32, Vec<u8>) {
    (CMSG_SET_ACTIVE_MOVER, guid.to_le_bytes().to_vec())
}

pub(crate) fn ping_packet(sequence: u32, last_rtt_ms: u32) -> (u32, Vec<u8>) {
    let mut body = Vec::with_capacity(8);
    body.extend(sequence.to_le_bytes());
    body.extend(last_rtt_ms.to_le_bytes());
    (CMSG_PING, body)
}

pub(crate) fn logout_packet() -> (u32, Vec<u8>) {
    (CMSG_LOGOUT_REQUEST, Vec::new())
}

pub(crate) fn item_query_packet(entry: u32, guid: u64) -> (u32, Vec<u8>) {
    (
        CMSG_ITEM_QUERY_SINGLE,
        benilla_protocol::wrath_items::encode_item_query(entry, guid),
    )
}

fn normalize_realms(
    realms: &[benilla_protocol::wrath_auth::RealmInfo],
) -> Vec<benilla_protocol::RealmInfo> {
    realms
        .iter()
        .map(|realm| benilla_protocol::RealmInfo {
            name: realm.name.clone(),
            address: realm.address.clone(),
            population: realm.population,
            characters: realm.characters,
            realm_type: u32::from(realm.icon),
            flags: realm.flags | if realm.locked { 0x02 } else { 0 },
            category: realm.timezone,
            id: realm.id,
        })
        .collect()
}

fn normalize_characters(characters: Vec<WrathCharacter>) -> Vec<Character> {
    characters.into_iter().map(normalize_character).collect()
}

fn normalize_character(character: WrathCharacter) -> Character {
    let mut equipment = [CharEnumItem::default(); 19];
    for (target, source) in equipment.iter_mut().zip(character.equipment.iter()) {
        *target = CharEnumItem {
            display_id: source.display_id,
            inventory_type: source.inventory_type,
        };
    }
    Character {
        guid: character.guid,
        name: character.name,
        race: character.race,
        class: character.class,
        gender: character.gender,
        skin: character.skin,
        face: character.face,
        hair_style: character.hair_style,
        hair_color: character.hair_color,
        facial_hair: character.facial_hair,
        level: character.level,
        zone: character.zone,
        map: character.map,
        position: benilla_protocol::wire::Vector3d {
            x: character.position[0],
            y: character.position[1],
            z: character.position[2],
        },
        flags: character.flags,
        equipment,
        pet_display_id: character.pet_display_id,
        pet_level: character.pet_level,
        pet_family: character.pet_family,
    }
}

fn retryable_timeout(error: &anyhow::Error) -> bool {
    if error.downcast_ref::<WrathPartialFrameRead>().is_some() {
        return false;
    }
    error.downcast_ref::<io::Error>().is_some_and(|error| {
        matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        )
    })
}

fn one_byte(body: &[u8], packet: &str) -> Result<u8> {
    if body.len() != 1 {
        bail!("Wrath {packet} has {} bytes, expected 1", body.len());
    }
    Ok(body[0])
}

fn le_u32_exact(body: &[u8], packet: &str) -> Result<u32> {
    if body.len() != 4 {
        bail!("Wrath {packet} has {} bytes, expected 4", body.len());
    }
    Ok(u32::from_le_bytes(body.try_into().unwrap()))
}

fn send(events: &Sender<SessionEvent>, event: SessionEvent) -> Result<()> {
    events.send(event).map_err(|_| anyhow!("app exited"))
}

fn send_login_failure(events: &Sender<SessionEvent>, reason: &str, terminal: bool) {
    let _ = events.send(SessionEvent::LoginFailed {
        refusal: None,
        reason: reason.into(),
        terminal,
        dial: None,
    });
}

fn drain<T>(receiver: &Receiver<T>) {
    while receiver.try_recv().is_ok() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::wrath_world::WrathCharEnumItem;

    fn test_self_position() -> MovementPosition {
        MovementPosition::Stationary(benilla_protocol::wrath_gameplay::Position {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            orientation: 0.5,
        })
    }

    #[test]
    fn immutable_endpoints_reject_old_sends_across_the_ecs_connection_barrier() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let old = crate::net::SessionCommandSender::new(tx.clone(), 7);
        let old_clone = old.clone();
        old.send(ClientCommand::Logout).unwrap();
        let new = crate::net::SessionCommandSender::new(tx, 8);
        old_clone.send(ClientCommand::ForceLogout).unwrap();
        new.send(ClientCommand::Logout).unwrap();

        let queued: Vec<_> = rx.try_iter().collect();
        assert_eq!(
            queued
                .iter()
                .map(|command| command.epoch)
                .collect::<Vec<_>>(),
            [7, 7, 8]
        );
        let accepted: Vec<_> = queued
            .into_iter()
            .filter(|command| accepts_epoch(command.epoch, 8))
            .collect();
        assert_eq!(accepted.len(), 1);
        assert!(matches!(accepted[0].command, ClientCommand::Logout));
    }

    #[test]
    fn stale_internal_retire_and_fatal_are_rejected_by_new_epoch() {
        let active = 12;
        assert_ne!(11, active, "stale app envelope is rejected");

        let stale_internal = WrathWriterControl::Internal {
            epoch: 11,
            command: WrathWriterCommand::Reply(WrathReply {
                opcode: 1,
                body: Vec::new(),
            }),
        };
        let (retired, _ack) = crossbeam_channel::bounded(0);
        let stale_retire = WrathWriterControl::Retire { epoch: 11, retired };
        let stale_fatal = WrathWriterControl::Fatal {
            epoch: 11,
            reason: "old failure".into(),
        };
        for control in [stale_internal, stale_retire, stale_fatal] {
            let epoch = match control {
                WrathWriterControl::Internal { epoch, .. }
                | WrathWriterControl::Retire { epoch, .. }
                | WrathWriterControl::Fatal { epoch, .. } => epoch,
                WrathWriterControl::Install { .. } => unreachable!(),
            };
            assert_ne!(epoch, active);
        }
    }

    #[test]
    fn reader_guard_waits_for_synchronous_retirement_acknowledgement() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(23, 0x23);
        authority
            .observe_self_create(0x23, &test_self_position())
            .unwrap();
        assert_eq!(authority.with_movement_send(|| 1), Some(1));
        let observed = authority.clone();
        let writer = std::thread::spawn(move || {
            let WrathWriterControl::Retire { epoch, retired } = rx.recv().unwrap() else {
                panic!("expected retirement");
            };
            assert_eq!(epoch, 23);
            assert!(observed.with_movement_send(|| ()).is_none());
            retired.send(()).unwrap();
        });
        {
            let _guard = WriterRetireGuard {
                epoch: 23,
                control: tx,
                authority: authority.clone(),
            };
        }
        assert!(authority.with_movement_send(|| ()).is_none());
        writer.join().unwrap();
    }

    #[test]
    fn realm_and_roster_normalization_preserve_shared_fields() {
        let realms = normalize_realms(&[benilla_protocol::wrath_auth::RealmInfo {
            icon: 1,
            locked: true,
            flags: 4,
            name: "Realm".into(),
            address: "127.0.0.1:8085".into(),
            population: 0.5,
            characters: 2,
            timezone: 8,
            id: 7,
            specified_build: None,
        }]);
        assert_eq!(realms[0].name, "Realm");
        assert_eq!(realms[0].realm_type, 1);
        assert_eq!(realms[0].flags, 6);
        assert_eq!(realms[0].category, 8);

        let mut source_equipment = [WrathCharEnumItem::default(); 23];
        source_equipment[18].display_id = 123;
        source_equipment[19].display_id = 999;
        let roster = normalize_characters(vec![WrathCharacter {
            guid: 5,
            name: "Hero".into(),
            race: 1,
            class: 1,
            gender: 0,
            skin: 2,
            face: 3,
            hair_style: 4,
            hair_color: 5,
            facial_hair: 6,
            level: 7,
            zone: 8,
            map: 9,
            position: [1.0, 2.0, 3.0],
            guild_id: 10,
            flags: 11,
            customization_flags: 12,
            first_login: true,
            pet_display_id: 13,
            pet_level: 14,
            pet_family: 15,
            equipment: source_equipment,
        }]);
        assert_eq!(roster[0].guid, 5);
        assert_eq!(roster[0].position.x, 1.0);
        assert_eq!(roster[0].equipment[18].display_id, 123);
        assert_eq!(roster[0].equipment.len(), 19);
    }

    #[test]
    fn exact_supported_packets_and_disabled_routes_are_explicit() {
        assert_eq!(
            active_mover_packet(0x0102_0304_0506_0708),
            (0x26a, vec![8, 7, 6, 5, 4, 3, 2, 1])
        );
        assert_eq!(ping_packet(3, 40), (0x1dc, vec![3, 0, 0, 0, 40, 0, 0, 0]));
        assert_eq!(logout_packet(), (0x4b, Vec::new()));
        assert_eq!(
            item_query_packet(0x1234_5678, 0x0123_4567_89ab_cdef),
            (
                0x56,
                vec![0x78, 0x56, 0x34, 0x12, 0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01,]
            )
        );
        assert_eq!(
            route(&ClientCommand::ItemQuery { entry: 1, guid: 2 }),
            WrathRoute::NativeItemQuery
        );
        assert_eq!(route(&ClientCommand::Logout), WrathRoute::Logout);
        assert_eq!(
            route(&ClientCommand::WorldportAck),
            WrathRoute::DisabledMovement
        );
        assert_eq!(
            route(&ClientCommand::QueryTime),
            WrathRoute::DisabledGameAction
        );
    }

    #[test]
    fn quest_turn_in_and_abandon_route_exact_native_commands() {
        let npc = 0xf130_0001_2300_4567u64;
        let quest = 783u32;
        let guid_quest = [npc.to_le_bytes().as_slice(), quest.to_le_bytes().as_slice()].concat();
        for (command, opcode, body) in [
            (ClientCommand::QuestgiverComplete { npc, quest }, 0x18a, guid_quest.clone()),
            (ClientCommand::QuestgiverRequestReward { npc, quest }, 0x18c, guid_quest.clone()),
            (ClientCommand::QuestgiverChooseReward { npc, quest, choice: 2 }, 0x18e,
                [guid_quest.as_slice(), 2u32.to_le_bytes().as_slice()].concat()),
            (ClientCommand::QuestlogRemove { slot: 24 }, 0x194, vec![24]),
        ] {
            assert_eq!(route(&command), WrathRoute::NativeSession);
            assert_eq!(session_packet(command), (opcode, body));
        }
    }

    #[test]
    fn deferred_queries_are_a_narrow_nonfatal_route_not_a_game_action_bypass() {
        for command in [
            ClientCommand::BattlefieldStatusRequest,
        ] {
            assert_eq!(route(&command), WrathRoute::DeferredReadOnlyQuery);
        }
        assert_eq!(
            route(&ClientCommand::QueryTime),
            WrathRoute::DisabledGameAction
        );
        assert_eq!(
            route(&ClientCommand::WorldportAck),
            WrathRoute::DisabledMovement
        );
    }

    #[test]
    fn native_melee_packet_routing_is_exact_and_never_uses_a_vanilla_writer() {
        for (command, opcode, bytes) in [
            (
                ClientCommand::SetSelection { guid: 0x1234 },
                0x13d,
                0x1234u64.to_le_bytes().to_vec(),
            ),
            (
                ClientCommand::AttackSwing { guid: 0x5678 },
                0x141,
                0x5678u64.to_le_bytes().to_vec(),
            ),
            (ClientCommand::AttackStop, 0x142, Vec::new()),
            (
                ClientCommand::SetSheathed { state: 1 },
                0x1e0,
                1u32.to_le_bytes().to_vec(),
            ),
            (
                ClientCommand::StandStateChange { state: 0 },
                0x101,
                0u32.to_le_bytes().to_vec(),
            ),
        ] {
            assert_eq!(route(&command), WrathRoute::NativeMelee);
            assert_eq!(melee_packet(command).unwrap(), (opcode, bytes));
        }
        assert!(melee_packet(ClientCommand::SetSheathed { state: 3 }).is_err());
        assert!(melee_packet(ClientCommand::StandStateChange { state: 2 }).is_err());
    }

    #[test]
    fn trainer_commands_route_native_with_full_guid() {
        let trainer = 0xf130000000001234;
        for (command, opcode, body) in [
            (
                ClientCommand::TrainerList { trainer },
                0x1b0,
                trainer.to_le_bytes().to_vec(),
            ),
            (
                ClientCommand::TrainerBuySpell {
                    trainer,
                    spell_id: 2018,
                },
                0x1b2,
                [
                    trainer.to_le_bytes().as_slice(),
                    2018u32.to_le_bytes().as_slice(),
                ]
                .concat(),
            ),
        ] {
            assert_eq!(route(&command), WrathRoute::NativeSession);
            assert_eq!(session_packet(command), (opcode, body));
        }
    }

    #[test]
    fn solo_corpse_loot_routes_encode_native_bytes_and_keep_other_services_disabled() {
        for (command, opcode, body) in [
            (
                ClientCommand::Loot { guid: 0x1234 },
                0x15d,
                0x1234u64.to_le_bytes().to_vec(),
            ),
            (
                ClientCommand::AutostoreLootItem { slot: 17 },
                0x108,
                vec![17],
            ),
            (ClientCommand::LootMoney, 0x15e, vec![]),
            (
                ClientCommand::LootRelease { guid: 0x1234 },
                0x15f,
                0x1234u64.to_le_bytes().to_vec(),
            ),
        ] {
            assert_eq!(route(&command), WrathRoute::NativeLoot);
            assert_eq!(loot_packet(command).unwrap(), (opcode, body));
        }
        assert!(loot_packet(ClientCommand::Loot { guid: 0 }).is_err());
        assert!(loot_packet(ClientCommand::LootRelease { guid: 0 }).is_err());
        assert!(loot_packet(ClientCommand::AutostoreLootItem { slot: 18 }).is_err());
        assert_eq!(
            route(&ClientCommand::QueryTime),
            WrathRoute::DisabledGameAction
        );
    }

    #[test]
    fn player_action_authority_requires_current_self_control_but_not_movement_permission() {
        let shared = Arc::new(MovementAuthority::default());
        let old = shared.install(60, 0x55);
        assert!(old.with_player_action(|| ()).is_none());
        old.observe_self_create(0x55, &test_self_position())
            .unwrap();
        assert_eq!(old.with_player_action(|| 1), Some(1));
        let root = ForceRootOrder {
            mover_guid: 0x55,
            counter: 1,
        };
        old.raise_root_barrier(root, true).unwrap();
        old.send_root_ack(root, true, 2, |_, _| Ok(())).unwrap();
        assert!(old.with_movement_send(|| ()).is_none());
        assert_eq!(
            old.with_player_action(|| 2),
            Some(2),
            "root cannot disable selection/melee"
        );
        old.revoke();
        assert!(old.with_player_action(|| ()).is_none());
        let current = shared.install(61, 0x55);
        current
            .observe_self_create(0x55, &test_self_position())
            .unwrap();
        assert!(old.with_player_action(|| ()).is_none());
        assert_eq!(current.with_player_action(|| 3), Some(3));
    }

    #[test]
    fn movement_authority_requires_self_create_before_normal_initial_grant() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(1, 0x55);
        let sends = AtomicU64::new(0);

        assert_eq!(
            authority.with_movement_send(|| sends.fetch_add(1, Ordering::SeqCst)),
            None
        );
        let error = authority
            .observe_self_create(0x66, &test_self_position())
            .unwrap_err();
        assert!(error.to_string().contains("foreign mover"));
        assert_eq!(
            authority.with_movement_send(|| sends.fetch_add(1, Ordering::SeqCst)),
            None
        );
        let authority = shared.install(2, 0x55);
        authority
            .observe_self_create(0x55, &test_self_position())
            .unwrap();
        assert_eq!(
            authority.with_movement_send(|| sends.fetch_add(1, Ordering::SeqCst)),
            Some(0)
        );
        assert_eq!(sends.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn pre_self_order_timeout_revokes_and_queues_epoch_fatal() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(35, 0x351);
        authority.observe_control(0x351, true).unwrap();
        let (control_tx, control_rx) = crossbeam_channel::unbounded();

        expire_pre_self_orders(&authority, 35, 2, &control_tx);

        let WrathWriterControl::Fatal { epoch, reason } = control_rx.recv().unwrap() else {
            panic!("expected fatal control");
        };
        assert_eq!(epoch, 35);
        assert!(reason.contains("self pose did not arrive"));
        assert!(reason.contains("2 pending movement order(s)"));
        authority
            .observe_self_create(0x351, &test_self_position())
            .unwrap();
        assert!(authority.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn self_control_grant_and_deny_gate_movement() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(2, 0x77);
        authority
            .observe_self_create(0x77, &test_self_position())
            .unwrap();
        authority.observe_control(0x77, false).unwrap();
        assert!(authority.with_movement_send(|| ()).is_none());
        authority.observe_control(0x77, true).unwrap();
        assert_eq!(authority.with_movement_send(|| 9), Some(9));
        authority.observe_control(0x77, false).unwrap();
        assert!(authority.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn deny_before_self_create_prevents_implicit_grant() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(8, 0xee);

        authority.observe_control(0xee, false).unwrap();
        authority
            .observe_self_create(0xee, &test_self_position())
            .unwrap();

        assert!(authority.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn repeated_self_create_does_not_restore_revoked_control() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(9, 0xff);

        authority
            .observe_self_create(0xff, &test_self_position())
            .unwrap();
        assert_eq!(authority.with_movement_send(|| 1), Some(1));
        authority.revoke();
        authority
            .observe_self_create(0xff, &test_self_position())
            .unwrap();

        assert!(authority.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn explicit_grant_before_self_create_still_requires_create() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(10, 0x110);

        authority.observe_control(0x110, true).unwrap();
        assert!(authority.with_movement_send(|| ()).is_none());
        authority
            .observe_self_create(0x110, &test_self_position())
            .unwrap();

        assert_eq!(authority.with_movement_send(|| 2), Some(2));
    }

    #[test]
    fn new_epoch_resets_explicit_control_decision() {
        let shared = Arc::new(MovementAuthority::default());
        let denied = shared.install(11, 0x120);
        denied.observe_control(0x120, false).unwrap();
        denied
            .observe_self_create(0x120, &test_self_position())
            .unwrap();
        assert!(denied.with_movement_send(|| ()).is_none());

        let replacement = shared.install(12, 0x130);
        replacement
            .observe_self_create(0x130, &test_self_position())
            .unwrap();

        assert_eq!(replacement.with_movement_send(|| 3), Some(3));
        assert!(denied.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn foreign_mover_revokes_before_failing_closed() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(3, 0x88);
        authority
            .observe_self_create(0x88, &test_self_position())
            .unwrap();
        let error = authority.observe_control(0x99, true).unwrap_err();
        assert!(error.to_string().contains("non-player mover"));
        assert!(authority.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn unsupported_mandatory_orders_are_classified_fail_closed() {
        for opcode in [0x00c5, 0x00e2, 0x00ef, 0x0343, 0x04ce, 0x0516] {
            assert!(mandatory_movement_order(opcode), "{opcode:#05x}");
        }
        for opcode in [
            SMSG_PONG,
            SMSG_CLIENT_CONTROL_UPDATE,
            wrath_authority::SMSG_MOVE_UNSET_CAN_FLY,
            wrath_authority::SMSG_FORCE_MOVE_ROOT,
            wrath_authority::SMSG_FORCE_MOVE_UNROOT,
            0x00dd,
        ] {
            assert!(!mandatory_movement_order(opcode), "{opcode:#05x}");
        }

        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(4, 0xaa);
        authority
            .observe_self_create(0xaa, &test_self_position())
            .unwrap();
        assert!(mandatory_movement_order(0x0343));
        authority.revoke();
        assert!(authority.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn stale_authority_handles_cannot_send_or_change_replacement_session() {
        let shared = Arc::new(MovementAuthority::default());
        let stale = shared.install(5, 0xbb);
        stale
            .observe_self_create(0xbb, &test_self_position())
            .unwrap();
        assert_eq!(stale.with_movement_send(|| 1), Some(1));

        let current = shared.install(6, 0xcc);
        stale
            .observe_self_create(0xbb, &test_self_position())
            .unwrap();
        stale.observe_control(0xbb, true).unwrap();
        assert!(stale.with_movement_send(|| ()).is_none());
        assert!(current.with_movement_send(|| ()).is_none());
        current
            .observe_self_create(0xcc, &test_self_position())
            .unwrap();
        assert_eq!(current.with_movement_send(|| 2), Some(2));
    }

    #[test]
    fn revoke_waits_for_in_flight_send_and_prevents_every_later_send() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(7, 0xdd);
        authority
            .observe_self_create(0xdd, &test_self_position())
            .unwrap();
        let sending = authority.clone();
        let revoking = authority.clone();
        let (entered_tx, entered_rx) = crossbeam_channel::bounded(0);
        let (release_tx, release_rx) = crossbeam_channel::bounded(0);
        let sent = Arc::new(AtomicU64::new(0));
        let sent_in_thread = Arc::clone(&sent);
        let writer = std::thread::spawn(move || {
            sending.with_movement_send(|| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                sent_in_thread.fetch_add(1, Ordering::SeqCst);
            })
        });
        entered_rx.recv().unwrap();
        let revoke = std::thread::spawn(move || revoking.revoke());
        release_tx.send(()).unwrap();
        writer.join().unwrap();
        revoke.join().unwrap();

        assert_eq!(sent.load(Ordering::SeqCst), 1);
        assert!(authority
            .with_movement_send(|| sent.fetch_add(1, Ordering::SeqCst))
            .is_none());
        assert_eq!(sent.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unset_barrier_blocks_app_movement_and_releases_only_after_successful_ack() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(30, 0x301);
        authority
            .observe_self_create(0x301, &test_self_position())
            .unwrap();
        let order = UnsetCanFlyOrder {
            mover_guid: 0x301,
            counter: 0x1122_3344,
        };

        authority.raise_unset_barrier(order).unwrap();
        assert!(authority.with_movement_send(|| ()).is_none());
        let failed = authority.send_unset_ack(order, 9, |_| bail!("socket failure"));
        assert!(failed.is_err());
        assert!(authority.with_movement_send(|| ()).is_none());

        let mut ack = Vec::new();
        assert_eq!(
            authority
                .send_unset_ack(order, 10, |body| {
                    ack.extend_from_slice(body);
                    Ok(())
                })
                .unwrap(),
            UnsetAckResult::Sent
        );
        assert_eq!(&ack[3..7], &order.counter.to_le_bytes());
        assert_eq!(&ack[ack.len() - 4..], &[0, 0, 0, 0]);
        assert_eq!(authority.with_movement_send(|| 1), Some(1));
    }

    #[test]
    fn pre_self_unset_waits_without_fabricating_pose_then_uses_actual_create() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(31, 0x311);
        let order = UnsetCanFlyOrder {
            mover_guid: 0x311,
            counter: 7,
        };
        authority.raise_unset_barrier(order).unwrap();
        let sends = AtomicU64::new(0);
        assert_eq!(
            authority
                .send_unset_ack(order, 1, |_| {
                    sends.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .unwrap(),
            UnsetAckResult::PendingSelfPose
        );
        assert_eq!(sends.load(Ordering::SeqCst), 0);

        authority
            .observe_self_create(0x311, &test_self_position())
            .unwrap();
        let mut ack = Vec::new();
        assert_eq!(
            authority
                .send_unset_ack(order, 2, |body| {
                    ack.extend_from_slice(body);
                    Ok(())
                })
                .unwrap(),
            UnsetAckResult::Sent
        );
        assert_eq!(&ack[17..21], &1.0f32.to_le_bytes());
        assert_eq!(&ack[21..25], &2.0f32.to_le_bytes());
        assert_eq!(&ack[25..29], &3.0f32.to_le_bytes());
    }

    #[test]
    fn unset_does_not_restore_revoked_authority_and_stale_orders_retire() {
        let shared = Arc::new(MovementAuthority::default());
        let stale = shared.install(32, 0x321);
        stale
            .observe_self_create(0x321, &test_self_position())
            .unwrap();
        stale.revoke();
        let stale_order = UnsetCanFlyOrder {
            mover_guid: 0x321,
            counter: 8,
        };
        stale.raise_unset_barrier(stale_order).unwrap();
        assert_eq!(
            stale.send_unset_ack(stale_order, 3, |_| Ok(())).unwrap(),
            UnsetAckResult::Sent
        );
        assert!(stale.with_movement_send(|| ()).is_none());

        let current = shared.install(33, 0x331);
        assert_eq!(
            stale
                .send_unset_ack(stale_order, 4, |_| panic!("stale ACK sent"))
                .unwrap(),
            UnsetAckResult::Stale
        );
        assert!(current.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn root_then_unroot_keeps_denied_control_and_barrier_until_successful_write() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(40, 0x401);
        authority.observe_control(0x401, false).unwrap();
        authority
            .observe_self_create(0x401, &test_self_position())
            .unwrap();
        let order = ForceRootOrder {
            mover_guid: 0x401,
            counter: 11,
        };
        authority.raise_root_barrier(order, true).unwrap();
        assert!(authority.with_movement_send(|| ()).is_none());
        let mut opcodes = Vec::new();
        assert_eq!(
            authority
                .send_root_ack(order, true, 7, |opcode, _| {
                    opcodes.push(opcode);
                    Ok(())
                })
                .unwrap(),
            UnsetAckResult::Sent
        );
        assert_eq!(opcodes, [wrath_authority::CMSG_FORCE_MOVE_ROOT_ACK]);
        assert!(authority.with_movement_send(|| ()).is_none());

        let unroot = ForceRootOrder {
            mover_guid: 0x401,
            counter: 12,
        };
        authority.raise_root_barrier(unroot, false).unwrap();
        assert!(authority
            .send_root_ack(unroot, false, 8, |_, _| bail!("write failed"))
            .is_err());
        assert!(authority.with_movement_send(|| ()).is_none());
        authority
            .send_root_ack(unroot, false, 9, |_, _| Ok(()))
            .unwrap();
        assert!(authority.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn airborne_authority_uses_latest_reported_pose_and_failed_sends_do_not_advance_it() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(70, 0x0500_0001);
        authority
            .observe_self_create(0x0500_0001, &test_self_position())
            .unwrap();
        let pose = OnFootPose {
            flags: wrath_ground::MOVE_FORWARD | wrath_ground::MOVE_FALLING,
            position: [8.0, 9.0, 12.0],
            orientation: 0.75,
            fall_time: 300,
            fall: Some(wrath_ground::FallTail {
                vertical_speed: 7.955547,
                sin_angle: 0.25,
                cos_angle: 0.75,
                horizontal_speed: 7.0,
            }),
        };
        authority
            .with_on_foot_send(pose, || Ok(()))
            .unwrap()
            .unwrap();
        let newer = OnFootPose {
            position: [9.0, 9.0, 12.5],
            ..pose
        };
        assert!(authority
            .with_on_foot_send(newer, || bail!("write failed"))
            .unwrap()
            .is_err());
        let unset = UnsetCanFlyOrder {
            mover_guid: 0x0500_0001,
            counter: 20,
        };
        authority.raise_unset_barrier(unset).unwrap();
        assert!(authority
            .with_on_foot_send(newer, || panic!("order barrier bypass"))
            .is_none());
        let mut body = Vec::new();
        authority
            .send_unset_ack(unset, 500, |bytes| {
                body = bytes.to_vec();
                Ok(())
            })
            .unwrap();
        assert_eq!(
            body,
            wrath_authority::encode_unset_can_fly_on_foot_ack(0x0500_0001, 20, pose, 500,).unwrap()
        );
        let root = ForceRootOrder {
            mover_guid: 0x0500_0001,
            counter: 21,
        };
        authority.raise_root_barrier(root, true).unwrap();
        let mut packets = Vec::new();
        authority
            .send_root_ack(root, true, 600, |opcode, body| {
                packets.push((opcode, body.to_vec()));
                Ok(())
            })
            .unwrap();
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0].0, wrath_ground::MSG_MOVE_STOP);
        let stopped =
            benilla_protocol::wrath_gameplay::OutboundMovement::from_bytes(&packets[0].1).unwrap();
        assert_eq!(
            [
                stopped.movement.position.x,
                stopped.movement.position.y,
                stopped.movement.position.z
            ],
            pose.position
        );
        assert_eq!(stopped.movement.fall_time, 300);
        assert_eq!(stopped.movement.flags, 0);
        assert!(stopped.movement.jump.is_none());
        assert_eq!(
            packets[1].1,
            wrath_authority::encode_force_root_on_foot_ack(
                0x0500_0001,
                21,
                pose.halted(),
                600,
                true,
            )
            .unwrap()
        );
        assert!(authority
            .with_on_foot_send(newer, || panic!("root bypass"))
            .is_none());
        authority.observe_control(0x0500_0001, false).unwrap();
        let replacement = shared.install(71, 0x0500_0001);
        assert!(authority
            .with_on_foot_send(pose, || panic!("stale epoch bypass"))
            .is_none());
        assert!(replacement
            .with_on_foot_send(pose, || panic!("pre-self bypass"))
            .is_none());
    }

    #[test]
    fn moving_root_stops_before_root_ack() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(41, 0x411);
        authority
            .observe_self_create(0x411, &test_self_position())
            .unwrap();
        authority
            .with_ground_send(
                GroundPose::new(wrath_ground::MOVE_FORWARD, [4.0, 5.0, 6.0], 0.25).unwrap(),
                || Ok(()),
            )
            .unwrap()
            .unwrap();
        let order = ForceRootOrder {
            mover_guid: 0x411,
            counter: 13,
        };
        authority.raise_root_barrier(order, true).unwrap();
        let mut opcodes = Vec::new();
        authority
            .send_root_ack(order, true, 1, |opcode, _| {
                opcodes.push(opcode);
                Ok(())
            })
            .unwrap();
        assert_eq!(
            opcodes,
            [
                wrath_ground::MSG_MOVE_STOP,
                wrath_authority::CMSG_FORCE_MOVE_ROOT_ACK
            ]
        );
        assert!(authority.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn mixed_authority_orders_require_exact_fifo_ack_kind_and_counter() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(43, 0x431);
        authority
            .observe_self_create(0x431, &test_self_position())
            .unwrap();
        let unset = UnsetCanFlyOrder {
            mover_guid: 0x431,
            counter: 20,
        };
        let root = ForceRootOrder {
            mover_guid: 0x431,
            counter: 21,
        };
        authority.raise_unset_barrier(unset).unwrap();
        authority.raise_root_barrier(root, true).unwrap();
        assert!(authority
            .send_root_ack(root, true, 1, |_, _| Ok(()))
            .is_err());
        assert!(authority.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn duplicate_and_stale_root_orders_fail_closed() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(44, 0x441);
        authority
            .observe_self_create(0x441, &test_self_position())
            .unwrap();
        let root = ForceRootOrder {
            mover_guid: 0x441,
            counter: 30,
        };
        let unroot = ForceRootOrder {
            mover_guid: 0x441,
            counter: 31,
        };
        authority.raise_root_barrier(root, true).unwrap();
        authority.raise_root_barrier(unroot, false).unwrap();
        assert!(authority.raise_root_barrier(unroot, false).is_err());
        assert!(authority.raise_root_barrier(root, true).is_err());
        assert!(authority.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn queued_root_unroot_root_never_momentarily_allows_movement() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(45, 0x451);
        authority
            .observe_self_create(0x451, &test_self_position())
            .unwrap();
        let orders = [
            (
                ForceRootOrder {
                    mover_guid: 0x451,
                    counter: 40,
                },
                true,
            ),
            (
                ForceRootOrder {
                    mover_guid: 0x451,
                    counter: 41,
                },
                false,
            ),
            (
                ForceRootOrder {
                    mover_guid: 0x451,
                    counter: 42,
                },
                true,
            ),
        ];
        for (order, rooted) in orders {
            authority.raise_root_barrier(order, rooted).unwrap();
        }
        for (order, rooted) in orders {
            authority
                .send_root_ack(order, rooted, 1, |_, _| Ok(()))
                .unwrap();
            assert!(authority.with_movement_send(|| ()).is_none());
        }
    }

    #[test]
    fn pre_self_root_waits_without_writing() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(42, 0x421);
        let order = ForceRootOrder {
            mover_guid: 0x421,
            counter: 14,
        };
        authority.raise_root_barrier(order, true).unwrap();
        assert_eq!(
            authority
                .send_root_ack(order, true, 1, |_, _| panic!("pre-self write"))
                .unwrap(),
            UnsetAckResult::PendingSelfPose
        );
    }

    #[test]
    fn initial_pre_self_unroot_waits_for_pose_and_never_grants_denied_control() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(46, 0x461);
        authority.observe_control(0x461, false).unwrap();
        let order = ForceRootOrder {
            mover_guid: 0x461,
            counter: 50,
        };
        authority.raise_root_barrier(order, false).unwrap();
        assert_eq!(
            authority
                .send_root_ack(order, false, 1, |_, _| panic!("pre-self write"))
                .unwrap(),
            UnsetAckResult::PendingSelfPose
        );
        authority
            .observe_self_create(0x461, &test_self_position())
            .unwrap();

        let mut ack = Vec::new();
        assert_eq!(
            authority
                .send_root_ack(order, false, 2, |opcode, body| {
                    ack.push((opcode, body.to_vec()));
                    Ok(())
                })
                .unwrap(),
            UnsetAckResult::Sent
        );
        assert_eq!(ack.len(), 1);
        assert_eq!(ack[0].0, wrath_authority::CMSG_FORCE_MOVE_UNROOT_ACK);
        assert_eq!(&ack[0].1[17..21], &1.0f32.to_le_bytes());
        assert_eq!(&ack[0].1[21..25], &2.0f32.to_le_bytes());
        assert_eq!(&ack[0].1[25..29], &3.0f32.to_le_bytes());
        assert!(authority.with_movement_send(|| ()).is_none());
    }

    #[test]
    fn successful_allowed_self_initial_unroot_does_not_fabricate_root_state() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(47, 0x471);
        authority
            .observe_self_create(0x471, &test_self_position())
            .unwrap();
        let order = ForceRootOrder {
            mover_guid: 0x471,
            counter: 51,
        };
        authority.raise_root_barrier(order, false).unwrap();
        assert!(authority.with_movement_send(|| ()).is_none());
        assert_eq!(
            authority
                .send_root_ack(order, false, 3, |opcode, _| {
                    assert_eq!(opcode, wrath_authority::CMSG_FORCE_MOVE_UNROOT_ACK);
                    Ok(())
                })
                .unwrap(),
            UnsetAckResult::Sent
        );
        assert_eq!(authority.with_movement_send(|| 1), Some(1));
    }

    #[test]
    fn foreign_unset_revokes_and_fails_closed_before_enqueue() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(34, 0x341);
        authority
            .observe_self_create(0x341, &test_self_position())
            .unwrap();
        let error = authority
            .raise_unset_barrier(UnsetCanFlyOrder {
                mover_guid: 0x999,
                counter: 1,
            })
            .unwrap_err();
        assert!(error.to_string().contains("foreign mover"));
        assert!(authority.with_movement_send(|| ()).is_none());
    }
}

#[cfg(test)]
#[path = "wrath/bind_tests.rs"]
mod bind_tests;

#[cfg(test)]
#[path = "wrath/taxi_tests.rs"]
mod taxi_tests;

#[path = "wrath/taxi_authority.rs"]
mod taxi_authority;

#[cfg(test)]
mod group_tests {
    use super::*;
    #[test]
    fn group_commands_route_and_encode_native_bodies() {
        for (command, opcode, body) in [
            (ClientCommand::GroupInvite { name: "Ally".into() }, 0x6e, b"Ally\0\0\0\0\0".to_vec()),
            (ClientCommand::GroupAccept, 0x72, vec![0; 4]),
            (ClientCommand::GroupDecline, 0x73, vec![]),
            (ClientCommand::GroupLeave, 0x7b, vec![]),
            (ClientCommand::PartyAssignment { assignment: 1, apply: true, guid: 0x8877665544332211 }, 0x38e, vec![1,1,0x11,0x22,0x33,0x44,0x55,0x66,0x77,0x88]),
            (ClientCommand::PartyAssignment { assignment: 0, apply: false, guid: 0 }, 0x38e, vec![0;10]),
            (ClientCommand::RequestPartyMemberStats { guid: 7 }, 0x27f, vec![7,0,0,0,0,0,0,0]),
        ] {
            assert_eq!(route(&command), WrathRoute::NativeSession);
            assert_eq!(session_packet(command), (opcode, body));
        }
    }
}

#[cfg(test)]
#[path = "wrath/mail_tests.rs"]
mod mail_tests;

#[cfg(test)]
mod channel_command_tests {
    use super::*;
    #[test]
    fn native_channel_commands_route_and_encode_in_request_order() {
        let commands = [ClientCommand::SetChannelWatch { name: "General".into() }, ClientCommand::ChannelDisplayList { name: "General".into() }, ClientCommand::SetChannelWatch { name: String::new() }];
        for command in &commands { assert_eq!(route(command), WrathRoute::NativeSession); }
        let packets: Vec<_> = commands.into_iter().map(session_packet).collect();
        assert_eq!(packets, vec![(0x3ef, b"General\0".to_vec()), (0x3d2, b"General\0".to_vec()), (0x3ef, vec![0])]);
    }
}

#[cfg(test)]
#[path = "wrath/raid_command_tests.rs"]
mod raid_command_tests;
