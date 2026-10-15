# Wrath channel display and roster contract

Scope: build 12340; native client SHA256
`c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c`.
The installed ChannelFrame.lua is the consumer. No Vanilla channel-slot ABI is
substituted for the Wrath display-list ABI.

## Native indices and queries

- `GetChannelRosterInfo` (`0x504f30`) requires two numeric arguments (including
  numeric strings), converts by truncation, and uses one-based **display rows**.
  Invalid/unready rows return zero values. Valid rows return name and numeric
  `1`/nil for flags `0x01`, `0x02`, `0x20`, `0x40`, `0x80` respectively.
  `0x08` text mute is not the fourth return. Empty resolved names use `UNKNOWN`.
- `GetNumChannelMembers` (`0x505190`) takes a numeric display row; invalid,
  unready or empty rosters return one nil, otherwise one member count.
- Resolver `0x4fe1c0` distinguishes server type 0, synthetic types 1/2/3
  (battleground/party/raid), and invalid/header type 4. Headers occupy rows.
- Builder `0x504840` preserves native slot order inside world/custom categories;
  collapsing a category changes display indices, not the selected identity.
  Group/world/custom headers are separate from joined-channel slot numbers.
- Selection `0x504490` clears the selected cache and sends
  `CMSG_SET_CHANNEL_WATCH` (`0x3ef`); request helper `0x4fe530` sends
  `CMSG_CHANNEL_DISPLAY_LIST` (`0x3d2`). Neither is ordinary list `0x9a`.
- Synthetic party flags are member 4 / leader 5; raid flags are member 4 /
  assistant 6 / leader 7. Native voice augmentation has no current app resource;
  do not derive it from text-ignore names.

## Receive and name lifecycle

`SMSG_CHANNEL_LIST` (`0x9b`), native handler `0x505dc0`, reads a leading byte,
channel C-string, channel flags byte, count u32, then (GUID u64, flags u8) rows.
Zero leading byte belongs to ordinary chat-list handling; nonzero belongs to
selected-display handling (`0x503990`). Preserve it losslessly.

Only a response matching the selected server channel is accepted. Hold getters
until **all** names have answers, then sort case-insensitively by name and emit
`CHANNEL_COUNT_UPDATE` and `CHANNEL_ROSTER_UPDATE`. `None` means pending;
`Some("")` means answered-negative and permits the `UNKNOWN` fallback. Response
revisions reject stale name callbacks. Selection change, leave, and disconnect
must not replay old members.

## Joined-channel metadata provenance

Do not infer native confirmed state from Vanilla `SlotState::Joined`: it merges
native confirmed 0 and join-sent 1. Do not infer fixed IDs from names/catalogs.

- Allocator `0x5076b0` initializes `+0x9c = 1`, `+0xa0 = 0`.
- Join confirmation `0x507880` sets `+0x9c = 0`; it **does not reset +0xa0**.
- Removal `0x507a30` clears slot/name/fixed ID and sets both states to 1.
- Explicit local leave `0x5093f0` sends `CMSG_LEAVE_CHANNEL` (`0x98`) then sets
  `+0xa0 = 1` before acknowledgment. A confirmed custom row hides immediately;
  a pending record may also be cleared locally. This is not a rule for automatic
  zone/recruitment leaves.
- Notify handler `0x50e120` notice 2 reads flags u8, fixed ID u32, reserved u32
  and optional trailing string, then confirms the matching allocated slot.
  Notice 3 reads fixed ID u32 and `is_constant` u8. An existing row with
  `+0xa0 == 0` and nonzero trailing byte is suspended/retained inactive;
  otherwise it is removed. Unknown suspension must not fabricate a row.

The independent app ledger associates raw notifications with extant slots after
shared lifecycle processing. Initial allocated/confirmed custom state 0 is proven;
explicit leave changes it to 1, and later confirmation must preserve that state.
Omit unconfirmed allocations until their native source is represented. The UI
metadata seam retains raw `custom_state`: custom 1 hides, custom 0 is active;
fixed-channel activity follows native confirmed state independently.

## Ownership and public seam

- Wire owns lossless `WrathChannelList` and `WrathChannelNotify` FIFOs, handlers,
  initialization/disconnect clearing, and both outgoing opcodes.
- UI owns display model, selection/getters and these `UiScript` methods:
  `set_wrath_display_channels`, `receive_wrath_channel_roster`,
  `resolve_wrath_channel_roster_name`, `reset_wrath_channel_roster`, and
  `take_wrath_channel_roster_requests` (`Watch` / `DisplayList`).
- App Migration consumes each FIFO once, manages name queries/revisions, drains
  requests and projects native membership plus explicit leave intent. Raw roster
  flags are preserved; absent voice resources are not fabricated.

The UI seam alone is not end-to-end acceptance. Integrated app tests and the
installed headless FrameXML census are required; census acceptance means zero
hard failures, not merely a successful Rust test exit status.
