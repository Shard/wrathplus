//! Build 12340 object queries and unit/world notifications. Layouts from AzerothCore
//! QueryHandler.cpp, Unit::SetPower, ThreatManager.cpp and WorldStatePackets.cpp.

use std::io::{self, Cursor};

use crate::events::SessionEvent;
use crate::wire::read_packed_guid;
use crate::wrath_session::{cstring, f32_le, u16_le, u32_le, u8_le};

pub const SMSG_GAMEOBJECT_QUERY_RESPONSE: u16 = 0x005F;
pub const SMSG_SET_PROFICIENCY: u16 = 0x0127;
pub const SMSG_INIT_WORLD_STATES: u16 = 0x02C2;
pub const SMSG_UPDATE_WORLD_STATE: u16 = 0x02C3;
pub const SMSG_POWER_UPDATE: u16 = 0x0480;
pub const SMSG_HIGHEST_THREAT_UPDATE: u16 = 0x0482;
pub const SMSG_THREAT_UPDATE: u16 = 0x0483;
pub const SMSG_THREAT_REMOVE: u16 = 0x0484;
pub const SMSG_THREAT_CLEAR: u16 = 0x0485;

pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_GAMEOBJECT_QUERY_RESPONSE
            | SMSG_SET_PROFICIENCY
            | SMSG_INIT_WORLD_STATES
            | SMSG_UPDATE_WORLD_STATE
            | SMSG_POWER_UPDATE
            | SMSG_HIGHEST_THREAT_UPDATE
            | SMSG_THREAT_UPDATE
            | SMSG_THREAT_REMOVE
            | SMSG_THREAT_CLEAR
    )
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mut r = Cursor::new(body);
    let event = match opcode {
        SMSG_GAMEOBJECT_QUERY_RESPONSE => {
            let entry = u32_le(&mut r)?;
            let mut type_id = 0;
            let mut display_id = 0;
            let mut name = String::new();
            let mut data = [0; 24];
            if entry & 0x8000_0000 == 0 {
                type_id = u32_le(&mut r)?;
                display_id = u32_le(&mut r)?;
                name = cstring(&mut r)?;
                // name2..4, icon, cast-bar caption and unknown string precede the data union.
                for _ in 0..6 {
                    cstring(&mut r)?;
                }
                for slot in &mut data {
                    *slot = u32_le(&mut r)? as i32;
                }
                let _size = f32_le(&mut r)?;
                for _ in 0..6 {
                    let _quest_item = u32_le(&mut r)?;
                }
            }
            SessionEvent::GameObjectInfo {
                entry: entry & 0x7fff_ffff,
                type_id,
                display_id,
                name,
                data,
            }
        }
        SMSG_SET_PROFICIENCY => SessionEvent::Proficiency {
            item_class: u32::from(u8_le(&mut r)?),
            subclass_mask: u32_le(&mut r)?,
        },
        SMSG_POWER_UPDATE => SessionEvent::PowerUpdate {
            guid: read_packed_guid(&mut r)?,
            power_type: u8_le(&mut r)?,
            value: u32_le(&mut r)?,
        },
        SMSG_INIT_WORLD_STATES => {
            let map = u32_le(&mut r)?;
            let zone = u32_le(&mut r)?;
            // The shared world-state consumer scopes by map/zone, not sub-area.
            let _area = u32_le(&mut r)?;
            let count = u16_le(&mut r)?;
            let mut states = Vec::new();
            for _ in 0..count {
                states.push((u32_le(&mut r)?, u32_le(&mut r)?));
            }
            SessionEvent::WorldStates {
                scope: Some((map, zone)),
                states,
            }
        }
        SMSG_UPDATE_WORLD_STATE => SessionEvent::WorldStates {
            scope: None,
            states: vec![(u32_le(&mut r)?, u32_le(&mut r)?)],
        },
        SMSG_THREAT_CLEAR => SessionEvent::ThreatClear {
            guid: read_packed_guid(&mut r)?,
        },
        SMSG_THREAT_REMOVE => SessionEvent::ThreatRemove {
            guid: read_packed_guid(&mut r)?,
            victim: read_packed_guid(&mut r)?,
        },
        SMSG_THREAT_UPDATE | SMSG_HIGHEST_THREAT_UPDATE => {
            let guid = read_packed_guid(&mut r)?;
            let highest = if opcode == SMSG_HIGHEST_THREAT_UPDATE {
                Some(read_packed_guid(&mut r)?)
            } else {
                None
            };
            let count = u32_le(&mut r)?;
            let mut threats = Vec::new();
            for _ in 0..count {
                threats.push((read_packed_guid(&mut r)?, u32_le(&mut r)?));
            }
            SessionEvent::ThreatUpdate {
                guid,
                highest,
                threats,
            }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a Wrath misc opcode",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Wrath misc trailing bytes",
        ));
    }
    Ok(vec![event])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::write_packed_guid;

    fn words(out: &mut Vec<u8>, values: &[u32]) {
        for value in values {
            out.extend(value.to_le_bytes());
        }
    }
    fn guid(out: &mut Vec<u8>, value: u64) {
        write_packed_guid(value, out).unwrap();
    }
    fn rejects_truncations(opcode: u16, body: &[u8]) {
        for end in 0..body.len() {
            assert!(decode(opcode, &body[..end]).is_err(), "prefix {end}");
        }
        let mut extra = body.to_vec();
        extra.push(0);
        assert!(decode(opcode, &extra).is_err());
    }

    #[test]
    fn gameobject_query_reads_wrath_strings_before_data() {
        let mut b = Vec::new();
        words(&mut b, &[1731, 3, 49]);
        b.extend(b"Copper Vein\0\0\0\0Interact\0Mining\0unknown\0");
        words(&mut b, &(0..24).collect::<Vec<_>>());
        b.extend(1.5f32.to_le_bytes());
        words(&mut b, &[1, 2, 3, 4, 5, 6]);
        let events = decode(SMSG_GAMEOBJECT_QUERY_RESPONSE, &b).unwrap();
        let [SessionEvent::GameObjectInfo {
            entry, name, data, ..
        }] = &events[..]
        else {
            panic!()
        };
        assert_eq!(*entry, 1731);
        assert_eq!(name, "Copper Vein");
        assert_eq!(data[23], 23);
        rejects_truncations(SMSG_GAMEOBJECT_QUERY_RESPONSE, &b);
    }

    #[test]
    fn gameobject_query_miss_masks_entry() {
        let b = 0x8000_0123u32.to_le_bytes();
        assert!(matches!(
            decode(SMSG_GAMEOBJECT_QUERY_RESPONSE, &b).unwrap()[0],
            SessionEvent::GameObjectInfo {
                entry: 0x123,
                type_id: 0,
                ..
            }
        ));
        rejects_truncations(SMSG_GAMEOBJECT_QUERY_RESPONSE, &b);
    }

    #[test]
    fn power_and_proficiency_use_native_byte_types() {
        let mut b = Vec::new();
        guid(&mut b, 0xF130000000000123);
        b.push(6);
        words(&mut b, &[120]);
        assert!(matches!(
            decode(SMSG_POWER_UPDATE, &b).unwrap()[0],
            SessionEvent::PowerUpdate {
                guid: 0xF130000000000123,
                power_type: 6,
                value: 120
            }
        ));
        rejects_truncations(SMSG_POWER_UPDATE, &b);
        let mut b = vec![4];
        words(&mut b, &[0x102]);
        assert!(matches!(
            decode(SMSG_SET_PROFICIENCY, &b).unwrap()[0],
            SessionEvent::Proficiency {
                item_class: 4,
                subclass_mask: 0x102
            }
        ));
        rejects_truncations(SMSG_SET_PROFICIENCY, &b);
    }

    #[test]
    fn world_states_are_counted_not_terminated() {
        let mut b = Vec::new();
        words(&mut b, &[0, 12, 9]);
        b.extend(2u16.to_le_bytes());
        words(&mut b, &[0, 0, 123, 456]);
        let events = decode(SMSG_INIT_WORLD_STATES, &b).unwrap();
        let [SessionEvent::WorldStates { scope, states }] = &events[..] else {
            panic!()
        };
        assert_eq!(*scope, Some((0, 12)));
        assert_eq!(states, &[(0, 0), (123, 456)]);
        rejects_truncations(SMSG_INIT_WORLD_STATES, &b);
        let mut b = Vec::new();
        words(&mut b, &[123, 456]);
        assert!(matches!(
            decode(SMSG_UPDATE_WORLD_STATE, &b).unwrap()[0],
            SessionEvent::WorldStates { scope: None, .. }
        ));
        rejects_truncations(SMSG_UPDATE_WORLD_STATE, &b);
    }

    #[test]
    fn threat_packets_keep_packed_guids_and_hundredths() {
        for opcode in [SMSG_THREAT_UPDATE, SMSG_HIGHEST_THREAT_UPDATE] {
            let mut b = Vec::new();
            guid(&mut b, 0x120034);
            if opcode == SMSG_HIGHEST_THREAT_UPDATE {
                guid(&mut b, 0x987);
            }
            words(&mut b, &[2]);
            guid(&mut b, 0x987);
            words(&mut b, &[12345]);
            guid(&mut b, 0);
            words(&mut b, &[0]);
            let events = decode(opcode, &b).unwrap();
            let [SessionEvent::ThreatUpdate {
                guid,
                highest,
                threats,
            }] = &events[..]
            else {
                panic!()
            };
            assert_eq!(*guid, 0x120034);
            assert_eq!(
                *highest,
                (opcode == SMSG_HIGHEST_THREAT_UPDATE).then_some(0x987)
            );
            assert_eq!(threats, &[(0x987, 12345), (0, 0)]);
            rejects_truncations(opcode, &b);
        }
        let mut b = Vec::new();
        guid(&mut b, 0x123);
        assert!(matches!(
            decode(SMSG_THREAT_CLEAR, &b).unwrap()[0],
            SessionEvent::ThreatClear { guid: 0x123 }
        ));
        rejects_truncations(SMSG_THREAT_CLEAR, &b);
        guid(&mut b, 0x456);
        assert!(matches!(
            decode(SMSG_THREAT_REMOVE, &b).unwrap()[0],
            SessionEvent::ThreatRemove {
                guid: 0x123,
                victim: 0x456
            }
        ));
        rejects_truncations(SMSG_THREAT_REMOVE, &b);
    }
    #[test]
    fn dispatch_routes_notifications_and_drops_bad_bodies() {
        use crate::wrath_events::{WrathDispatch, WrathEventAdapter};
        let mut adapter = WrathEventAdapter::new();
        let mut b = vec![4];
        words(&mut b, &[0x102]);
        assert!(
            matches!(adapter.dispatch(SMSG_SET_PROFICIENCY, &b, 0).unwrap(),
            WrathDispatch::Handled { session_events, .. } if matches!(session_events[0],
                SessionEvent::Proficiency { item_class: 4, subclass_mask: 0x102 }))
        );
        assert!(matches!(
            adapter.dispatch(SMSG_SET_PROFICIENCY, &[4], 0).unwrap(),
            WrathDispatch::Unhandled { .. }
        ));
    }
}
