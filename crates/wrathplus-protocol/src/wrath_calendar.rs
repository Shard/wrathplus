//! Build-12340 calendar receipts and commands. Capture-only: no calendar, invites, lockouts,
//! holidays, pending counts or Lua events are inferred.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: CalendarHandler.cpp
//! (`HandleCalendarGetCalendar`, the handlers' reads, `SendCalendarRaidLockout*`), CalendarMgr.cpp
//! (`SendCalendar*`), CalendarPackets.cpp, Guild.cpp and ArenaTeam.cpp (`MassInviteToEvent`).
//! Strict framing of the AC emitted shapes; raw flags, ranks, statuses and unknown bytes are kept.
//! Packed times stay packed.
use crate::wire::{read_cstring, read_packed_guid, write_packed_guid};
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 18] = [
    0x0436, 0x0437, 0x0438, 0x0439, 0x043a, 0x043b, 0x043c, 0x043d, 0x043e, 0x043f, 0x0440, 0x0441,
    0x0443, 0x0444, 0x0445, 0x0448, 0x0471, 0x04bb,
];

/// `MAX_HOLIDAY_DATES`, `MAX_HOLIDAY_DURATIONS`, `MAX_HOLIDAY_FLAGS`.
const HOLIDAY_DATES: usize = 26;
const HOLIDAY_DURATIONS: usize = 10;
const HOLIDAY_FLAGS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteRow {
    pub event_id: u64,
    pub invite_id: u64,
    pub status: u8,
    pub rank: u8,
    pub guild_event: u8,
    /// The event's creator, or the invite's sender when the event is gone.
    pub creator: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRow {
    pub event_id: u64,
    pub title: String,
    pub kind: u32,
    pub packed_time: u32,
    pub flags: u32,
    pub dungeon: i32,
    pub creator: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lockout {
    pub map: u32,
    pub difficulty: u32,
    pub seconds_left: u32,
    pub instance: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaidReset {
    pub map: i32,
    pub period: i32,
    pub offset: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holiday {
    pub id: u32,
    pub region: u32,
    pub looping: u32,
    pub priority: u32,
    pub filter_type: u32,
    pub dates: [u32; HOLIDAY_DATES],
    pub durations: [u32; HOLIDAY_DURATIONS],
    pub flags: [u32; HOLIDAY_FLAGS],
    pub texture: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    pub invites: Vec<InviteRow>,
    pub events: Vec<EventRow>,
    pub server_time: u32,
    pub zone_packed_time: u32,
    pub lockouts: Vec<Lockout>,
    pub relation_time: u32,
    pub raid_resets: Vec<RaidReset>,
    pub holidays: Vec<Holiday>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventInvitee {
    pub guid: u64,
    pub level: u8,
    pub status: u8,
    pub rank: u8,
    pub guild_member: u8,
    pub invite_id: u64,
    pub status_time: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub send_type: u8,
    pub creator: u64,
    pub event_id: u64,
    pub title: String,
    pub description: String,
    pub kind: u8,
    pub repeat: u8,
    pub max_invites: u32,
    pub dungeon: i32,
    pub flags: u32,
    pub packed_time: u32,
    pub timezone_time: u32,
    pub guild_id: u32,
    pub invitees: Vec<EventInvitee>,
}

/// Capture-only receipts, in arrival order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// 0436.
    Calendar(Calendar),
    /// 0437.
    Event(Event),
    /// 0438: guild members matching the filter, each with a raw byte.
    FilterGuild { members: Vec<(u64, u8)> },
    /// 0439: arena teammates, each with a raw byte.
    ArenaTeam { members: Vec<(u64, u8)> },
    /// 043A: `status_time` is present only when the raw flag is nonzero.
    Invite {
        invitee: u64,
        event_id: u64,
        invite_id: u64,
        level: u8,
        status: u8,
        status_time: Option<u32>,
        raw_not_signup: u8,
    },
    /// 043B.
    InviteRemoved {
        invitee: u64,
        event_id: u64,
        flags: u32,
        raw_byte: u8,
    },
    /// 043C.
    Status {
        invitee: u64,
        event_id: u64,
        packed_time: u32,
        flags: u32,
        status: u8,
        rank: u8,
        status_time: u32,
    },
    /// 043D: `param` names a player for three errors, else empty.
    CommandResult {
        raw_word: u32,
        raw_byte: u8,
        param: String,
        error: u32,
    },
    /// 043E.
    RaidLockoutAdded { packed_time: u32, lockout: Lockout },
    /// 043F.
    RaidLockoutRemoved { lockout: Lockout },
    /// 0471: the old and new seconds to reset after an extend toggle.
    RaidLockoutUpdated {
        packed_time: u32,
        map: u32,
        difficulty: u32,
        old_seconds: u32,
        new_seconds: u32,
    },
    /// 0440.
    InviteAlert {
        event_id: u64,
        title: String,
        packed_time: u32,
        flags: u32,
        kind: u32,
        dungeon: i32,
        invite_id: u64,
        status: u8,
        rank: u8,
        creator: u64,
        sender: u64,
    },
    /// 0441.
    InviteRemovedAlert {
        event_id: u64,
        packed_time: u32,
        flags: u32,
        status: u8,
    },
    /// 0443.
    RemovedAlert {
        raw_byte: u8,
        event_id: u64,
        packed_time: u32,
    },
    /// 0444.
    UpdatedAlert {
        raw_byte: u8,
        event_id: u64,
        old_packed_time: u32,
        flags: u32,
        packed_time: u32,
        kind: u8,
        dungeon: i32,
        title: String,
        description: String,
        repeat: u8,
        max_invites: u32,
        raw_unknown: u32,
    },
    /// 0445.
    ModeratorStatusAlert {
        invitee: u64,
        event_id: u64,
        rank: u8,
        raw_byte: u8,
    },
    /// 0448.
    NumPending { count: u32 },
    /// 04BB: empty.
    ClearPendingAction,
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u8_(r: &mut Cursor<&[u8]>) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn u32_(r: &mut Cursor<&[u8]>) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn u64_(r: &mut Cursor<&[u8]>) -> io::Result<u64> {
    let mut b = [0; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn words<const N: usize>(r: &mut Cursor<&[u8]>) -> io::Result<[u32; N]> {
    let mut out = [0; N];
    for word in &mut out {
        *word = u32_(r)?;
    }
    Ok(out)
}

/// A wire count, bounded by the bytes left at `min_row` bytes a row.
fn count(r: &mut Cursor<&[u8]>, min_row: usize) -> io::Result<usize> {
    let count = u32_(r)? as usize;
    let left = r.get_ref().len() - r.position() as usize;
    if count > left / min_row {
        return Err(invalid("calendar count"));
    }
    Ok(count)
}

fn rows<T>(
    r: &mut Cursor<&[u8]>,
    min_row: usize,
    row: impl Fn(&mut Cursor<&[u8]>) -> io::Result<T>,
) -> io::Result<Vec<T>> {
    let n = count(r, min_row)?;
    (0..n).map(|_| row(r)).collect()
}

fn lockout(r: &mut Cursor<&[u8]>) -> io::Result<Lockout> {
    Ok(Lockout {
        map: u32_(r)?,
        difficulty: u32_(r)?,
        seconds_left: u32_(r)?,
        instance: u64_(r)?,
    })
}

fn members(r: &mut Cursor<&[u8]>) -> io::Result<Vec<(u64, u8)>> {
    rows(r, 2, |r| Ok((read_packed_guid(r)?, u8_(r)?)))
}

fn calendar(r: &mut Cursor<&[u8]>) -> io::Result<Calendar> {
    let invites = rows(r, 20, |r| {
        Ok(InviteRow {
            event_id: u64_(r)?,
            invite_id: u64_(r)?,
            status: u8_(r)?,
            rank: u8_(r)?,
            guild_event: u8_(r)?,
            creator: read_packed_guid(r)?,
        })
    })?;
    let events = rows(r, 26, |r| {
        Ok(EventRow {
            event_id: u64_(r)?,
            title: read_cstring(r)?,
            kind: u32_(r)?,
            packed_time: u32_(r)?,
            flags: u32_(r)?,
            dungeon: u32_(r)? as i32,
            creator: read_packed_guid(r)?,
        })
    })?;
    let server_time = u32_(r)?;
    let zone_packed_time = u32_(r)?;
    let lockouts = rows(r, 20, lockout)?;
    let relation_time = u32_(r)?;
    let raid_resets = rows(r, 12, |r| {
        Ok(RaidReset {
            map: u32_(r)? as i32,
            period: u32_(r)? as i32,
            offset: u32_(r)? as i32,
        })
    })?;
    let holidays = rows(
        r,
        4 * (5 + HOLIDAY_DATES + HOLIDAY_DURATIONS + HOLIDAY_FLAGS) + 1,
        |r| {
            Ok(Holiday {
                id: u32_(r)?,
                region: u32_(r)?,
                looping: u32_(r)?,
                priority: u32_(r)?,
                filter_type: u32_(r)?,
                dates: words(r)?,
                durations: words(r)?,
                flags: words(r)?,
                texture: read_cstring(r)?,
            })
        },
    )?;
    Ok(Calendar {
        invites,
        events,
        server_time,
        zone_packed_time,
        lockouts,
        relation_time,
        raid_resets,
        holidays,
    })
}

fn event(r: &mut Cursor<&[u8]>) -> io::Result<Event> {
    Ok(Event {
        send_type: u8_(r)?,
        creator: read_packed_guid(r)?,
        event_id: u64_(r)?,
        title: read_cstring(r)?,
        description: read_cstring(r)?,
        kind: u8_(r)?,
        repeat: u8_(r)?,
        max_invites: u32_(r)?,
        dungeon: u32_(r)? as i32,
        flags: u32_(r)?,
        packed_time: u32_(r)?,
        timezone_time: u32_(r)?,
        guild_id: u32_(r)?,
        invitees: rows(r, 18, |r| {
            Ok(EventInvitee {
                guid: read_packed_guid(r)?,
                level: u8_(r)?,
                status: u8_(r)?,
                rank: u8_(r)?,
                guild_member: u8_(r)?,
                invite_id: u64_(r)?,
                status_time: u32_(r)?,
                text: read_cstring(r)?,
            })
        })?,
    })
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let packet = match opcode {
        0x0436 => Packet::Calendar(calendar(r)?),
        0x0437 => Packet::Event(event(r)?),
        0x0438 => Packet::FilterGuild {
            members: members(r)?,
        },
        0x0439 => Packet::ArenaTeam {
            members: members(r)?,
        },
        0x043a => {
            let (invitee, event_id, invite_id) = (read_packed_guid(r)?, u64_(r)?, u64_(r)?);
            let (level, status, has_time) = (u8_(r)?, u8_(r)?, u8_(r)?);
            let status_time = if has_time != 0 { Some(u32_(r)?) } else { None };
            Packet::Invite {
                invitee,
                event_id,
                invite_id,
                level,
                status,
                status_time,
                raw_not_signup: u8_(r)?,
            }
        }
        0x043b => Packet::InviteRemoved {
            invitee: read_packed_guid(r)?,
            event_id: u64_(r)?,
            flags: u32_(r)?,
            raw_byte: u8_(r)?,
        },
        0x043c => Packet::Status {
            invitee: read_packed_guid(r)?,
            event_id: u64_(r)?,
            packed_time: u32_(r)?,
            flags: u32_(r)?,
            status: u8_(r)?,
            rank: u8_(r)?,
            status_time: u32_(r)?,
        },
        0x043d => Packet::CommandResult {
            raw_word: u32_(r)?,
            raw_byte: u8_(r)?,
            param: read_cstring(r)?,
            error: u32_(r)?,
        },
        0x043e => Packet::RaidLockoutAdded {
            packed_time: u32_(r)?,
            lockout: lockout(r)?,
        },
        0x043f => Packet::RaidLockoutRemoved {
            lockout: lockout(r)?,
        },
        0x0471 => Packet::RaidLockoutUpdated {
            packed_time: u32_(r)?,
            map: u32_(r)?,
            difficulty: u32_(r)?,
            old_seconds: u32_(r)?,
            new_seconds: u32_(r)?,
        },
        0x0440 => Packet::InviteAlert {
            event_id: u64_(r)?,
            title: read_cstring(r)?,
            packed_time: u32_(r)?,
            flags: u32_(r)?,
            kind: u32_(r)?,
            dungeon: u32_(r)? as i32,
            invite_id: u64_(r)?,
            status: u8_(r)?,
            rank: u8_(r)?,
            creator: read_packed_guid(r)?,
            sender: read_packed_guid(r)?,
        },
        0x0441 => Packet::InviteRemovedAlert {
            event_id: u64_(r)?,
            packed_time: u32_(r)?,
            flags: u32_(r)?,
            status: u8_(r)?,
        },
        0x0443 => Packet::RemovedAlert {
            raw_byte: u8_(r)?,
            event_id: u64_(r)?,
            packed_time: u32_(r)?,
        },
        0x0444 => Packet::UpdatedAlert {
            raw_byte: u8_(r)?,
            event_id: u64_(r)?,
            old_packed_time: u32_(r)?,
            flags: u32_(r)?,
            packed_time: u32_(r)?,
            kind: u8_(r)?,
            dungeon: u32_(r)? as i32,
            title: read_cstring(r)?,
            description: read_cstring(r)?,
            repeat: u8_(r)?,
            max_invites: u32_(r)?,
            raw_unknown: u32_(r)?,
        },
        0x0445 => Packet::ModeratorStatusAlert {
            invitee: read_packed_guid(r)?,
            event_id: u64_(r)?,
            rank: u8_(r)?,
            raw_byte: u8_(r)?,
        },
        0x0448 => Packet::NumPending { count: u32_(r)? },
        0x04bb => Packet::ClearPendingAction,
        _ => return Err(invalid("calendar opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("calendar packet extent"));
    }
    Ok(packet)
}

/// One invitee of an added event: packed GUID, status, rank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddInvitee {
    pub guid: u64,
    pub status: u8,
    pub rank: u8,
}

/// Calendar commands. None has a UI producer yet; transport claims no server acceptance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    GetCalendar,
    GetEvent {
        event_id: u64,
    },
    GuildFilter {
        min_level: u32,
        max_level: u32,
        min_rank: u32,
    },
    ArenaTeam {
        team_id: u32,
    },
    /// `invites` is absent for a guild announcement, which AC reads none for.
    AddEvent {
        title: String,
        description: String,
        kind: u8,
        repeat: u8,
        max_invites: u32,
        dungeon: i32,
        packed_time: u32,
        raw_packed_time: u32,
        flags: u32,
        invites: Option<Vec<AddInvitee>>,
    },
    UpdateEvent {
        event_id: u64,
        invite_id: u64,
        title: String,
        description: String,
        kind: u8,
        repeat: u8,
        max_invites: u32,
        dungeon: i32,
        packed_time: u32,
        timezone_time: u32,
        flags: u32,
    },
    /// AC reads the event id and skips the rest.
    RemoveEvent {
        event_id: u64,
        invite_id: u64,
        flags: u32,
    },
    CopyEvent {
        event_id: u64,
        invite_id: u64,
        packed_time: u32,
    },
    Invite {
        event_id: u64,
        invite_id: u64,
        name: String,
        pre_invite: u8,
        guild_event: u8,
    },
    Rsvp {
        event_id: u64,
        invite_id: u64,
        status: u32,
    },
    RemoveInvite {
        invitee: u64,
        invite_id: u64,
        owner_invite_id: u64,
        event_id: u64,
    },
    Status {
        invitee: u64,
        event_id: u64,
        invite_id: u64,
        owner_invite_id: u64,
        status: u8,
    },
    ModeratorStatus {
        invitee: u64,
        event_id: u64,
        invite_id: u64,
        owner_invite_id: u64,
        rank: u8,
    },
    Complain {
        event_id: u64,
        guid: u64,
    },
    GetNumPending,
    Signup {
        event_id: u64,
        tentative: u8,
    },
}

fn cstring(text: &str, body: &mut Vec<u8>) {
    body.extend(text.split('\0').next().unwrap_or("").as_bytes());
    body.push(0);
}

fn packed(guid: u64, body: &mut Vec<u8>) {
    write_packed_guid(guid, body).expect("writing to a Vec");
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    let mut b = Vec::new();
    let opcode = match command {
        Command::GetCalendar => 0x0429,
        Command::GetEvent { event_id } => {
            b.extend(event_id.to_le_bytes());
            0x042a
        }
        Command::GuildFilter {
            min_level,
            max_level,
            min_rank,
        } => {
            for word in [min_level, max_level, min_rank] {
                b.extend(word.to_le_bytes());
            }
            0x042b
        }
        Command::ArenaTeam { team_id } => {
            b.extend(team_id.to_le_bytes());
            0x042c
        }
        Command::AddEvent {
            title,
            description,
            kind,
            repeat,
            max_invites,
            dungeon,
            packed_time,
            raw_packed_time,
            flags,
            invites,
        } => {
            cstring(title, &mut b);
            cstring(description, &mut b);
            b.extend([*kind, *repeat]);
            b.extend(max_invites.to_le_bytes());
            b.extend(dungeon.to_le_bytes());
            for word in [packed_time, raw_packed_time, flags] {
                b.extend(word.to_le_bytes());
            }
            if let Some(invites) = invites {
                b.extend((invites.len() as u32).to_le_bytes());
                for invite in invites {
                    packed(invite.guid, &mut b);
                    b.extend([invite.status, invite.rank]);
                }
            }
            0x042d
        }
        Command::UpdateEvent {
            event_id,
            invite_id,
            title,
            description,
            kind,
            repeat,
            max_invites,
            dungeon,
            packed_time,
            timezone_time,
            flags,
        } => {
            b.extend(event_id.to_le_bytes());
            b.extend(invite_id.to_le_bytes());
            cstring(title, &mut b);
            cstring(description, &mut b);
            b.extend([*kind, *repeat]);
            b.extend(max_invites.to_le_bytes());
            b.extend(dungeon.to_le_bytes());
            for word in [packed_time, timezone_time, flags] {
                b.extend(word.to_le_bytes());
            }
            0x042e
        }
        Command::RemoveEvent {
            event_id,
            invite_id,
            flags,
        } => {
            b.extend(event_id.to_le_bytes());
            b.extend(invite_id.to_le_bytes());
            b.extend(flags.to_le_bytes());
            0x042f
        }
        Command::CopyEvent {
            event_id,
            invite_id,
            packed_time,
        } => {
            b.extend(event_id.to_le_bytes());
            b.extend(invite_id.to_le_bytes());
            b.extend(packed_time.to_le_bytes());
            0x0430
        }
        Command::Invite {
            event_id,
            invite_id,
            name,
            pre_invite,
            guild_event,
        } => {
            b.extend(event_id.to_le_bytes());
            b.extend(invite_id.to_le_bytes());
            cstring(name, &mut b);
            b.extend([*pre_invite, *guild_event]);
            0x0431
        }
        Command::Rsvp {
            event_id,
            invite_id,
            status,
        } => {
            b.extend(event_id.to_le_bytes());
            b.extend(invite_id.to_le_bytes());
            b.extend(status.to_le_bytes());
            0x0432
        }
        Command::RemoveInvite {
            invitee,
            invite_id,
            owner_invite_id,
            event_id,
        } => {
            packed(*invitee, &mut b);
            for id in [invite_id, owner_invite_id, event_id] {
                b.extend(id.to_le_bytes());
            }
            0x0433
        }
        Command::Status {
            invitee,
            event_id,
            invite_id,
            owner_invite_id,
            status,
        } => {
            packed(*invitee, &mut b);
            for id in [event_id, invite_id, owner_invite_id] {
                b.extend(id.to_le_bytes());
            }
            b.push(*status);
            0x0434
        }
        Command::ModeratorStatus {
            invitee,
            event_id,
            invite_id,
            owner_invite_id,
            rank,
        } => {
            packed(*invitee, &mut b);
            for id in [event_id, invite_id, owner_invite_id] {
                b.extend(id.to_le_bytes());
            }
            b.push(*rank);
            0x0435
        }
        Command::Complain { event_id, guid } => {
            b.extend(event_id.to_le_bytes());
            b.extend(guid.to_le_bytes());
            0x0446
        }
        Command::GetNumPending => 0x0447,
        Command::Signup {
            event_id,
            tentative,
        } => {
            b.extend(event_id.to_le_bytes());
            b.push(*tentative);
            0x04ba
        }
    };
    (opcode, b)
}

#[cfg(test)]
mod tests;
