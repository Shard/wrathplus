# Proposed L2 queue, role check, proposal and boot boundary

Status: **AGREED** by App Migration and Native UI; awaiting the manager's approval. It
covers the plan's L2 (queue and role check) with the parts of L3 (proposal) and L4 (boot,
completion, continue, teleport, disabled) that ride the same native handler. Evidence: one
headless Ghidra round on the pinned Wow.exe (`/tmp/azb-app-mig/ghidra/lfg-l2-*.log`,
`lfg-msgs.log`, `lfg-joinres.log`, `lfg-backfill*.log`); no executable bytes are committed.

## Native handler

`0x0055bdc0` is the one LFG message handler (registered by `0x0055d440`). It stores each
packet's fields in their own globals, prints a system line through the message table
`0xac8af0` (`0x5216f0(id)`; kind 0 is a chat system line, kind 2 a UI error), may play a
sound (`0x4c8fc0(id)`), then fires events through `0x81b530`. Event ids (`0xc24eb0` table):
`0x1f5 LFG_UPDATE`, `0x1f6 LFG_PROPOSAL_UPDATE`, `0x1f7 LFG_PROPOSAL_SHOW`,
`0x1f8 LFG_PROPOSAL_FAILED`, `0x1f9 LFG_PROPOSAL_SUCCEEDED`, `0x1fa LFG_ROLE_UPDATE`,
`0x1fb LFG_ROLE_CHECK_UPDATE`, `0x1fc LFG_ROLE_CHECK_SHOW`, `0x1fd LFG_ROLE_CHECK_HIDE`,
`0x1fe LFG_ROLE_CHECK_ROLE_CHOSEN`, `0x1ff LFG_QUEUE_STATUS_UPDATE`,
`0x200 LFG_BOOT_PROPOSAL_UPDATE`, `0x203 LFG_OFFER_CONTINUE`, `0x205 LFG_COMPLETION_REWARD`.

Per packet, in order:

| Packet | Native effect |
| --- | --- |
| `0x365` QUEUE_STATUS | Stores dungeon, five waits, three needs, and `queued_at = now_ms − queued_s·1000`; three zero needs read as one damager needed. Fires `LFG_QUEUE_STATUS_UPDATE`. |
| `0x2BB` ROLE_CHOSEN | The member's name; roles 0 prints `ERR_LFG_PLAYER_DECLINED_ROLE_CHECK` (`0x2bf`, name), else fires `LFG_ROLE_CHECK_ROLE_CHOSEN(name, tank, healer, damage)` (role bits 1, 2, 3). |
| `0x363` ROLE_CHECK_UPDATE | Stores state, dungeons and members (guid, ready, roles, level). Beginning: prints `ERR_LFG_ROLE_CHECK_INITIATED`, sound `0x43a6`, and one `ROLE_CHOSEN` per ready member. Then `LFG_ROLE_CHECK_UPDATE`; our own row unready in state 2 fires `LFG_ROLE_CHECK_SHOW`, else `LFG_ROLE_CHECK_HIDE` (state 5 also prints `ERR_LFG_ROLE_CHECK_ABORTED`, states 5 and 6 sound `0x43bd`). |
| `0x364` JOIN_RESULT | Result 6 stores the party-lock block; result 1 with reason 3 or 4 prints the timeout or not-viable failure; otherwise the 17-row table `0xacdda8` maps result → `ERR_LFG_*` (`1` role check failed … `17` can't use dungeons). |
| `0x361` PROPOSAL_UPDATE | Stores entry, state, id, mask, silent and up to five anonymous rows. Fires `LFG_PROPOSAL_UPDATE`/`_SHOW` (`0x552900`). A party member's decline clears the proposal, prints `ERR_LFG_PROPOSAL_DECLINED_PARTY`, sound, `LFG_PROPOSAL_FAILED`; our own decline prints `_DECLINED_SELF`, sound, `LFG_PROPOSAL_FAILED`. |
| `0x367`/`0x368` UPDATE_PLAYER/PARTY | Rebuild the joined/queued/dungeon state and comment. With a proposal up and the queue left or entered: drop it, print `ERR_LFG_PROPOSAL_FAILED` when joined, fire `LFG_PROPOSAL_SUCCEEDED` (update type 10, party also 1 and 16) else `_FAILED`. Joining prints `ERR_LFG_JOINED_QUEUE`/`_LIST`, leaving on update types 7 or 15 `ERR_LFG_LEFT_QUEUE`/`_LIST`, with sounds. Then `LFG_UPDATE`. |
| `0x36D` BOOT_PROPOSAL_UPDATE | Stores the vote; a finished vote prints `ERR_PARTY_LFG_BOOT_VOTE_SUCCEEDED`/`_FAILED` (agrees below needed) with the victim's name. Fires `LFG_BOOT_PROPOSAL_UPDATE`. |
| `0x1FF` PLAYER_REWARD | Stores the completion reward; XP and money lines; fires `LFG_COMPLETION_REWARD`. |
| `0x293` OFFER_CONTINUE | Fires `LFG_OFFER_CONTINUE(name, lfgID, typeID)` for the dungeon. |
| `0x200` TELEPORT_DENIED | Prints the reason's line (table `0x55d148`), above 7 `ERR_CLIENT_LOCKED_OUT`. |
| `0x398` DISABLED | Clears every LFG store, `LFG_UPDATE`, prints `ERR_SYSTEM_DISABLED`. |

`PartyLFGStartBackfill` (`0x55a0c0`) clears the selection, selects the party's current dungeon
and sends the ordinary join (`0x5572d0`).

## Proposed seam (agreed by App Migration and Native UI)

App owns the sole reducer of `NativeLfgPackets` for these packets (L1's lock packets stay as
they are) and publishes one immutable snapshot **before** each packet's events, in wire order.
Native UI owns the DTO (`benilla-ui/src/script/wrath_lfg/queue.rs`) and its setter, which only
stores (a DTO setter is not a receipt, as in L1). Raw wire fields; Native UI derives the Lua
shapes.

```text
WrathLfgQueueSnapshot {
  player: Option<WrathLfgUpdate>, party: Option<WrathLfgUpdate>,   // 0x367 / 0x368
  queue: Option<WrathLfgQueueStatus>,                              // 0x365
  role_check: Option<WrathLfgRoleCheck>,                           // 0x363 (+ 0x2BB)
  proposal: Option<WrathLfgProposal>,                              // 0x361, None once dropped
  boot: Option<WrathLfgBoot>,                                      // 0x36D
  completion: Option<WrathLfgCompletion>,                          // 0x1FF
  disabled: bool,                                                  // 0x398
}
WrathLfgUpdate { update_type, joined (extra present), queued, the two raw bytes
  (GetLFGInfoServer's noPartialClear and achievements), dungeons (full-packed, wire order),
  comment }
WrathLfgQueueStatus { dungeon (full-packed), waits avg/my/tank/healer/dps (signed seconds,
  -1 kept), needs tank/healer/dps, queued_at (GetTime clock) }
  - App stores the needs as native does: three zeros read one damager (0x55bdc0 writes it).
WrathLfgRoleCheck { state, is_beginning, dungeons, members in wire order
  { guid, ready, roles, level, name: Option<String> (App resolves from the name cache) } }
WrathLfgProposal { dungeon, state, id, encounter_mask, silent, rows in wire order
  { roles, is_self, in_dungeon, same_group, answered, accepted } }
WrathLfgBoot { in_progress, did_vote, my_vote, victim, victim_name: Option<String>, votes,
  boot_votes, needed, time_left, captured_at (GetTime clock), reason }
WrathLfgCompletion { random_dungeon, dungeon, done, money, xp, the raw ints, items
  { item, display, count } in order }
UiScript::set_wrath_lfg_queue(Arc<WrathLfgQueueSnapshot>)
```

`IsPartyLFG`/`IsInLFGDungeon` read the group, not this handler: Native UI adds
`lfg_party: bool` and `lfg_dungeon_entry: Option<u32>` to `WrathGroup`; App fills them from
`SMSG_GROUP_LIST`'s LFG block (group type `0x08`, `Roster::lfg` state and entry).

App fires the native events with their arguments and prints the native system lines (the
existing UI error/system-line path) after each publish. Native UI owns the getters
(`GetLFGMode`, `GetLFGQueueStats`, `GetLFGInfoServer`, `GetLFGQueuedList`,
`GetLFGRoleUpdate*`, `GetLFGProposal*`, `GetLFGProposalEncounter` with `DungeonEncounter.dbc`,
`GetLFGBootProposal`, `GetLFGCompletionReward*`, `IsPartyLFG`, `IsInLFGDungeon`) and the
request outbox, which App sends through the existing `LfgCommand` writers:

- `Join { roles, dungeons (full-packed, L0 selection order), needs/raw bytes, comment }`, built
  by Native UI from its L0 state (`JoinLFG()` takes no Lua arguments);
- `Leave`; `RoleCheckAnswer { roles: u8 }` (`CompleteLFGRoleCheck(bool)`: CMSG `0x36A` with the
  stored roles, 0 declining; `SetLFGRoles` stays the L0 preference setter);
- `Proposal { id, accept }`, the id filled by Native UI from the snapshot (published before
  the events, so no race);
- `Teleport { out: bool }`; `SetComment { text }`.

Boot votes and backfill already ride StaticPopup's outbox. No optimistic state from a send.
Resets: socket teardown clears the snapshot; `LFG_DISABLED` clears it natively. ReloadUI seeds
the current snapshot and replays no events.
