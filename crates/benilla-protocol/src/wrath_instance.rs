//! Build 12340 saved-instance snapshot (AzerothCore PlayerStorage.cpp::SendRaidInfo).
//! Native receiver 0x501030 retains both flag bytes; do not invert the first as "expired".
use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_RAID_INSTANCE_INFO: u16 = 0x02cc;
pub const CMSG_REQUEST_RAID_INFO: u32 = 0x02cd;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaidInstanceInfo {
    pub entries: Vec<SavedInstance>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedInstance {
    pub map: u32,
    pub difficulty: u32,
    pub instance_guid: u64,
    pub raw_flag: u8,
    pub extended: u8,
    pub reset_seconds: u32,
}

pub fn decode(body: &[u8]) -> io::Result<RaidInstanceInfo> {
    let mut r = Cursor::new(body);
    let count = u32_le(&mut r)? as usize;
    let remaining = body.len() - r.position() as usize;
    // Validate the exact extent before any count-controlled allocation.
    if remaining % 22 != 0 || count != remaining / 22 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Wrath saved-instance count/tail",
        ));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        entries.push(SavedInstance {
            map: u32_le(&mut r)?,
            difficulty: u32_le(&mut r)?,
            instance_guid: u64_le(&mut r)?,
            raw_flag: u8_le(&mut r)?,
            extended: u8_le(&mut r)?,
            reset_seconds: u32_le(&mut r)?,
        });
    }
    Ok(RaidInstanceInfo { entries })
}

#[cfg(test)]
mod tests;
