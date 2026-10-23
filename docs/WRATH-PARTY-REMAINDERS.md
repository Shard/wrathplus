# Group/raid remainders

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `Group.cpp` (`ChangeLeader`,
`Disband`), `MiscPackets.cpp` (`MinimapPing`, `RandomRoll`), `PlayerUpdates.cpp`
(`SMSG_RAID_GROUP_ONLY`), `GroupHandler.cpp`. No client-only behaviour is inferred; no Ghidra round.
Codec: `crates/wrathplus-protocol/src/wrath_party.rs`. Invite/roster/stats/raid-target/ready-check
and loot-roll handling was already native and is unchanged; instance rows are the instance family's.

Replies that mean what their 1.12 counterparts mean feed the shared events their feeds own
(**handled**): 0079 leader changed (name), 007C group destroyed (empty), 01D5 minimap ping
(GUID, x, y), 01FB random roll (min, max, roll, GUID), 0286 raid-group-only homebind warning.

Outbound (routes need only the live writer; no leadership or permission is claimed):
- Existing commands now with native writers: 0075 uninvite by name, 0078 set leader, 01D5 minimap
  ping, 01FB random roll.
- `ClientCommand::NativeParty`: 0076 uninvite by GUID with a reason, from 12340's
  `UninviteUnit(unitOrName [, reason])` (`0x51a7a0` → `0x6d43c0`; the stock kick popups and
  `/kick`), and 03C6 ready check finished, the initiator's close when its own check's 30 s pass
  (`0x573390`, `A/ui_party/ready_close.rs`).

Not actionable: 0071 group cancel, 0299 chat not in party, 0397 real group update, 0408 ready-check
error, 0411 action throttled, 0469 dynamic drop roll result, 03F6 squelch echo (no AC sender);
0070 group cancel, 03AE ready-check confirm (C→S), 03DD/03DE/03DF party silence/squelch,
0508/0509 low-level raid (AC `Handle_NULL`).

The instance family's coverage rows (`WRATH-INSTANCE-CONTROL.md`) were stale and now read
capture-only / transport-only as that document states.
