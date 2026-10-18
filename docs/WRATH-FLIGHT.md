# Flight (movement M5a): native behaviour

One read-only Ghidra round on the installed build-12340 `Wow.exe` (headless 12.0.4, project opened
`-readOnly -noanalysis`, scripts and logs under `/tmp/azb-wire-ghidra`, nothing written to the
project). Addresses are build 12340. Wire layouts are AzerothCore's (`MovementHandler.cpp`,
`Unit::SetCanFly`); this file records only the client-side behaviour the wire does not show.

The mover's flags word is `CMovement+0x44`, `flags2` is `+0x48`.

**Movement-event drain `0x6ef860`** (one switch arm per queued movement event; the opcode is what it
sends):

| Event | Call | Effect | Sends |
|---|---|---|---|
| 6 | `0x9898e0(1)` | ascend | `MSG_MOVE_START_ASCEND` 0x359 |
| 7 | `0x9898e0(0)` | descend | `MSG_MOVE_START_DESCEND` 0x3A7 |
| 8 | `0x989940` | stop ascend/descend | `MSG_MOVE_STOP_ASCEND` 0x35A |
| 0x2d | `0x6ebc50` → `0x989890` | start flying | `CMSG_MOVE_SET_FLY` 0x346 (flags, flags2) |
| 0x2e | `0x98c240` | stop flying | `CMSG_MOVE_SET_FLY` 0x346 |
| 0x2f | `flags \|= 0x0100_0000` | CAN_FLY granted | `CMSG_MOVE_SET_CAN_FLY_ACK` 0x345 |
| 0x30 | `0x98c240` if FLYING, then `flags &= !0x0100_0000` | CAN_FLY revoked | 0x345 |

**`0x9898e0` (ascend/descend).** Only while SWIMMING or FLYING (`flags & 0x0220_0000`) and `flags2`
bit 0 clear. Sets ASCENDING `0x0040_0000` (ascend) or DESCENDING `0x0080_0000` (descend). Ascend is
the jump key in water as well as in the air.

**`0x989940` (stop).** Only while SWIMMING or FLYING. Clears both (`& 0xff3f_ffff`).

**`0x989890` (start flying).** Requires CAN_FLY (`0x0100_0000`) and not PENDING_ROOT (`0x0010_0000`).
`flags = flags & 0xfbdf_cfff | 0x0200_0000`: clears SPLINE_ELEVATION, SWIMMING, FALLING and FALLING_FAR
and sets FLYING. The caller (`0x6ebc50`) then resolves pending start/stop bits (`0x6eb3b0`), and the
drain sends `CMSG_MOVE_SET_FLY`.

**`0x98c240` (stop flying / land).** `flags &= 0xfd3f_ff3f`: clears FLYING, ASCENDING, DESCENDING and
the pitch keys (`0xc0`). Zeroes the pitch (`+0x24`, `+0x5c`) and re-seeds the mover's motion. The
drain sends `CMSG_MOVE_SET_FLY` with FLYING clear.

**`0x9883f0` (jump).** Refused while FLYING, ROOT or FALLING (`flags & 0x0200_1800`), and while
hovering (`0x4000_0000`) unless swimming. Swimming uses the swim jump speed (`0xaa33e0`, else
`0xaa33dc`).

**Handlers that compare the opcodes** (`0x740d30`, observed-mover replay): 0x359 and 0x3A7 replay
the ascend and descend (`0x6ef5c0(.., 1/0)`), 0x35A replays the stop (`0x6ef630`), 0x3AD the can-fly
update (`0x6ef0d0`) and 0x4D2 the gravity change (`0x6ee700`).

## What the client does (M5a)

**Writer.** `wrath_ground` admits `MOVE_CAN_FLY`, `MOVE_FLYING`, `MOVE_ASCENDING` and
`MOVE_DESCENDING`:
- the pitch rides exactly while SWIMMING or FLYING;
- FLYING needs CAN_FLY and never sits with SWIMMING;
- neither medium carries FALLING or a jump tail;
- ascend and descend need a medium and never come together.

Each edge opcode checks its bit: `START_ASCEND` needs ASCENDING, `START_DESCEND` needs DESCENDING,
and `STOP_ASCEND` must leave neither key set and keep the medium. `CMSG_MOVE_SET_FLY` is an
ordinary movement packet (AzerothCore `HandleMovementOpcodes`).

**Authority.**
- A root keeps the medium (SWIMMING, FLYING, ON_TRANSPORT, with their tails) and the granted modes
  (CAN_FLY, water walking, slow fall). It drops the keys and the arc.
- The can-fly grant's ACK sets the bit.
- Both revokes (the mover-order path and the dedicated `SMSG_MOVE_UNSET_CAN_FLY` path) land a
  flier first (`landed()`), then clear the bit.
- A knockback launches a flier from the landed pose.

The flight and pitch-rate orders now reach the controller as `ForceSpeedChange`, and the can-fly
grant as `MoveMode::CanFly`. Revoking can-fly reaches it as `MoveMode::CanFly` off.

**Controller** (`player/flight.rs`).
- **Take-off.** The ascend key held while airborne with CAN_FLY, not rooted and not swimming.
  This is an *inference*: the enqueuer of drain event `0x2d` was not traced within the one Ghidra
  round, but it gives the stock "jump, hold Space" take-off.
- **Flying.** The swim resolver with no waterline: pitched forward, level strafe, and the ascend
  and descend keys straight up and down. Speeds are the server's flight/flight-back, with stock
  7.0/4.5 as the fallback.
- **Landing.** Ground contact while not rising, entering water, a knockback, or losing the grant.
  Losing the grant lands inside the ACK, so no `SET_FLY` follows, matching event 0x30.
- **Keys.** The flier's jump is refused (`0x9883f0`) and its sit key descends.
- **Wire.** `SET_FLY` goes out on the FLYING edge, then `START_ASCEND`/`START_DESCEND` on a key
  press and `STOP_ASCEND` on its release while still flying.
- **Internal bit.** The controller keeps FLYING in an internal bit (`0x0800_0000`), because
  Wrath's `0x0200_0000` is the shared word's ON_TRANSPORT. It is mapped at the command boundary.

**Bindings.** `JumpOrAscendStart`, `AscendStop`, `SitStandOrDescendStart` and `DescendStop` are
native now; they replace the Lua shims that aliased them to `Jump`/`SitOrStand`. Each Start fires
the ground verb and holds the vertical key, and the engine picks by state.

**Observers.** `MSG_MOVE_SET_FLIGHT_SPEED`, `_FLIGHT_BACK_SPEED` and `_PITCH_RATE` (relays), and
their `SMSG_SPLINE_SET_*` forms, decode into `SpeedChanged` (`wrath_speed`).

**Not modelled (labelled):**
- The swimmer's ascend and descend keys. `0x9898e0` accepts SWIMMING too, but the swim jump
  stays the 1.12 breach.
- Taking off from the water.
- A flier's remote extrapolation. The relay still drops observers' FLYING, because it collides
  with the shared ON_TRANSPORT.
- Flying animation selection.
