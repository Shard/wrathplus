# Dev-only Wrath gameplay packet capture

Set `WOW_WRATH_PACKET_RECORD=/absolute/path/new-session.azbwrpk` before starting the client.
The default is off. The manager alone runs live checks; worker bots must not launch the client.
Use a new filename for each connection: existing files are never overwritten or appended to.

The recorder attaches to the inbound gameplay reader after world authentication and character
selection, before event adaptation. It records decrypted SMSG bodies (including compressed update
bodies unchanged), not socket headers or outbound packets. Auth challenge/session/response and
Warden opcodes are also explicitly excluded. It has no access to passwords, SRP state or session
keys. Character roster packets read during selection are outside this recorder.

Files contain private gameplay data, including character names, GUIDs and chat. Keep them local,
review/redact before sharing, and **never commit captures**. New files have Unix mode `0600`.
Recording stops at 256 MiB or on an I/O failure, with a warning; gameplay continues. No directory
is created automatically. A stopped capture is not reopened during that connection. An interrupted
write may leave an incomplete final record; replay should reject/ignore that tail, not invent a
packet. There is no automatic playback in this unit.

## Binary format (version 1)

All integers are little-endian. Header:

| Bytes | Value |
| --- | --- |
| 8 | ASCII `AZBWRPK1` |
| 4 | build `12340` (`34 30 00 00`) |

Then repeated records in receive order:

| Bytes | Value |
| --- | --- |
| 8 | elapsed microseconds since recorder opened (monotonic) |
| 2 | SMSG opcode |
| 4 | body byte length |
| length | raw decrypted body |

A replay fixture can feed each `(opcode, body)` directly to `WrathEventAdapter::dispatch`, then
send its session events through the real app handler drain. Elapsed times allow later timing-aware
replay without storing wall-clock timestamps or connection/auth metadata. No replay parser should
allocate from an unchecked length: cap bodies at the transport maximum (`0x7f_ffff - 2` bytes),
validate build/version, and reject truncated headers/bodies.

## Replay

Two halves, both offline and safe for any worker to run on a capture the manager recorded:

- `cargo run -p benilla-protocol --example replay_capture -- <capture.azbwrpk>` reads the capture
  strictly (`wrath_capture::read`; a cut-off final record is reported, never invented), replays it
  through the live reader's routing (`wrath_capture::census`: `SMSG_MULTIPLE_MOVES` split in place,
  the session loop's own opcodes through their decoders, movement relays, then
  `WrathEventAdapter`), and prints, cross-referenced with `WRATH-OPCODE-COVERAGE.md`: opcodes the
  table calls handled or capture-only that nothing decoded (a decode mismatch or regression; the
  adapter reports a malformed body for a known family as unhandled), decode failures, unhandled
  opcodes the table already lists as missing, a per-opcode table (route, seen, handled, unhandled,
  failed, events) and the session-event kinds produced. It exits 1 when there is a finding.
- `WOW_REPLAY_CAPTURE=<capture.azbwrpk> cargo test -p benilla-app --lib capture_replay -- --ignored
  --nocapture --test-threads 1` pushes the same events, one at a time, through the real app handler
  drain on the headless client, and reports each handler panic with its event kind and the event
  kinds no handler owns (dropped silently live). The headless client runs no schedule, so
  per-frame feeds are not exercised.

Not modelled: the movement authority's state (barriers, the unsupported-movement guard, seats);
a relay that fails to decode is listed as a failure, which live is a drop for another unit and
fatal only for our own mover.
