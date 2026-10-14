//! Build 12340's aura messages, `SMSG_AURA_UPDATE_ALL` and `SMSG_AURA_UPDATE`, as the 1.12 aura
//! descriptor the client's aura feed reads. 3.3.5 sends visible auras as packets, not unit fields;
//! each unit's visible slots are mapped onto 1.12's 48 (buffs 0-31, debuffs 32-47) and the packed
//! `UNIT_FIELD_AURA*` words that changed go out as an ordinary values update. Our own auras also
//! report their remaining time as 1.12's `SMSG_UPDATE_AURA_DURATION` does.
//!
//! Layout from AzerothCore `AuraApplication::BuildUpdatePacket`: per entry `u8 slot, u32 spell`;
//! a zero spell removes; otherwise `u8 flags, u8 caster level, u8 stacks or charges`, the packed
//! caster unless `AFLAG_CASTER`, and `u32 max, u32 remaining` under `AFLAG_DURATION`.

use std::collections::HashMap;
use std::io::{self, Cursor, Read};

use crate::events::SessionEvent;
use crate::messages::ObjectFields;
use crate::wire::read_packed_guid;

pub const SMSG_AURA_UPDATE_ALL: u16 = 0x0495;
pub const SMSG_AURA_UPDATE: u16 = 0x0496;

const AFLAG_EFF_INDEX_MASK: u8 = 0x07;
const AFLAG_CASTER: u8 = 0x08;
const AFLAG_POSITIVE: u8 = 0x10;
const AFLAG_DURATION: u8 = 0x20;
const AFLAG_NEGATIVE: u8 = 0x80;

// 1.12's unit aura block (`UpdateFields.h`).
const FIELD_UNIT_AURA: u16 = 47;
const FIELD_UNIT_AURAFLAGS: u16 = 95;
const FIELD_UNIT_AURALEVELS: u16 = 101;
const FIELD_UNIT_AURAAPPLICATIONS: u16 = 113;
const SLOTS: usize = 48;
const POSITIVE_SLOTS: usize = 32;

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

/// One unit's 1.12-shaped aura block and the 3.3.5 slot each occupied 1.12 slot came from.
#[derive(Clone, Debug)]
struct UnitAuras {
    from_wrath: HashMap<u8, usize>,
    native: std::collections::BTreeMap<u8, NativeAura>,
    spells: [u32; SLOTS],
    flags: [u8; SLOTS],
    levels: [u8; SLOTS],
    applications: [u8; SLOTS],
}

impl Default for UnitAuras {
    fn default() -> Self {
        Self {
            from_wrath: HashMap::new(),
            native: Default::default(),
            spells: [0; SLOTS],
            flags: [0; SLOTS],
            levels: [0; SLOTS],
            applications: [0; SLOTS],
        }
    }
}

impl UnitAuras {
    fn words(&self) -> Vec<(u16, u32)> {
        let mut out = Vec::with_capacity(SLOTS + 6 + 24);
        for (slot, spell) in self.spells.iter().enumerate() {
            out.push((FIELD_UNIT_AURA + slot as u16, *spell));
        }
        for word in 0..SLOTS / 8 {
            let packed = (0..8).fold(0u32, |acc, k| {
                acc | (u32::from(self.flags[word * 8 + k] & 0x0f) << (k * 4))
            });
            out.push((FIELD_UNIT_AURAFLAGS + word as u16, packed));
        }
        for (base, bytes) in [
            (FIELD_UNIT_AURALEVELS, &self.levels),
            (FIELD_UNIT_AURAAPPLICATIONS, &self.applications),
        ] {
            for word in 0..SLOTS / 4 {
                let packed = (0..4).fold(0u32, |acc, k| {
                    acc | (u32::from(bytes[word * 4 + k]) << (k * 8))
                });
                out.push((base + word as u16, packed));
            }
        }
        out
    }

    fn remove(&mut self, wrath_slot: u8) {
        self.native.remove(&wrath_slot);
        if let Some(slot) = self.from_wrath.remove(&wrath_slot) {
            self.spells[slot] = 0;
            self.flags[slot] = 0;
            self.levels[slot] = 0;
            self.applications[slot] = 0;
        }
    }

    /// The 1.12 slot for a 3.3.5 one: its current slot when the class still fits, else the
    /// first free one of the class. `None` when that half is full.
    fn place(&mut self, wrath_slot: u8, helpful: bool) -> Option<usize> {
        let range = if helpful {
            0..POSITIVE_SLOTS
        } else {
            POSITIVE_SLOTS..SLOTS
        };
        if let Some(&slot) = self.from_wrath.get(&wrath_slot) {
            if range.contains(&slot) {
                return Some(slot);
            }
            self.remove(wrath_slot);
        }
        let taken: Vec<usize> = self.from_wrath.values().copied().collect();
        let slot = range.clone().find(|s| !taken.contains(s))?;
        self.from_wrath.insert(wrath_slot, slot);
        Some(slot)
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

    /// Decode one aura message into the changed 1.12 aura words and, for `self_guid`, the
    /// remaining durations.
    pub fn decode(
        &mut self,
        opcode: u16,
        body: &[u8],
        self_guid: Option<u64>,
    ) -> io::Result<Vec<SessionEvent>> {
        let mut r = Cursor::new(body);
        let guid = read_packed_guid(&mut r)?;
        let unit = self.units.get(&guid).cloned().unwrap_or_default();
        let before = unit.words();
        let mut next = if opcode == SMSG_AURA_UPDATE_ALL {
            UnitAuras::default()
        } else {
            unit.clone()
        };
        let mut durations = Vec::new();
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
            let helpful = flags & AFLAG_NEGATIVE == 0 && flags & AFLAG_POSITIVE != 0;
            let mapped_slot = next.place(wrath_slot, helpful);
            next.native.insert(wrath_slot, aura);
            let Some(slot) = mapped_slot else { continue };
            next.spells[slot] = spell_id;
            // 1.12's nibble: the effect bits shifted over the cancelable bit, which a buff has
            // (the server refuses a cancel it does not allow). A visible aura is always live.
            let effects = (flags & AFLAG_EFF_INDEX_MASK).max(1) << 1;
            next.flags[slot] = effects | u8::from(helpful);
            next.levels[slot] = level;
            next.applications[slot] = stacks.max(1) - 1;
            if Some(guid) == self_guid {
                if let Some(remaining_ms) = remaining_ms {
                    durations.push(SessionEvent::AuraDuration {
                        slot: slot as u8,
                        remaining_ms,
                    });
                }
            }
        }
        let after = next.words();
        let changed: Vec<(u16, u32)> = after
            .into_iter()
            .filter(|pair| !before.contains(pair))
            .collect();
        let snapshot = next.native.values().cloned().collect();
        self.units.insert(guid, next);
        let mut events = Vec::with_capacity(durations.len() + 2);
        events.push(SessionEvent::UnitAuras {
            guid,
            auras: snapshot,
            updated_slots,
        });
        if !changed.is_empty() {
            events.push(SessionEvent::ObjectValues {
                guid,
                fields: ObjectFields::from_pairs(&changed),
            });
        }
        events.extend(durations);
        Ok(events)
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

    fn values(events: &[SessionEvent]) -> &ObjectFields {
        events
            .iter()
            .find_map(|e| match e {
                SessionEvent::ObjectValues { fields, .. } => Some(fields),
                _ => None,
            })
            .expect("a values update")
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
    fn a_buff_and_a_debuff_land_in_their_halves_with_duration_for_self() {
        let mut t = AuraTracker::default();
        let mut body = vec![0x01, 0x07]; // packed guid 7
        body.extend(entry(
            3,
            59752,
            0x01 | AFLAG_CASTER | AFLAG_POSITIVE | AFLAG_DURATION,
            1,
            Some((120_000, 119_000)),
        ));
        body.extend(entry(9, 772, 0x01 | AFLAG_NEGATIVE, 2, None));
        let events = t.decode(SMSG_AURA_UPDATE_ALL, &body, Some(7)).unwrap();
        let f = values(&events);
        let buff = f.unit_aura(0).expect("buff in slot 0");
        assert_eq!((buff.spell_id, buff.stacks, buff.level), (59752, 1, 10));
        assert!(buff.is_helpful() && buff.is_cancelable());
        let debuff = f.unit_aura(32).expect("debuff in slot 32");
        assert_eq!((debuff.spell_id, debuff.stacks), (772, 2));
        assert!(!debuff.is_cancelable());
        assert!(events.iter().any(|e| matches!(
            e,
            SessionEvent::AuraDuration {
                slot: 0,
                remaining_ms: 119_000
            }
        )));
    }

    #[test]
    fn a_removal_clears_the_mapped_slot_and_frees_it() {
        let mut t = AuraTracker::default();
        let mut body = vec![0x01, 0x07];
        body.extend(entry(3, 100, AFLAG_CASTER | AFLAG_POSITIVE, 1, None));
        t.decode(SMSG_AURA_UPDATE, &body, None).unwrap();
        let mut gone = vec![0x01, 0x07];
        gone.extend(entry(3, 0, 0, 0, None));
        let events = t.decode(SMSG_AURA_UPDATE, &gone, None).unwrap();
        let f = values(&events);
        assert_eq!(f.unit_aura(0), None);
        let mut again = vec![0x01, 0x07];
        again.extend(entry(5, 200, AFLAG_CASTER | AFLAG_POSITIVE, 1, None));
        let events = t.decode(SMSG_AURA_UPDATE, &again, None).unwrap();
        assert_eq!(values(&events).unit_aura(0).map(|a| a.spell_id), Some(200));
    }
}
