# Control hand-over, vehicles, root and pitch keys (movement M5b)

The server-side layouts come from AzerothCore:
- `MovementHandler.cpp`: `HandleMovementOpcodes`, `HandleMoveNotActiveMover`, `HandleMoveRootAck`
  and `VerifyMovementInfo`;
- `VehicleHandler.cpp`;
- `Player::SetClientControl` / `SetMover` (`Player.cpp`);
- `Unit::SendMoveRoot` (`Unit.cpp`).

The client-side behaviour comes from one read-only Ghidra round on build 12340 `Wow.exe`. The
scripts and logs are under `/tmp/azb-wire-ghidra/m5b`. The movement-event drain `0x6ef860` was
already traced for M5a (`docs/WRATH-FLIGHT.md`).

## The active mover

AzerothCore keys every movement packet and ACK on `m_mover`. It drops a packet whose guid is not
`m_mover`. `SetClientControl(unit, allowMove)` sends `SMSG_CLIENT_CONTROL_UPDATE` and, when it
allows movement, makes `unit` the mover. That covers:
- a vehicle seat that controls the vehicle;
- possession;
- control returning to our body.

The authority (`net/io/wrath.rs`) now follows the same model:

- **Mover switch.**
  - `mover` is our own guid until a control update allows another unit. A grant parks our body's
    pose and root, then takes the unit's.
  - A refusal for the active mover holds it. A refusal for any other unit changes nothing.
  - The unit's pose comes from its create block (`observe_unit_create`; ROOT stripped, and an
    unreportable state is stood at its position) or from its relays. That pose ACKs its first
    orders: AzerothCore roots a `DISABLE_MOVE` vehicle right after the grant.
- **Orders.**
  - An order for the active mover or a parked unit is ACKed from that unit's own pose.
  - An order for a unit we never saw is logged and ignored, not fatal. Its counter still advances,
    and AzerothCore would refuse the ACK anyway.
- **Writer.** Movement is written under the mover the controller last claimed with
  `CMSG_SET_ACTIVE_MOVER`, and only while that is the server's mover. A frame still in flight from
  our body cannot relocate the vehicle.
- **Hand-over order** (`0x729010`). `CMSG_MOVE_NOT_ACTIVE_MOVER` (packed guid + the body's parting
  `MovementInfo`) goes out only when the old mover is the player. `CMSG_SET_ACTIVE_MOVER`
  (full guid) follows. The controller already sends them in this order (`yield_own_body`, then the
  claim).
- **Controller.**
  - Our body's modes are set aside while another unit is driven, and the driven unit's mode orders
    reach the controller (`death/net.rs` forwards them for `Player::foreign_mover`).
  - The body's modes return with control. A grant also lands a flier.

## Root

AzerothCore drops a rooted mover's packet that lacks ROOT (`0x800`) or carries any
`MOVEMENTFLAG_MASK_MOVING` bit: direction, pitch keys, falling, ascend or descend. Turning is
allowed.

- **Before:** the Wrath writer dropped every packet while rooted, and the controller was never told
  it was rooted. The avatar kept walking locally while the server held it, and turning in place
  was lost. The shared ROOT bit (`0x1000`) would also have gone out as Wrath FALLING.
- **Now:**
  - The reader emits `MoveMode::Root` for root and unroot, as 1.12 does, so the controller stops
    translating.
  - The writer sets ROOT from the root order state. A rooted moving packet is dropped. A free
    packet never carries ROOT.
  - The unroot ACK clears ROOT.
  - A root keeps ROOT, the medium and the granted modes (`halted`).
  - A knockback launch strips ROOT and the pitch keys.

## The vehicle we drive

`VehicleControl::{Dismiss, ChangeSeats}` → `WrathRoute::NativeVehicleControl`. The writer sends it
with the driven vehicle's guid and its halted pose; drain events 0x34/0x37 halt the mover through
`0x6e9980` first.
- `CMSG_DISMISS_CONTROLLED_VEHICLE` `0x046D`: packed guid + MovementInfo.
- `CMSG_CHANGE_SEATS_ON_CONTROLLED_VEHICLE` `0x049B`: packed vehicle guid + MovementInfo, then a
  packed accessory guid and an `i8` seat.

These are transport-only: the vehicle UI (`VehicleExit` and the rest) is still a Lua stub.

## Pitch keys

- **Native.** `0x989220` sets PITCH_UP `0x40` or PITCH_DOWN `0x80`, only while swimming or flying.
  `0x989450` clears both. The drain sends `MSG_MOVE_START_PITCH_UP` `0x00BF`,
  `MSG_MOVE_START_PITCH_DOWN` `0x00C0` and `MSG_MOVE_STOP_PITCH` `0x00C1`.
- **Here.**
  - `PitchUpStart/Stop` and `PitchDownStart/Stop` are now held inputs; they were inert.
  - The controller tilts `mover_pitch` at the server's pitch rate (π by default), clamped to ±π/2.
  - The key's bit rides the wire while in the medium, and the edges go out as above.

## Labelled, not built

The second Ghidra round (movement M6, `/tmp/azb-wire-ghidra/m6`) narrowed each of these. None is
proven far enough to emit:

- **`MSG_MOVE_SET_PITCH` `0x00DB`:**
  - Drain event 0x14 (`0x6e9380`) sends it whenever `0x989bc0` stores a pitch change above its
    threshold; a resting water walker is the exception.
  - The event is queued only by `0x6ee460(pitch)`, which clamps the pitch to the controlled
    vehicle seat's pitch range when the seat allows pitching.
  - Its caller `0x72ead0` (vehicle aim, mouse look, or both) was not traced.
- **`CMSG_MOVE_FALL_RESET` `0x02CA`:**
  - The generic movement sender (`0x71ef80`) sets `+0xa30 & 0x80` when it sends while FALLING and
    clears the bit otherwise.
  - `0x71f2c0` sends FALL_RESET if the bit is set, reached through `0x6e9270` from `0x7612b0`,
    which was not traced.
- **`CMSG_MOVE_CHNG_TRANSPORT` `0x038D`:**
  - `0x6f0c70` sends it when `CMovementData_C::ForceSetTransportInt` (`0x6ec400`) changes the active
    player's transport guid or seat.
  - Its callers (`0x70ffd0`, `0x710460`, `0x73c8e0`, `0x7576e0`, `0x760720`) re-seat or unseat the
    player from vehicle and transport handlers whose triggers were not traced.
- **`CMSG_ENABLETAXI` `0x0493`:**
  - The right-click unit handler (`0x6ddbb0`) sends it, with the unit's guid, before its NPC-flag
    dispatch.
  - It is sent only when an untraced unit field at `+0x94` equals 7.
  - AzerothCore handles it as `CMSG_TAXIQUERYAVAILABLENODES`, which the client sends.
- **`MSG_MOVE_WORLDPORT_ACK` S→C:** AzerothCore never constructs it. It moves to the out-of-scope
  "no AC sender" bucket.
- **Passengers.** The ON_TRANSPORT seat of a passenger who does not drive stays `-1`; AzerothCore
  overwrites it from its own vehicle state.
