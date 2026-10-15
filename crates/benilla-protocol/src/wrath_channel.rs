//! Build 12340 channel roster. AzerothCore Chat/Channels/Channel.cpp::Channel::List
//! (lines 702–730): leading u8 channel type, cstring name, u8 channel flags,
//! u32 visible-member count, then (full guid, u8 member flags). Unlike 1.12, type leads name.
use crate::events::SessionEvent;
use crate::wrath_session::{cstring, u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_CHANNEL_LIST: u16 = 0x009b;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelList {
    /// AC currently writes 1; preserve unknown values rather than interpreting them as flags.
    pub channel_type: u8,
    pub channel: String,
    pub flags: u8,
    pub members: Vec<(u64, u8)>,
}
impl ChannelList {
    pub fn session_event(&self) -> SessionEvent {
        SessionEvent::ChannelList {
            channel: self.channel.clone(),
            flags: self.flags,
            members: self.members.clone(),
        }
    }
}
pub fn decode(body: &[u8]) -> io::Result<ChannelList> {
    let mut r = Cursor::new(body);
    let channel_type = u8_le(&mut r)?;
    let channel = cstring(&mut r)?;
    let flags = u8_le(&mut r)?;
    let count = u32_le(&mut r)? as usize;
    let remaining = body.len() - r.position() as usize;
    // Validate before allocation; no unbounded count-controlled allocation or partial rosters.
    if remaining % 9 != 0 || count != remaining / 9 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Wrath channel roster count/tail",
        ));
    }
    let mut members = Vec::with_capacity(count);
    for _ in 0..count {
        members.push((u64_le(&mut r)?, u8_le(&mut r)?));
    }
    Ok(ChannelList {
        channel_type,
        channel,
        flags,
        members,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wrath_events::{WrathDispatch, WrathEvent, WrathEventAdapter};
    fn packet() -> Vec<u8> {
        let mut b = b"\x07General\0\x81".to_vec();
        b.extend(2u32.to_le_bytes());
        b.extend(0x1234_5678_9abc_def0u64.to_le_bytes());
        b.push(0x83);
        b.extend(12u64.to_le_bytes());
        b.push(4);
        b
    }
    #[test]
    fn roster_shared_semantics_and_native_type_are_both_retained_in_dispatch() {
        let mut adapter = WrathEventAdapter::new();
        let WrathDispatch::Handled {
            session_events,
            wrath_events,
            ..
        } = adapter.dispatch(SMSG_CHANNEL_LIST, &packet(), 0).unwrap()
        else {
            panic!("unhandled")
        };
        assert!(
            matches!(&session_events[0], SessionEvent::ChannelList { channel, flags: 0x81, members }
            if channel == "General" && members == &vec![(0x1234_5678_9abc_def0,0x83),(12,4)])
        );
        assert!(
            matches!(&session_events[1], SessionEvent::WrathChannelList { roster } if roster.channel_type == 7)
        );
        assert!(
            matches!(&wrath_events[0], WrathEvent::ChannelList(list) if list.channel_type == 7)
        );
    }
    #[test]
    fn empty_roster_and_strict_count_truncation_and_tail() {
        assert!(decode(b"\x01General\0\0\0\0\0\0")
            .unwrap()
            .members
            .is_empty());
        let b = packet();
        for end in 0..b.len() {
            assert!(decode(&b[..end]).is_err(), "end {end}");
        }
        let mut bad = b.clone();
        bad.push(0);
        assert!(decode(&bad).is_err());
        let mut bad = b;
        bad[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&bad).is_err());
    }
}
