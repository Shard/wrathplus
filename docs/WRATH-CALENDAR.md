# Calendar family

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `CalendarHandler.cpp`
(`HandleCalendarGetCalendar`, every handler's read order, `SendCalendarRaidLockout*`),
`CalendarMgr.cpp` (`SendCalendar*`), `CalendarPackets.cpp`, `Guild.cpp` and `ArenaTeam.cpp`
(`MassInviteToEvent`). No client-only behaviour is inferred; no Ghidra round. Codec:
`crates/benilla-protocol/src/wrath_calendar.rs`.

All receive rows are **capture-only** (`NativeCalendarPackets` FIFO, sole owner, cleared on
Connected and Disconnected, duplicates kept): 0436 calendar (invites, events, server and zone time,
permanent lockouts, raid reset periods, holidays with 26 dates, 10 durations, 10 flags and the
texture), 0437 event with invitees, 0438/0439 guild-filter and arena-team member lists, 043A..043D
invite, invite removed, status and command result (the three name-carrying errors keep their
name), 043E/043F/0471 raid lockout added, removed and updated, 0440/0441/0443/0444/0445 alerts,
0448 pending count, 04BB clear pending action. Packed times stay packed; raw bytes and flags are
kept; counts are bounded by the body. No calendar, lockout, holiday or pending state, and no Lua
events, are produced.

Outbound transport-only `ClientCommand::NativeCalendar` (no UI producer): 0429 get calendar,
042A get event, 042B guild filter, 042C arena team, 042D add event (the invite list is omitted for
a guild announcement, as AC reads none), 042E update, 042F remove (AC reads only the event id),
0430 copy, 0431 invite, 0432 RSVP, 0433 remove invite, 0434 status, 0435 moderator status,
0446 complain, 0447 pending count, 04BA sign up.

Not decoded: 0442 status alert and 0460/0461 invite notes (AC never sends them), 045F invite notes
(AC `Handle_NULL`). The instance family's 0292 saved-instance extension is unchanged.
