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
