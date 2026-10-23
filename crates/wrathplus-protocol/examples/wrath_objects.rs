//! Dedicated-account Wrath object observation probe.
//! The default remains read-only. `AZEROTHBENCH_OBJECT_MOVE=bounded` explicitly enables one
//! ordinary, sub-yard, on-foot movement probe followed by normal logout and roster verification.

use std::collections::{HashMap, HashSet};
use std::io;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use wrathplus_protocol::wrath_gameplay::{
    DestroyObject, MovementBlock, MovementInfo, MovementPosition, NamedField, ObjectType,
    ObjectUpdate, OutboundMovement, Position, UpdateFields, UpdateObject, MOVEMENT_FLAG_FALLING,
    MOVEMENT_FLAG_FLYING, MOVEMENT_FLAG_ON_TRANSPORT, MOVEMENT_FLAG_SPLINE_ELEVATION,
    MOVEMENT_FLAG_SPLINE_ENABLED, MOVEMENT_FLAG_SWIMMING,
};
use wrathplus_protocol::{
    wrath_auth,
    wrath_world::{WrathServerPacket, WrathWorldSession, WrathWorldWriter},
};

const ACCOUNT: &str = "AZBENCH";
const CHARACTER: &str = "Benchone";
const DEFAULT_DEADLINE_SECS: u64 = 15;
const MAX_DEADLINE_SECS: u64 = 60;
const DEFAULT_PACKET_LIMIT: usize = 2_048;
const MAX_PACKET_LIMIT: usize = 65_536;
const LOGOUT_DEADLINE: Duration = Duration::from_secs(25);
const LOGOUT_PACKET_LIMIT: usize = 512;
const MOVE_OPT_IN: &str = "bounded";
const FORWARD_DISTANCE: f32 = 0.75;
const RETURN_DISTANCE: f32 = 0.50;
const MAX_DISPLACEMENT: f32 = 0.75;
const MAX_TOTAL_PATH: f32 = 1.25;
const FORWARD_TIME: Duration = Duration::from_millis(300);
const RETURN_TIME: Duration = Duration::from_millis(200);
const MOVEMENT_DEADLINE: Duration = Duration::from_secs(2);
const MOVEMENT_PACKET_LIMIT: usize = 256;
const POSITION_TOLERANCE: f32 = 0.10;

const SMSG_NEW_WORLD: u16 = 0x03e;
const SMSG_TRANSFER_PENDING: u16 = 0x03f;
const SMSG_CHARACTER_LOGIN_FAILED: u16 = 0x041;
const CMSG_LOGOUT_REQUEST: u32 = 0x04b;
const SMSG_LOGOUT_RESPONSE: u16 = 0x04c;
const SMSG_LOGOUT_COMPLETE: u16 = 0x04d;
const MSG_MOVE_START_FORWARD: u32 = 0x0b5;
const MSG_MOVE_START_BACKWARD: u32 = 0x0b6;
const MSG_MOVE_STOP: u32 = 0x0b7;
const MSG_MOVE_TELEPORT_ACK: u16 = 0x0c7;
const SMSG_MONSTER_MOVE: u16 = 0x0dd;
const SMSG_FORCE_MOVE_ROOT: u16 = 0x0e8;
const SMSG_FORCE_MOVE_UNROOT: u16 = 0x0ea;
const MSG_MOVE_HEARTBEAT: u32 = 0x0ee;
const SMSG_MOVE_KNOCK_BACK: u16 = 0x0ef;
const SMSG_UPDATE_OBJECT: u16 = 0x0a9;
const SMSG_DESTROY_OBJECT: u16 = 0x0aa;
const SMSG_COMPRESSED_UPDATE_OBJECT: u16 = 0x1f6;
const SMSG_WARDEN_DATA: u16 = 0x2e6;
const SMSG_TIME_SYNC_REQ: u16 = 0x390;
const CMSG_TIME_SYNC_RESP: u32 = 0x391;

const MOVEMENT_FLAG_FORWARD: u32 = 0x0000_0001;
const MOVEMENT_FLAG_BACKWARD: u32 = 0x0000_0002;
const MOVEMENT_FLAG_WALKING: u32 = 0x0000_0100;
const MOVEMENT_FLAG_DISABLE_GRAVITY: u32 = 0x0000_0400;
const MOVEMENT_FLAG_ROOT: u32 = 0x0000_0800;
const MOVEMENT_FLAG_CAN_FLY: u32 = 0x0100_0000;
const MOVEMENT_FLAG_HOVER: u32 = 0x4000_0000;
const UNIT_FLAG_DISABLE_MOVE: u32 = 0x0000_0004;
const UNIT_FLAG_IN_COMBAT: u32 = 0x0008_0000;
const PLAYER_FLAG_GM: u32 = 0x0000_0008;
const UNSAFE_MOVEMENT_FLAGS: u32 = MOVEMENT_FLAG_ON_TRANSPORT
    | MOVEMENT_FLAG_DISABLE_GRAVITY
    | MOVEMENT_FLAG_ROOT
    | MOVEMENT_FLAG_FALLING
    | MOVEMENT_FLAG_SWIMMING
    | MOVEMENT_FLAG_CAN_FLY
    | MOVEMENT_FLAG_FLYING
    | MOVEMENT_FLAG_SPLINE_ELEVATION
    | MOVEMENT_FLAG_SPLINE_ENABLED
    | MOVEMENT_FLAG_HOVER;

#[derive(Debug, Default, PartialEq, Eq)]
struct Counts {
    create: usize,
    values: usize,
    movement: usize,
    out_of_range: usize,
    destroy: usize,
}

#[derive(Debug, Default)]
struct TrackedObject {
    object_type: Option<ObjectType>,
    position: Option<[f32; 4]>,
    living: Option<MovementInfo>,
    speeds: Option<[f32; 9]>,
    health: Option<u32>,
    unit_flags: Option<u32>,
    player_flags: Option<u32>,
}

#[derive(Debug, Default)]
struct Tracker {
    self_guid: u64,
    objects: HashMap<u64, TrackedObject>,
    object_types: HashMap<u64, ObjectType>,
    nearby_units: HashSet<u64>,
    decoded_units: HashSet<u64>,
    counts: Counts,
    self_created: bool,
}

impl Tracker {
    fn new(self_guid: u64) -> Self {
        Self {
            self_guid,
            ..Self::default()
        }
    }

    fn object_type(&self, guid: u64) -> Option<ObjectType> {
        self.object_types.get(&guid).copied()
    }

    fn observe_update(&mut self, update: UpdateObject) {
        for object in update.objects {
            match object {
                ObjectUpdate::Create {
                    guid,
                    object_type,
                    movement,
                    fields,
                    ..
                } => {
                    self.counts.create += 1;
                    self.object_types.insert(guid, object_type);
                    if guid == self.self_guid {
                        self.self_created = true;
                    } else if object_type == ObjectType::Unit {
                        self.nearby_units.insert(guid);
                        self.decoded_units.insert(guid);
                    }
                    let tracked = self.objects.entry(guid).or_default();
                    tracked.object_type = Some(object_type);
                    if matches!(object_type, ObjectType::Unit | ObjectType::Player) {
                        // Create fields are a complete snapshot; omitted zero-valued mask entries
                        // authoritatively mean that these flags are clear.
                        tracked.unit_flags = Some(0);
                    }
                    if object_type == ObjectType::Player {
                        tracked.player_flags = Some(0);
                    }
                    apply_movement(tracked, &movement);
                    apply_fields(tracked, &fields);
                }
                ObjectUpdate::Values { guid, fields } => {
                    self.counts.values += 1;
                    let tracked = self.objects.entry(guid).or_default();
                    tracked.object_type = Some(fields.object_type);
                    apply_fields(tracked, &fields);
                }
                ObjectUpdate::Movement { guid, movement } => {
                    self.counts.movement += 1;
                    apply_movement(self.objects.entry(guid).or_default(), &movement);
                }
                ObjectUpdate::OutOfRange(guids) => {
                    self.counts.out_of_range += guids.len();
                    for guid in guids {
                        self.remove(guid);
                    }
                }
                ObjectUpdate::Near(_) => {}
            }
        }
    }

    fn observe_destroy(&mut self, destroy: DestroyObject) {
        self.counts.destroy += 1;
        self.remove(destroy.guid);
    }

    fn remove(&mut self, guid: u64) {
        self.objects.remove(&guid);
        self.object_types.remove(&guid);
        self.nearby_units.remove(&guid);
    }

    fn acceptance(&self) -> Result<()> {
        if !self.self_created {
            bail!("acceptance failed: the selected character was not decoded in a create update");
        }
        if self.decoded_units.is_empty() {
            bail!("acceptance failed: no nearby unit create was decoded");
        }
        Ok(())
    }
}

fn apply_fields(tracked: &mut TrackedObject, fields: &UpdateFields) {
    if let Some(health) = fields.named(NamedField::UnitHealth) {
        tracked.health = Some(health);
    }
    if let Some(flags) = fields.named(NamedField::UnitFlags) {
        tracked.unit_flags = Some(flags);
    }
    if let Some(flags) = fields.named(NamedField::PlayerFlags) {
        tracked.player_flags = Some(flags);
    }
}

fn apply_movement(tracked: &mut TrackedObject, movement: &MovementBlock) {
    if let MovementPosition::Living(info) = &movement.position {
        tracked.living = Some(info.clone());
    }
    if let Some(speeds) = movement.speeds {
        tracked.speeds = Some(speeds);
    }
    tracked.position = match &movement.position {
        MovementPosition::Living(info) => Some([
            info.position.x,
            info.position.y,
            info.position.z,
            info.position.orientation,
        ]),
        MovementPosition::Positioned {
            world, orientation, ..
        } => Some([world[0], world[1], world[2], *orientation]),
        MovementPosition::Stationary(position) => {
            Some([position.x, position.y, position.z, position.orientation])
        }
        MovementPosition::None => tracked.position,
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MoveStep {
    opcode: u32,
    flags: u32,
    position: Position,
}

fn movement_sequence(origin: Position) -> Result<[MoveStep; 6]> {
    if !origin.x.is_finite()
        || !origin.y.is_finite()
        || !origin.z.is_finite()
        || !origin.orientation.is_finite()
    {
        bail!("movement origin is not finite");
    }
    let forward = offset_position(origin, FORWARD_DISTANCE);
    let returned = offset_position(origin, FORWARD_DISTANCE - RETURN_DISTANCE);
    if distance_3d(origin, forward) > MAX_DISPLACEMENT
        || distance_3d(origin, forward) + distance_3d(forward, returned) > MAX_TOTAL_PATH
    {
        bail!("bounded movement constants exceed their displacement/path limits");
    }
    Ok([
        MoveStep {
            opcode: MSG_MOVE_START_FORWARD,
            flags: MOVEMENT_FLAG_FORWARD | MOVEMENT_FLAG_WALKING,
            position: origin,
        },
        MoveStep {
            opcode: MSG_MOVE_HEARTBEAT,
            flags: MOVEMENT_FLAG_FORWARD | MOVEMENT_FLAG_WALKING,
            position: forward,
        },
        MoveStep {
            opcode: MSG_MOVE_STOP,
            flags: MOVEMENT_FLAG_WALKING,
            position: forward,
        },
        MoveStep {
            opcode: MSG_MOVE_START_BACKWARD,
            flags: MOVEMENT_FLAG_BACKWARD | MOVEMENT_FLAG_WALKING,
            position: forward,
        },
        MoveStep {
            opcode: MSG_MOVE_HEARTBEAT,
            flags: MOVEMENT_FLAG_BACKWARD | MOVEMENT_FLAG_WALKING,
            position: returned,
        },
        MoveStep {
            opcode: MSG_MOVE_STOP,
            flags: MOVEMENT_FLAG_WALKING,
            position: returned,
        },
    ])
}

fn offset_position(origin: Position, distance: f32) -> Position {
    Position {
        x: origin.x + origin.orientation.cos() * distance,
        y: origin.y + origin.orientation.sin() * distance,
        ..origin
    }
}

fn distance_3d(a: Position, b: Position) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

fn main() -> Result<()> {
    let config = Config::from_env()?;
    let login = wrath_auth::logon(&config.auth_host, &config.username, &config.password)?;
    let realm = login.realms.first().context("no realm advertised")?;
    let mut session = WrathWorldSession::connect(
        realm.address.as_str(),
        &config.username,
        login.session_key,
        u32::from(realm.id),
    )?;
    let characters = session.enumerate_characters()?;
    let character = characters
        .iter()
        .find(|character| character.name == CHARACTER)
        .context("dedicated Benchone character missing; this probe never creates characters")?;

    session.begin_player_login(character.guid)?;
    let (mut reader, mut writer) = session.into_split()?;
    let clock = Instant::now();
    let deadline = clock + config.deadline;
    let mut tracker = Tracker::new(character.guid);
    let mut packets = 0usize;

    while packets < config.packet_limit && Instant::now() < deadline {
        let packet = reader
            .recv_raw_until(deadline)
            .with_context(|| format!("reading observation packet {}", packets + 1))?;
        packets += 1;
        observe_packet(packet, &mut tracker, &mut writer, clock)?;
        if tracker.acceptance().is_ok() {
            break;
        }
    }

    tracker.acceptance()?;
    let own = tracker.objects.get(&character.guid);
    println!(
        "objects packets={packets} create={} values={} movement={} out_of_range={} destroy={} nearby_units={}",
        tracker.counts.create,
        tracker.counts.values,
        tracker.counts.movement,
        tracker.counts.out_of_range,
        tracker.counts.destroy,
        tracker.nearby_units.len()
    );
    println!(
        "self position={} health={}",
        format_position(own.and_then(|object| object.position)),
        own.and_then(|object| object.health)
            .map_or_else(|| "unknown".into(), |health| health.to_string())
    );

    let movement = if config.movement {
        Some(run_bounded_movement(
            &mut reader,
            &mut writer,
            &mut tracker,
            clock,
        ))
    } else {
        None
    };
    let logout = clean_logout(&mut reader, &mut writer, clock);
    if let Some(Err(error)) = movement.as_ref() {
        logout.context("normal logout after bounded movement failure")?;
        return Err(
            anyhow!(error.to_string()).context("bounded movement failed; STOP was attempted")
        );
    }
    logout?;
    if let Some(movement) = movement {
        let movement = movement.expect("movement error returned above");
        let verification =
            verify_persisted_position(&config, character.guid, character.map, movement)?;
        println!(
            "movement emitted: start/heartbeat/STOP, reverse/heartbeat/STOP; server does not echo ordinary movement to its mover"
        );
        match verification {
            PositionVerification::Verified(position) => println!(
                "PASS: bounded movement persisted at {:.3},{:.3},{:.3}; normal logout and roster verification completed",
                position[0], position[1], position[2]
            ),
            PositionVerification::Unverified(position) => println!(
                "UNVERIFIED: movement packets were emitted and logout completed, but roster position {:.3},{:.3},{:.3} did not match the final STOP",
                position[0], position[1], position[2]
            ),
        }
    } else {
        println!("PASS: self create and nearby unit creates decoded; normal logout completed");
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionVerification {
    Verified([f32; 3]),
    Unverified([f32; 3]),
}

fn run_bounded_movement(
    reader: &mut wrathplus_protocol::wrath_world::WrathWorldReader,
    writer: &mut WrathWorldWriter,
    tracker: &mut Tracker,
    clock: Instant,
) -> Result<Position> {
    let own = tracker
        .objects
        .get(&tracker.self_guid)
        .context("movement gate: self object is absent")?;
    validate_movement_gate(own)?;
    let origin = own.living.as_ref().unwrap().position;
    let sequence = movement_sequence(origin)?;
    let overall_deadline = Instant::now() + MOVEMENT_DEADLINE;
    let mut last_position = origin;
    let result = (|| {
        require_movement_time(overall_deadline)?;
        send_move(writer, tracker.self_guid, sequence[0], clock)?;
        wait_movement_phase(
            reader,
            writer,
            tracker,
            clock,
            Instant::now() + FORWARD_TIME,
            overall_deadline,
        )?;
        validate_tracked_self(tracker, "forward leg")?;
        require_movement_time(overall_deadline)?;
        send_move(writer, tracker.self_guid, sequence[1], clock)?;
        last_position = sequence[1].position;
        require_movement_time(overall_deadline)?;
        send_move(writer, tracker.self_guid, sequence[2], clock)?;
        require_movement_time(overall_deadline)?;
        send_move(writer, tracker.self_guid, sequence[3], clock)?;
        wait_movement_phase(
            reader,
            writer,
            tracker,
            clock,
            Instant::now() + RETURN_TIME,
            overall_deadline,
        )?;
        validate_tracked_self(tracker, "return leg")?;
        require_movement_time(overall_deadline)?;
        send_move(writer, tracker.self_guid, sequence[4], clock)?;
        last_position = sequence[4].position;
        require_movement_time(overall_deadline)?;
        send_move(writer, tracker.self_guid, sequence[5], clock)?;
        Ok(sequence[5].position)
    })();
    if result.is_err() {
        let _ = send_stop(writer, tracker.self_guid, last_position, clock);
    }
    result
}

fn require_movement_time(overall_deadline: Instant) -> Result<()> {
    if Instant::now() >= overall_deadline {
        bail!("movement gate exceeded its absolute deadline");
    }
    Ok(())
}

fn validate_tracked_self(tracker: &Tracker, phase: &str) -> Result<()> {
    validate_movement_gate(
        tracker
            .objects
            .get(&tracker.self_guid)
            .with_context(|| format!("movement gate: self object disappeared during {phase}"))?,
    )
}

fn validate_movement_gate(own: &TrackedObject) -> Result<()> {
    let movement = own
        .living
        .as_ref()
        .context("movement gate: self create was not living movement")?;
    let speeds = own
        .speeds
        .context("movement gate: self movement speeds were not decoded")?;
    if own
        .health
        .context("movement gate: self health was not decoded")?
        == 0
    {
        bail!("movement gate: self is not living");
    }
    let unit_flags = own
        .unit_flags
        .context("movement gate: UNIT_FIELD_FLAGS was not decoded")?;
    let player_flags = own
        .player_flags
        .context("movement gate: PLAYER_FLAGS was not decoded")?;
    if unit_flags & (UNIT_FLAG_IN_COMBAT | UNIT_FLAG_DISABLE_MOVE) != 0 {
        bail!("movement gate: self is in combat or movement-disabled");
    }
    if player_flags & PLAYER_FLAG_GM != 0 {
        bail!("movement gate: GM character refused");
    }
    if movement.flags != 0
        || movement.flags2 != 0
        || movement.transport.is_some()
        || movement.jump.is_some()
        || movement.pitch.is_some()
        || movement.spline_elevation.is_some()
        || movement.flags & UNSAFE_MOVEMENT_FLAGS != 0
    {
        bail!("movement gate: self is not stationary ordinary on-foot movement");
    }
    if !speeds.iter().all(|speed| speed.is_finite() && *speed > 0.0) {
        bail!("movement gate: self movement speeds are invalid");
    }
    if !movement.position.x.is_finite()
        || !movement.position.y.is_finite()
        || !movement.position.z.is_finite()
        || !movement.position.orientation.is_finite()
    {
        bail!("movement gate: self position is invalid");
    }
    Ok(())
}

fn send_move(
    writer: &mut WrathWorldWriter,
    guid: u64,
    step: MoveStep,
    clock: Instant,
) -> Result<()> {
    let movement = OutboundMovement {
        mover_guid: guid,
        movement: MovementInfo {
            flags: step.flags,
            flags2: 0,
            timestamp: u32::try_from(clock.elapsed().as_millis()).unwrap_or(u32::MAX),
            position: step.position,
            transport: None,
            pitch: None,
            fall_time: 0,
            jump: None,
            spline_elevation: None,
        },
    };
    writer
        .send_raw(step.opcode, &movement.to_bytes()?)
        .with_context(|| format!("sending bounded movement opcode {:#05x}", step.opcode))
}

fn send_stop(
    writer: &mut WrathWorldWriter,
    guid: u64,
    position: Position,
    clock: Instant,
) -> Result<()> {
    send_move(
        writer,
        guid,
        MoveStep {
            opcode: MSG_MOVE_STOP,
            flags: MOVEMENT_FLAG_WALKING,
            position,
        },
        clock,
    )
}

fn wait_movement_phase(
    reader: &mut wrathplus_protocol::wrath_world::WrathWorldReader,
    writer: &mut WrathWorldWriter,
    tracker: &mut Tracker,
    clock: Instant,
    phase_deadline: Instant,
    overall_deadline: Instant,
) -> Result<()> {
    let deadline = phase_deadline.min(overall_deadline);
    let mut packets = 0usize;
    while Instant::now() < deadline {
        if packets == MOVEMENT_PACKET_LIMIT {
            bail!("movement gate exceeded its packet limit");
        }
        match reader.recv_raw_until(deadline) {
            Ok(packet) => {
                packets += 1;
                match packet.opcode {
                    SMSG_TIME_SYNC_REQ => respond_time_sync(&packet.body, writer, clock)?,
                    SMSG_UPDATE_OBJECT | SMSG_COMPRESSED_UPDATE_OBJECT | SMSG_DESTROY_OBJECT => {
                        observe_packet(packet, tracker, writer, clock)?;
                        validate_tracked_self(tracker, "movement")?;
                    }
                    SMSG_MONSTER_MOVE => {
                        if monster_move_guid(&packet.body)? == tracker.self_guid {
                            bail!("movement gate: server-directed self spline arrived");
                        }
                    }
                    SMSG_NEW_WORLD
                    | SMSG_TRANSFER_PENDING
                    | MSG_MOVE_TELEPORT_ACK
                    | SMSG_FORCE_MOVE_ROOT
                    | SMSG_FORCE_MOVE_UNROOT
                    | SMSG_MOVE_KNOCK_BACK => {
                        bail!("movement gate: server-directed movement/teleport arrived")
                    }
                    SMSG_WARDEN_DATA => bail!("Warden is required and unsupported"),
                    _ => {}
                }
            }
            Err(error) if Instant::now() >= deadline && is_timeout(&error) => {
                // Probe with an expired deadline. Only a reader that consumed no frame byte
                // returns another timeout; a partial-frame timeout permanently poisons it.
                let probe = reader
                    .recv_raw_until(Instant::now())
                    .expect_err("an expired receive deadline cannot yield a packet");
                if retryable_timeout_pair(&error, &probe) {
                    require_movement_time(overall_deadline)?;
                    return Ok(());
                }
                return Err(error).context(format!(
                    "reading during bounded movement poisoned the reader: {probe:#}"
                ));
            }
            Err(error) => return Err(error).context("reading during bounded movement"),
        }
    }
    require_movement_time(overall_deadline)?;
    // Self echo is intentionally not required; AC relays ordinary movement to everyone but mover.
    Ok(())
}

fn is_timeout(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<io::Error>().map(io::Error::kind),
        Some(io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock)
    )
}

fn retryable_timeout_pair(first: &anyhow::Error, probe: &anyhow::Error) -> bool {
    is_timeout(first) && is_timeout(probe)
}

fn monster_move_guid(body: &[u8]) -> Result<u64> {
    let mut body = body;
    wrathplus_protocol::wire::read_packed_guid(&mut body)
        .context("decoding SMSG_MONSTER_MOVE mover guid")
}

fn verify_persisted_position(
    config: &Config,
    guid: u64,
    map: u32,
    expected: Position,
) -> Result<PositionVerification> {
    let login = wrath_auth::logon(&config.auth_host, &config.username, &config.password)?;
    let realm = login
        .realms
        .first()
        .context("no realm advertised on verification reconnect")?;
    let mut session = WrathWorldSession::connect(
        realm.address.as_str(),
        &config.username,
        login.session_key,
        u32::from(realm.id),
    )?;
    let character = session
        .enumerate_characters()?
        .into_iter()
        .find(|character| character.guid == guid)
        .context("moved character absent from verification roster")?;
    let actual = Position {
        x: character.position[0],
        y: character.position[1],
        z: character.position[2],
        orientation: expected.orientation,
    };
    if persisted_position_matches(character.map, actual, map, expected) {
        Ok(PositionVerification::Verified(character.position))
    } else {
        Ok(PositionVerification::Unverified(character.position))
    }
}

fn persisted_position_matches(
    actual_map: u32,
    actual: Position,
    expected_map: u32,
    expected: Position,
) -> bool {
    actual_map == expected_map && distance_3d(actual, expected) <= POSITION_TOLERANCE
}

fn observe_packet(
    packet: WrathServerPacket,
    tracker: &mut Tracker,
    writer: &mut WrathWorldWriter,
    clock: Instant,
) -> Result<()> {
    match packet.opcode {
        SMSG_UPDATE_OBJECT => {
            let update = UpdateObject::from_bytes_with_object_types(&packet.body, |guid| {
                tracker.object_type(guid)
            })?;
            tracker.observe_update(update);
        }
        SMSG_COMPRESSED_UPDATE_OBJECT => {
            let update =
                UpdateObject::from_compressed_bytes_with_object_types(&packet.body, |guid| {
                    tracker.object_type(guid)
                })?;
            tracker.observe_update(update);
        }
        SMSG_DESTROY_OBJECT => tracker.observe_destroy(DestroyObject::from_bytes(&packet.body)?),
        SMSG_TIME_SYNC_REQ => respond_time_sync(&packet.body, writer, clock)?,
        SMSG_CHARACTER_LOGIN_FAILED => {
            let code = packet
                .body
                .first()
                .copied()
                .context("empty login failure")?;
            bail!("Wrath server refused player login: result {code:#04x}");
        }
        SMSG_WARDEN_DATA => bail!("Warden is required and unsupported"),
        _ => {}
    }
    Ok(())
}

fn respond_time_sync(body: &[u8], writer: &mut WrathWorldWriter, clock: Instant) -> Result<()> {
    let counter: [u8; 4] = body
        .get(..4)
        .context("truncated SMSG_TIME_SYNC_REQ")?
        .try_into()
        .unwrap();
    if body.len() != 4 {
        bail!("SMSG_TIME_SYNC_REQ has {} bytes, expected 4", body.len());
    }
    // AzerothCore sends only a request counter and accepts an arbitrary client-clock epoch;
    // HandleTimeSyncResp derives the clock delta from this monotonic client timestamp.
    let timestamp = u32::try_from(clock.elapsed().as_millis()).unwrap_or(u32::MAX);
    let mut response = Vec::with_capacity(8);
    response.extend(counter);
    response.extend(timestamp.to_le_bytes());
    writer
        .send_raw(CMSG_TIME_SYNC_RESP, &response)
        .context("sending CMSG_TIME_SYNC_RESP")
}

fn clean_logout(
    reader: &mut wrathplus_protocol::wrath_world::WrathWorldReader,
    writer: &mut WrathWorldWriter,
    clock: Instant,
) -> Result<()> {
    writer
        .send_raw(CMSG_LOGOUT_REQUEST, &[])
        .context("sending CMSG_LOGOUT_REQUEST")?;
    let deadline = Instant::now() + LOGOUT_DEADLINE;
    let mut accepted = false;
    for _ in 0..LOGOUT_PACKET_LIMIT {
        if Instant::now() >= deadline {
            break;
        }
        let packet = reader
            .recv_raw_until(deadline)
            .context("waiting for normal logout")?;
        match packet.opcode {
            SMSG_TIME_SYNC_REQ => respond_time_sync(&packet.body, writer, clock)?,
            SMSG_LOGOUT_RESPONSE => {
                if packet.body.len() != 5 {
                    bail!(
                        "SMSG_LOGOUT_RESPONSE has {} bytes, expected 5",
                        packet.body.len()
                    );
                }
                let reason = u32::from_le_bytes(packet.body[..4].try_into().unwrap());
                if reason != 0 {
                    bail!("normal logout refused with reason {reason}");
                }
                accepted = true;
            }
            SMSG_LOGOUT_COMPLETE => return Ok(()),
            SMSG_WARDEN_DATA => bail!("Warden is required and unsupported"),
            _ => {}
        }
    }
    if accepted {
        bail!("normal logout was accepted but did not complete within 25 seconds");
    }
    bail!("normal logout received no response within 25 seconds")
}

fn format_position(position: Option<[f32; 4]>) -> String {
    position.map_or_else(
        || "unknown".into(),
        |p| format!("{:.3},{:.3},{:.3},{:.3}", p[0], p[1], p[2], p[3]),
    )
}

struct Config {
    auth_host: String,
    username: String,
    password: String,
    deadline: Duration,
    packet_limit: usize,
    movement: bool,
}

impl Config {
    fn from_env() -> Result<Self> {
        let auth_host =
            std::env::var("AZEROTHBENCH_AUTH_HOST").unwrap_or_else(|_| "127.0.0.1:3724".into());
        let username =
            std::env::var("AZEROTHBENCH_TEST_USER").context("dedicated test username required")?;
        let password = std::env::var("AZEROTHBENCH_TEST_PASSWORD")
            .context("dedicated test password required")?;
        if !username.eq_ignore_ascii_case(ACCOUNT) {
            bail!("this probe is restricted to the dedicated AZBENCH account");
        }
        let deadline_secs = bounded_env(
            "AZEROTHBENCH_OBJECT_DEADLINE_SECS",
            DEFAULT_DEADLINE_SECS,
            MAX_DEADLINE_SECS,
        )?;
        let packet_limit = bounded_env(
            "AZEROTHBENCH_OBJECT_PACKET_LIMIT",
            DEFAULT_PACKET_LIMIT,
            MAX_PACKET_LIMIT,
        )?;
        let movement = match std::env::var("AZEROTHBENCH_OBJECT_MOVE") {
            Ok(value) if value == MOVE_OPT_IN => true,
            Ok(value) => bail!(
                "AZEROTHBENCH_OBJECT_MOVE={value:?} is invalid; omit it for read-only observation or set it exactly to {MOVE_OPT_IN:?}"
            ),
            Err(std::env::VarError::NotPresent) => false,
            Err(error) => {
                return Err(anyhow!(error).context("reading AZEROTHBENCH_OBJECT_MOVE"));
            }
        };
        Ok(Self {
            auth_host,
            username,
            password,
            deadline: Duration::from_secs(deadline_secs),
            packet_limit,
            movement,
        })
    }
}

fn bounded_env<T>(name: &str, default: T, maximum: T) -> Result<T>
where
    T: Copy + std::str::FromStr + PartialOrd + std::fmt::Display,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let value = match std::env::var(name) {
        Ok(value) => value
            .parse::<T>()
            .with_context(|| format!("invalid {name}"))?,
        Err(std::env::VarError::NotPresent) => default,
        Err(error) => return Err(anyhow!(error).context(format!("reading {name}"))),
    };
    if value > maximum {
        bail!("{name}={value} exceeds maximum {maximum}");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wrathplus_protocol::wrath_gameplay::{FieldValue, Position, UPDATE_FLAG_STATIONARY_POSITION};

    fn safe_movement() -> MovementInfo {
        MovementInfo {
            flags: 0,
            flags2: 0,
            timestamp: 1,
            position: Position {
                x: 10.0,
                y: 20.0,
                z: 30.0,
                orientation: 0.0,
            },
            transport: None,
            pitch: None,
            fall_time: 0,
            jump: None,
            spline_elevation: None,
        }
    }

    fn safe_self() -> TrackedObject {
        TrackedObject {
            object_type: Some(ObjectType::Player),
            position: Some([10.0, 20.0, 30.0, 0.0]),
            living: Some(safe_movement()),
            speeds: Some([1.0; 9]),
            health: Some(100),
            unit_flags: Some(0),
            player_flags: Some(0),
        }
    }

    fn movement(x: f32) -> MovementBlock {
        MovementBlock {
            update_flags: UPDATE_FLAG_STATIONARY_POSITION,
            position: MovementPosition::Stationary(Position {
                x,
                y: 2.0,
                z: 3.0,
                orientation: 4.0,
            }),
            speeds: None,
            spline: None,
            unknown: None,
            low_guid: None,
            target_guid: None,
            transport_progress: None,
            vehicle: None,
            rotation: None,
        }
    }

    fn fields(object_type: ObjectType, health: Option<u32>) -> UpdateFields {
        UpdateFields {
            object_type,
            field_count: 64,
            values: health
                .map(|value| FieldValue { index: 0x18, value })
                .into_iter()
                .collect(),
        }
    }

    fn create(guid: u64, object_type: ObjectType, health: Option<u32>) -> ObjectUpdate {
        ObjectUpdate::Create {
            update_type: 2,
            guid,
            object_type,
            movement: movement(1.0),
            fields: fields(object_type, health),
        }
    }

    #[test]
    fn synthetic_tracking_applies_lifecycle_and_self_state() {
        let self_guid = 7;
        let unit_guid = 0xf130_0000_0000_0008;
        let mut tracker = Tracker::new(self_guid);
        tracker.observe_update(UpdateObject {
            objects: vec![
                create(self_guid, ObjectType::Player, Some(100)),
                create(unit_guid, ObjectType::Unit, Some(50)),
            ],
        });
        tracker.observe_update(UpdateObject {
            objects: vec![
                ObjectUpdate::Values {
                    guid: self_guid,
                    fields: fields(ObjectType::Player, Some(90)),
                },
                ObjectUpdate::Movement {
                    guid: self_guid,
                    movement: movement(9.0),
                },
                ObjectUpdate::OutOfRange(vec![unit_guid]),
            ],
        });
        tracker.observe_destroy(DestroyObject {
            guid: self_guid,
            on_death: false,
        });

        assert_eq!(
            tracker.counts,
            Counts {
                create: 2,
                values: 1,
                movement: 1,
                out_of_range: 1,
                destroy: 1,
            }
        );
        assert!(tracker.objects.is_empty());
        assert!(tracker.object_types.is_empty());
    }

    #[test]
    fn synthetic_acceptance_requires_self_create_and_nearby_unit() {
        let self_guid = 7;
        let unit_guid = 0xf130_0000_0000_0008;
        let mut tracker = Tracker::new(self_guid);
        assert!(tracker.acceptance().is_err());

        tracker.observe_update(UpdateObject {
            objects: vec![create(self_guid, ObjectType::Player, Some(100))],
        });
        assert!(tracker.acceptance().is_err());

        tracker.observe_update(UpdateObject {
            objects: vec![create(unit_guid, ObjectType::Unit, Some(50))],
        });
        assert!(tracker.acceptance().is_ok());
    }

    #[test]
    fn synthetic_item_type_is_retained_for_values_resolver() {
        let item_guid = 0x4000_0000_0000_0009;
        let mut tracker = Tracker::new(7);
        tracker.observe_update(UpdateObject {
            objects: vec![create(item_guid, ObjectType::Container, None)],
        });
        assert_eq!(tracker.object_type(item_guid), Some(ObjectType::Container));
    }

    #[test]
    fn movement_sequence_is_bounded_and_always_stops_each_leg() {
        let origin = safe_movement().position;
        let sequence = movement_sequence(origin).unwrap();
        assert_eq!(
            sequence.map(|step| step.opcode),
            [
                MSG_MOVE_START_FORWARD,
                MSG_MOVE_HEARTBEAT,
                MSG_MOVE_STOP,
                MSG_MOVE_START_BACKWARD,
                MSG_MOVE_HEARTBEAT,
                MSG_MOVE_STOP,
            ]
        );
        assert!(distance_3d(origin, sequence[1].position) <= MAX_DISPLACEMENT);
        assert!(
            distance_3d(origin, sequence[1].position)
                + distance_3d(sequence[1].position, sequence[4].position)
                <= MAX_TOTAL_PATH
        );
        assert!(distance_3d(origin, sequence[5].position) < FORWARD_DISTANCE);
        assert_eq!(sequence[2].flags & MOVEMENT_FLAG_FORWARD, 0);
        assert_eq!(sequence[5].flags & MOVEMENT_FLAG_BACKWARD, 0);
    }

    #[test]
    fn movement_gate_rejects_unsafe_flags_combat_gm_and_invalid_speeds() {
        assert!(validate_movement_gate(&safe_self()).is_ok());

        for flag in [
            MOVEMENT_FLAG_ON_TRANSPORT,
            MOVEMENT_FLAG_FALLING,
            MOVEMENT_FLAG_SWIMMING,
            MOVEMENT_FLAG_SPLINE_ENABLED,
            MOVEMENT_FLAG_ROOT,
        ] {
            let mut own = safe_self();
            own.living.as_mut().unwrap().flags = flag;
            assert!(validate_movement_gate(&own).is_err(), "accepted {flag:#x}");
        }

        let mut combat = safe_self();
        combat.unit_flags = Some(UNIT_FLAG_IN_COMBAT);
        assert!(validate_movement_gate(&combat).is_err());
        let mut gm = safe_self();
        gm.player_flags = Some(PLAYER_FLAG_GM);
        assert!(validate_movement_gate(&gm).is_err());
        let mut stopped = safe_self();
        stopped.speeds = Some([0.0; 9]);
        assert!(validate_movement_gate(&stopped).is_err());
    }

    #[test]
    fn movement_timeout_policy_accepts_only_two_retryable_timeouts() {
        let timeout = anyhow!(io::Error::new(io::ErrorKind::TimedOut, "deadline"));
        let would_block = anyhow!(io::Error::new(io::ErrorKind::WouldBlock, "deadline"));
        let poisoned = anyhow!("Wrath world reader is unusable after a partial frame read failed");

        assert!(retryable_timeout_pair(&timeout, &would_block));
        assert!(!retryable_timeout_pair(&timeout, &poisoned));
        assert!(!retryable_timeout_pair(&poisoned, &would_block));
    }

    #[test]
    fn expired_overall_deadline_refuses_the_next_movement_leg() {
        let expired = Instant::now() - Duration::from_millis(1);
        assert!(require_movement_time(expired).is_err());
    }

    #[test]
    fn monster_move_only_blocks_the_self_mover() {
        let self_guid = 0xf130_0000_0000_0008;
        let nearby_guid = 0xf130_0000_0000_0009;
        let mut self_body = Vec::new();
        let mut nearby_body = Vec::new();
        wrathplus_protocol::wire::write_packed_guid(self_guid, &mut self_body).unwrap();
        wrathplus_protocol::wire::write_packed_guid(nearby_guid, &mut nearby_body).unwrap();

        assert_eq!(monster_move_guid(&self_body).unwrap(), self_guid);
        assert_ne!(monster_move_guid(&nearby_body).unwrap(), self_guid);
        assert!(monster_move_guid(&[]).is_err());
    }

    #[test]
    fn persisted_position_requires_the_final_stop_within_tolerance() {
        let origin = safe_movement().position;
        let final_stop = offset_position(origin, FORWARD_DISTANCE - RETURN_DISTANCE);

        assert!(!persisted_position_matches(0, origin, 0, final_stop));
        assert!(distance_3d(origin, final_stop) > POSITION_TOLERANCE);
        assert!(persisted_position_matches(0, final_stop, 0, final_stop));
        assert!(!persisted_position_matches(1, final_stop, 0, final_stop));
    }
}
