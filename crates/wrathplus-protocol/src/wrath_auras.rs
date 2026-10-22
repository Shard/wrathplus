//! Build 12340 packet aura snapshots, never synthesized descriptor words.
//!
//! Layout from AzerothCore `AuraApplication::BuildUpdatePacket`: per entry `u8 slot, u32 spell`;
//! a zero spell removes; otherwise `u8 flags, u8 caster level, u8 stacks or charges`, the packed
//! caster unless `AFLAG_CASTER`, and `u32 max, u32 remaining` under `AFLAG_DURATION`.

use std::collections::HashMap;
use std::io::{self, Cursor, Read};

use crate::events::SessionEvent;
use crate::wire::read_packed_guid;

pub const SMSG_AURA_UPDATE_ALL: u16 = 0x0495;
pub const SMSG_AURA_UPDATE: u16 = 0x0496;

const AFLAG_CASTER: u8 = 0x08;
const AFLAG_POSITIVE: u8 = 0x10;
const AFLAG_DURATION: u8 = 0x20;
const AFLAG_NEGATIVE: u8 = 0x80;

/// One build-12340 visible aura, with native slot and unmodified wire metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeAura {
    pub slot: u8,
    pub spell_id: u32,
    pub flags: u8,
    pub level: u8,
    pub stacks: u8,
    /// Absent on the wire when AFLAG_CASTER says the target is its own caster.
    pub caster_guid: Option<u64>,
    pub max_duration_ms: Option<u32>,
    pub remaining_ms: Option<u32>,
}

impl NativeAura {
    pub fn is_helpful(&self) -> bool {
        self.flags & AFLAG_NEGATIVE == 0 && self.flags & AFLAG_POSITIVE != 0
    }
}

/// One unit's native slots, ordered for stable snapshot presentation.
#[derive(Clone, Debug, Default)]
struct UnitAuras {
    native: std::collections::BTreeMap<u8, NativeAura>,
}

impl UnitAuras {
    fn remove(&mut self, slot: u8) {
        self.native.remove(&slot);
    }
}

/// Every tracked unit's aura block, owned by the Wrath event adapter.
#[derive(Debug, Default)]
pub struct AuraTracker {
    units: HashMap<u64, UnitAuras>,
}

impl AuraTracker {
    /// A (re)created or departed unit starts with no auras; `SMSG_AURA_UPDATE_ALL` follows a
    /// create.
    pub fn forget(&mut self, guid: u64) {
        self.units.remove(&guid);
    }

    pub fn reset(&mut self) {
        self.units.clear();
    }

    /// Decode transactionally into native snapshots with only the wire-updated slot list.
    pub fn decode(
        &mut self,
        opcode: u16,
        body: &[u8],
        _self_guid: Option<u64>,
    ) -> io::Result<Vec<SessionEvent>> {
        let mut r = Cursor::new(body);
        let guid = read_packed_guid(&mut r)?;
        let unit = self.units.get(&guid).cloned().unwrap_or_default();
        let mut next = if opcode == SMSG_AURA_UPDATE_ALL {
            UnitAuras::default()
        } else {
            unit.clone()
        };
        let mut updated_slots = Vec::new();
        while (r.position() as usize) < body.len() {
            let wrath_slot = u8_le(&mut r)?;
            updated_slots.push(wrath_slot);
            let spell_id = u32_le(&mut r)?;
            if spell_id == 0 {
                next.remove(wrath_slot);
                continue;
            }
            let flags = u8_le(&mut r)?;
            let level = u8_le(&mut r)?;
            let stacks = u8_le(&mut r)?;
            let caster_guid = if flags & AFLAG_CASTER == 0 {
                Some(read_packed_guid(&mut r)?)
            } else {
                None
            };
            let (max_duration_ms, remaining_ms) = if flags & AFLAG_DURATION != 0 {
                (Some(u32_le(&mut r)?), Some(u32_le(&mut r)?))
            } else {
                (None, None)
            };
            let aura = NativeAura {
                slot: wrath_slot,
                spell_id,
                flags,
                level,
                stacks,
                caster_guid,
                max_duration_ms,
                remaining_ms,
            };
            next.native.insert(wrath_slot, aura);
        }
        let snapshot = next.native.values().cloned().collect();
        self.units.insert(guid, next);
        Ok(vec![SessionEvent::UnitAuras {
            guid,
            auras: snapshot,
            updated_slots,
        }])
    }
}

fn u8_le(r: &mut impl Read) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn u32_le(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(slot: u8, spell: u32, flags: u8, stacks: u8, duration: Option<(u32, u32)>) -> Vec<u8> {
        let mut out = vec![slot];
        out.extend(spell.to_le_bytes());
        if spell == 0 {
            return out;
        }
        out.extend([flags, 10, stacks]);
        if flags & AFLAG_CASTER == 0 {
            out.extend([0x01, 0x42]); // packed guid 0x42
        }
        if let Some((max, left)) = duration {
            out.extend(max.to_le_bytes());
            out.extend(left.to_le_bytes());
        }
        out
    }

    fn snapshot(events: &[SessionEvent]) -> &[NativeAura] {
        events
            .iter()
            .find_map(|e| match e {
                SessionEvent::UnitAuras { auras, .. } => Some(auras.as_slice()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn native_snapshots_preserve_slots_metadata_and_merge_remove_reset_transactionally() {
        let mut tracker = AuraTracker::default();
        let mut body = vec![1, 7];
        body.extend(entry(
            55,
            123,
            AFLAG_POSITIVE | AFLAG_DURATION,
            3,
            Some((30000, 29000)),
        ));
        let events = tracker
            .decode(SMSG_AURA_UPDATE_ALL, &body, Some(7))
            .unwrap();
        assert_eq!(
            snapshot(&events),
            &[NativeAura {
                slot: 55,
                spell_id: 123,
                flags: AFLAG_POSITIVE | AFLAG_DURATION,
                level: 10,
                stacks: 3,
                caster_guid: Some(0x42),
                max_duration_ms: Some(30000),
                remaining_ms: Some(29000)
            }]
        );
        let mut delta = vec![1, 7];
        delta.extend(entry(2, 456, AFLAG_CASTER | AFLAG_NEGATIVE, 1, None));
        let events = tracker.decode(SMSG_AURA_UPDATE, &delta, None).unwrap();
        assert_eq!(
            snapshot(&events).iter().map(|a| a.slot).collect::<Vec<_>>(),
            vec![2, 55]
        );
        assert!(tracker
            .decode(SMSG_AURA_UPDATE_ALL, &[1, 7, 3, 1], None)
            .is_err());
        let events = tracker.decode(SMSG_AURA_UPDATE, &[1, 7], None).unwrap();
        assert_eq!(snapshot(&events).len(), 2);
        let mut remove = vec![1, 7];
        remove.extend(entry(55, 0, 0, 0, None));
        let events = tracker.decode(SMSG_AURA_UPDATE, &remove, None).unwrap();
        assert_eq!(snapshot(&events).len(), 1);
        assert_eq!(snapshot(&events)[0].caster_guid, None);
        let events = tracker.decode(SMSG_AURA_UPDATE_ALL, &[1, 7], None).unwrap();
        assert!(snapshot(&events).is_empty());
        tracker.decode(SMSG_AURA_UPDATE, &body, None).unwrap();
        tracker.forget(7);
        assert!(snapshot(&tracker.decode(SMSG_AURA_UPDATE, &[1, 7], None).unwrap()).is_empty());
    }

    #[test]
    fn packet_auras_never_emit_descriptor_or_legacy_duration_events() {
        let mut tracker = AuraTracker::default();
        let mut body = vec![1, 7];
        for slot in 0..64 {
            body.extend(entry(
                slot,
                100 + u32::from(slot),
                AFLAG_CASTER | AFLAG_POSITIVE | AFLAG_DURATION,
                2,
                Some((120000, 119000)),
            ));
        }
        let events = tracker
            .decode(SMSG_AURA_UPDATE_ALL, &body, Some(7))
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(snapshot(&events).len(), 64);
        assert_eq!(snapshot(&events)[63].slot, 63);
        assert_eq!(snapshot(&events)[63].remaining_ms, Some(119000));
        let mut removal = vec![1, 7];
        removal.extend(entry(63, 0, 0, 0, None));
        let events = tracker.decode(SMSG_AURA_UPDATE, &removal, Some(7)).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(snapshot(&events).len(), 63);
        assert!(
            matches!(&events[0], SessionEvent::UnitAuras { updated_slots, .. } if updated_slots == &[63])
        );
        tracker.reset();
        assert!(snapshot(&tracker.decode(SMSG_AURA_UPDATE, &[1, 7], None).unwrap()).is_empty());
    }
}
