# Movement authority orders (movement/teleport M1)

Wire reference: AzerothCore (`../azerothcore-playerbots/core/src/server/game`):
`Unit::SendSpeedToController`, `SetWaterWalking`, `SetFeatherFall`, `SetHover`, `SetCanFly`,
`SetDisableGravity`, `KnockbackFrom`, `SetCollisionHeight`, `SendTeleportPacket` (Unit.cpp),
`Player::SendTeleportAckPacket` (Player.cpp); readers `HandleForceSpeedChangeAck`,
`HandleMoveKnockBackAck`, `HandleMoveTeleportAck` (MovementHandler.cpp) and
`HandleMoveFlagChangeOpcode` (MiscHandler.cpp); `MovementFlags` (UnitDefines.h). No client-only
behaviour is inferred; no Ghidra round. Codecs: `crates/benilla-protocol/src/wrath_authority.rs`.

**Before this unit** every one of these self orders was fatal in the Wrath reader
(`mandatory_movement_order` revoked authority and ended the session): any forced speed change
(mounting, sprint, a slow), water walk, feather fall, hover, can-fly, gravity, knockback, collision
height, and the observer `MSG_MOVE_TELEPORT`, so another player blinking nearby dropped us. A
relay of another unit's movement outside the on-foot subset (swimming, flying) also ended the
session. Near teleports (`MSG_MOVE_TELEPORT_ACK`) were dropped, freezing movement server-side.

**Design.** The established root/unset-can-fly path is extended, not replaced. The reader decodes
an order strictly and raises it in the `MovementAuthority` pending-order FIFO, with the same
strict counter advance and foreign-mover refusal. Ordinary movement stays held while any order
is pending, so nothing overtakes it. All orders share AzerothCore's one session order counter.
- Speeds, modes, knockback and collision height: the writer acknowledges the head order from the
  last pose reported to the server (`send_mover_ack`), as for root. It sends the speed or height
  verbatim (AzerothCore resets or kicks a mismatch), sets or clears the mode flag with
  `u32 applied` (gravity ACKs have none), and sends the knockback as its launch frame (FALLING,
  zero fall time, the received down-positive tail). The controller still gets the shared event
  where it models the state (`ForceSpeedChange` for the 1.12 speeds, `MoveMode` for water walk,
  feather fall and hover, `KnockBack`). Its own ACK commands are no-ops on this route
  (`WrathRoute::AuthorityAcked`).
- Near teleport: the controller snaps on `Teleport` and sends `TeleportAck`. The writer sends
  `MSG_MOVE_TELEPORT_ACK` (`packed guid, u32 counter, u32 time`) only while that teleport is the
  head order (`WrathRoute::NativeTeleportAck`), then reports from the destination. No heartbeat
  from the old pose can reach the server after the ACK.
- Water walking (`0x1000_0000`) and slow fall (`0x2000_0000`), the same bits as 1.12, are now
  admitted on-foot flags, so the controller keeps reporting a granted mode.

**Not modelled by the controller** (acknowledged, no event): flight and flight-back speeds, pitch
rate, can-fly, gravity, collision height. Flight and hover remain fenced by `WrathGroundLimit`,
which holds the mover rather than sending unsupported movement.

**Observer relays.** `MSG_MOVE_TELEPORT` joins the on-foot relay whitelist as
`RelayVerb::Teleport`. A relay for a foreign mover that falls outside the subset is dropped as
`PacketDropped`; our own mover's relays stay strict.

**No order is fatal for being unproven** (manager rule): the one undecodable pair,
`SMSG_MOVE_SET/UNSET_CAN_TRANSITION_BETWEEN_SWIM_AND_FLY` (registry only, never sent), is logged and
ignored, since nothing is queued and the authority state is untouched. A malformed proven order still
fails closed: its counter may already be owed, so the FIFO would be uncertain. Regression:
`observer_teleport_tests` shows a foreign `00C5` never touches our authority state.

**Movement M2: swimming and speed echoes.** On Wrath the controller used to *hold* the player on
entering deep water (`WrathGroundLimit::Swimming`), because the outbound codec and the authority
pose were on-foot only.
- `wrath_ground::encode_movement` admits `SWIMMING` with its pitch (AzerothCore `ReadMovementInfo`
  reads pitch for SWIMMING or FLYING). Swimming requires a pitch and a pitch requires swimming; there
  is no airborne flag or fall tail in the water. `MSG_MOVE_START_SWIM` and `STOP_SWIM` carry the edge.
  `encode_on_foot` is the no-pitch wrapper.
- `OnFootPose` carries the pitch, so ACKs sent while swimming report it. A root keeps a still swimmer
  in the water (ROOT plus SWIMMING, no STOP). A knockback launch leaves the water (FALLING, no
  pitch).
- The controller no longer holds a swimmer (`WrathGroundLimit` keeps only Transport and Flight).
- Relays admit swimmers (SWIMMING, pitch, pitch-key bits) and the swim and pitch opcodes
  `00BF..00C1`, `00CA`, `00CB` and `00DB`.
- Observer speeds: `MSG_MOVE_SET_*_SPEED` (`packed guid + MovementInfo + f32`) and
  `SMSG_SPLINE_SET_*_SPEED` (`packed guid + f32`) for the six 1.12 kinds become `SpeedChanged`
  (`wrath_speed.rs`).

**Movement M3: observers, bundles, mount and time-skip.**
- Relays (`wrath_relay.rs`) now place flying units and passengers. Flight keeps the pitch and drops
  the Wrath-only CAN_FLY, FLYING, ASCENDING and DESCENDING bits. Wrath FLYING (`0x0200_0000`) is the
  shared ON_TRANSPORT bit, so it must never pass through. Wrath ON_TRANSPORT (`0x200`) becomes the
  shared bit with the deck pose. DISABLE_GRAVITY and HOVER share their 1.12 bits. Flyers are placed
  but not flight-animated. Spline elevation stays fenced.
- The mode, knockback and collision-height echoes relay their `MovementInfo`; the knockback and
  height extras are framed and dropped. `MSG_MOVE_TIME_SKIPPED` becomes `MoveTimeSkipped`.
- `SMSG_MULTIPLE_MOVES`: the reader splits it strictly and handles each record in order as the
  packet it names. The can-fly, water-walk, feather-fall and hover orders AzerothCore bundles at
  login now raise their barrier and are ACKed instead of being dropped. A malformed bundle fails
  closed.
- Mount special anim both ways; `CMSG_MOVE_TIME_SKIPPED` admitted; `SMSG_DISMOUNT` capture-only (the
  descriptor carries the mount).

**Not yet** (movement M4, controller-level): our own flight (flight physics; `WrathGroundLimit`
holds the flyer), our own transport movement (the authority pose and outbound transport tail),
vehicles (`046D`, `0476..0479`, `049B`, `04A8`, `04A9`, `04A7`, `049D`), `CMSG_MOVE_SET_FLY`,
`CMSG_MOVE_CHNG_TRANSPORT`, `CMSG_MOVE_NOT_ACTIVE_MOVER` (possession).
