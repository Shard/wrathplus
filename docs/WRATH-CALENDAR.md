# Calendar family

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `CalendarHandler.cpp`
(`HandleCalendarGetCalendar`, every handler's read order, `SendCalendarRaidLockout*`),
`CalendarMgr.cpp` (`SendCalendar*`), `CalendarPackets.cpp`, `Guild.cpp` and `ArenaTeam.cpp`
(`MassInviteToEvent`). Codec: `crates/wrathplus-protocol/src/wrath_calendar.rs`.

Every receive row is decoded into the `NativeCalendarPackets` FIFO (cleared on Connected and
Disconnected, duplicates kept), which `ui_calendar` reduces (below): 0436 calendar (invites, events, server and zone time,
permanent lockouts, raid reset periods, holidays with 26 dates, 10 durations, 10 flags and the
texture), 0437 event with invitees, 0438/0439 guild-filter and arena-team member lists, 043A..043D
invite, invite removed, status and command result (the three name-carrying errors keep their
name), 043E/043F/0471 raid lockout added, removed and updated, 0440/0441/0443/0444/0445 alerts,
0448 pending count, 04BB clear pending action. Packed times stay packed; raw bytes and flags are
kept; counts are bounded by the body.

Outbound `ClientCommand::NativeCalendar`, produced by the VM's requests through `ui_calendar`:
0429 get calendar,
042A get event, 042B guild filter, 042C arena team, 042D add event (the invite list is omitted for
a guild announcement, as AC reads none), 042E update, 042F remove (AC reads only the event id),
0430 copy, 0431 invite, 0432 RSVP, 0433 remove invite, 0434 status, 0435 moderator status,
0446 complain, 0447 pending count, 04BA sign up.

Not decoded: 0442 status alert and 0460/0461 invite notes (AC never sends them), 045F invite notes
(AC `Handle_NULL`). The instance family's 0292 saved-instance extension is unchanged.

## App reducer and the Lua calendar

App (`crates/wrathplus-app/src/ui_calendar.rs`) is the sole reducer of `NativeCalendarPackets` and
the sender of the `NativeCalendar` writers; Native UI's VM side
(`crates/wrathplus-ui/src/script/wrath_calendar/`) holds the 96 build-12340 globals (every
`Calendar*` the executable names, `OpenCalendar`, `CanEditGuildEvent`). Evidence: one headless
Ghidra round over the bindings (`/tmp/azb-app-mig/ghidra/cal1.log`), AzerothCore for the wire's
meaning, the stock `Blizzard_Calendar` for the shapes. App publishes one `WrathCalendarView`
(listed rows with absolute server-local dates, raid resets, pending count, the open event or draft,
the `LFGDungeons` texture lists); the VM keeps the client-local state (shown month, selections,
clipboard, edited fields, invite sort, action-pending bit, the 2 s invite and 5 s add throttles).

| Receipt | Effect |
| --- | --- |
| 0436 calendar | Replaces the rows: events with the player's invite, permanent lockouts (reset at server time + seconds left), raid resets (`relationTime + n·period`), holiday days (from `Holidays.dbc` names); the zone time fixes the server's zone. `CALENDAR_UPDATE_EVENT_LIST`. |
| 0437 event | Get: opens the asked-for event (`CALENDAR_OPEN_EVENT(type)`), a resend `CALENDAR_UPDATE_EVENT` + `_INVITE_LIST`. Add/copy: listed, `CALENDAR_NEW_EVENT(isCopy)`; an add closes the draft. |
| 0438/0439 members | Fill an open draft's invites. |
| 043A/043B/043C/0445 | The open event's invitees (`CALENDAR_UPDATE_INVITE_LIST`); the player's own status and rank on the row. |
| 043D result | Error → `CALENDAR_UPDATE_ERROR(line)`, the name filling `%s`. |
| 043E/043F/0471 | Lockouts added, removed, moved. |
| 0440/0441/0443/0444 | Row added, removed (the open event closes, `CALENDAR_CLOSE_EVENT`), updated (`CALENDAR_UPDATE_EVENT`). |
| 0448 / 04BB | `CALENDAR_UPDATE_PENDING_INVITES`; the action-pending bit clears (also on 0437/043D). |

Traced (Wire B's round, `/tmp/azb-wireb-ghidra/calendar-decomp.log`):
- 043D's error switch `0x6cd380` → `0x5b9960`: its own keys for 9 (`ERR_GUILD_PLAYER_NOT_IN_GUILD`),
  11 (`PLAYER_NOT_FOUND`) and 13 (`ERR_IGNORING_YOU_S`); the limits it formats into `%d` (100 guild
  events, 30 events, 100 invites); the name only for 4, 10 and 13;
- `CalendarGetMaxDate` (`0x5b82a0`) 1 January 2031, `CalendarGetMaxCreateDate` (`0x5b7a80`) the
  first of the month thirteen months on, its year at most 2031; `CalendarGetMinDate` 24 November
  2004.

Labelled, not traced or never sent:
- `CalendarGetMinHistoryDate` (`0x5b8350`) answers the minimum date, where the decompile steps a copy
  of today back 14 days and pushes today;
- the `|4one:many;` plural escapes in the `%d` error lines, left to the text renderer;
- the day list's order (holidays, then by time);
- a holiday's day split (its first duration from each start date);
- the raid-reset projection;
- the status-option filter `0x5f26f0`;
- the texture lists' row choice (every `LFGDungeons` raid, then dungeon);
- the guild filter's level cap (80);
- the not-signed-up and sign-up constants `0x9f0df0`/`0x9e8ce8`;
- invite sorting by party (falls back to status);
- the member lists' byte;
- an open server event ignoring a mass invite;
- before the first calendar, today is the host's UTC clock.

Never sent by AzerothCore: system events, 0442 status alert, invite notes, so no
`CALENDAR_EVENT_ALARM`.
