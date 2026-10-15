//! Native build-12340 bank and money log framing (handler 0x005a4800).
//! Do not use AzerothCore's default-money branch: native treats only kinds
//! 4/5/6/8/9 as money. Unknown kinds use the item branch. Tab is a raw byte;
//! native's money-tab selection and 25-row cache are not wire-count limits.
use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const MSG_GUILD_BANK_LOG_QUERY: u16 = 0x03ee;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Log {
    pub tab: u8,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub kind: u8,
    pub actor: u64,
    pub payload: Payload,
    /// Server-provided offset, not a fabricated absolute timestamp.
    pub elapsed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    Money {
        amount: u32,
    },
    Item {
        item: u32,
        count: u32,
        destination_tab: Option<u8>,
    },
}

pub fn decode(body: &[u8]) -> io::Result<Log> {
    let mut r = Cursor::new(body);
    let tab = u8_le(&mut r)?;
    let count = u8_le(&mut r)?;
    let mut entries = Vec::new();
    for _ in 0..count {
        let kind = u8_le(&mut r)?;
        let actor = u64_le(&mut r)?;
        let payload = if matches!(kind, 4 | 5 | 6 | 8 | 9) {
            Payload::Money {
                amount: u32_le(&mut r)?,
            }
        } else {
            Payload::Item {
                item: u32_le(&mut r)?,
                count: u32_le(&mut r)?,
                destination_tab: if matches!(kind, 3 | 7) {
                    Some(u8_le(&mut r)?)
                } else {
                    None
                },
            }
        };
        entries.push(Entry {
            kind,
            actor,
            payload,
            elapsed: u32_le(&mut r)?,
        });
    }
    if r.position() != body.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "trailing guild bank log bytes",
        ));
    }
    Ok(Log { tab, entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Handwritten native branches, independent of any encoder.
    fn fixture() -> Vec<u8> {
        vec![
            6, 4, 4, 8, 7, 6, 5, 4, 3, 2, 1, 255, 255, 255, 255, 0, 0, 0, 128, 3, 0, 0, 0, 0, 0, 0,
            0, 0, 4, 3, 2, 1, 2, 0, 0, 0, 255, 1, 0, 0, 0, 7, 1, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0,
            0, 0, 0, 128, 0, 2, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0,
            255, 255, 255, 255, 255, 255, 255, 255,
        ]
    }

    #[test]
    fn native_money_move_and_unknown_kind_branches_are_lossless() {
        let log = decode(&fixture()).unwrap();
        assert_eq!(log.tab, 6);
        assert_eq!(
            log.entries,
            vec![
                Entry {
                    kind: 4,
                    actor: 0x0102030405060708,
                    payload: Payload::Money { amount: u32::MAX },
                    elapsed: 0x80000000
                },
                Entry {
                    kind: 3,
                    actor: 0,
                    payload: Payload::Item {
                        item: 0x01020304,
                        count: 2,
                        destination_tab: Some(255)
                    },
                    elapsed: 1
                },
                Entry {
                    kind: 7,
                    actor: 1,
                    payload: Payload::Item {
                        item: 9,
                        count: 0x80000000,
                        destination_tab: Some(0)
                    },
                    elapsed: 2
                },
                Entry {
                    kind: 255,
                    actor: u64::MAX,
                    payload: Payload::Item {
                        item: 0,
                        count: u32::MAX,
                        destination_tab: None
                    },
                    elapsed: u32::MAX
                },
            ]
        );
    }

    #[test]
    fn every_kind_obeys_native_predicate_and_all_u8_rows_survive() {
        for kind in 0..=255 {
            let money = matches!(kind, 4 | 5 | 6 | 8 | 9);
            let mut body = vec![255, 1, kind];
            body.extend([0; 8]);
            body.extend([1, 0, 0, 0]);
            if !money {
                body.extend([2, 0, 0, 0]);
            }
            if matches!(kind, 3 | 7) {
                body.push(254);
            }
            body.extend([3, 0, 0, 0]);
            let log = decode(&body).unwrap();
            assert_eq!(log.entries[0].kind, kind);
            assert_eq!(log.entries[0].elapsed, 3);
            assert_eq!(
                log.entries[0].payload,
                if money {
                    Payload::Money { amount: 1 }
                } else {
                    Payload::Item {
                        item: 1,
                        count: 2,
                        destination_tab: if matches!(kind, 3 | 7) {
                            Some(254)
                        } else {
                            None
                        },
                    }
                }
            );
        }
        let mut body = vec![6, 255];
        for _ in 0..255 {
            body.push(5);
            body.extend([0; 16]);
        }
        assert_eq!(decode(&body).unwrap().entries.len(), 255);
        assert_eq!(
            decode(&[255, 0]).unwrap(),
            Log {
                tab: 255,
                entries: vec![]
            }
        );
    }

    #[test]
    fn rejects_every_truncation_and_trailing_data_without_partial_result() {
        let body = fixture();
        for len in 0..body.len() {
            assert!(decode(&body[..len]).is_err(), "prefix {len}");
        }
        let mut trailing = body;
        trailing.push(0);
        assert!(decode(&trailing).is_err());
        assert!(decode(&[0, 255]).is_err());
    }
}
