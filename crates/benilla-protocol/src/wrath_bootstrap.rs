//! Build 12340's auth bootstrap packets, sent right after `SMSG_AUTH_RESPONSE` and before any
//! character is in the world (AzerothCore `WorldSession::SendAddonsInfo`,
//! `SendClientCacheVersion`). [`crate::wrath_world::WrathWorldSession`] keeps them through the
//! select screen; the gameplay stream publishes them as [`SessionEvent::WrathBootstrap`].

use std::io::{self, Read};

use crate::events::SessionEvent;

pub const SMSG_ADDON_INFO: u16 = 0x02ef;
pub const SMSG_CLIENTCACHE_VERSION: u16 = 0x04ab;

/// Whether `opcode` is one of the two bootstrap packets.
pub fn handles(opcode: u16) -> bool {
    matches!(opcode, SMSG_ADDON_INFO | SMSG_CLIENTCACHE_VERSION)
}

/// One addon AzerothCore names in `SMSG_ADDON_INFO`'s banned list (`AddonMgr::BannedAddon`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BannedAddon {
    pub id: u32,
    pub name_md5: [u8; 16],
    pub version_md5: [u8; 16],
    pub timestamp: u32,
    /// AzerothCore writes 1.
    pub banned: u32,
}

/// The server's verdict on one addon our auth session named, as 12340's reader `0x5f79a0` takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddonVerdict {
    /// The addon this record answers: the auth session's record in the same position.
    pub name: &'static str,
    /// AzerothCore writes 2, which the reader turns into "secure": the addon leaves the Lua index
    /// space (`GetNumAddOns`, `GetAddOnInfo(index)`). The reader marks a 0 as not loadable.
    pub state: u8,
    /// The public key a server sends when the addon's CRC is not the standard one; the reader
    /// saves it to the addon's `.pub`. AzerothCore sends it only for a non-standard CRC.
    pub public_key: Option<Box<[u8; 256]>>,
    /// The `u32` after the key byte, present when the key byte block is; AzerothCore writes 0.
    /// The reader flags the addon when this exceeds the record's own (unresolved meaning).
    pub key_stamp: Option<u32>,
    /// An update URL the reader saves to the addon's `.url`; AzerothCore sends none.
    pub url: Option<String>,
}

impl AddonVerdict {
    /// Whether the reader hides the addon from the Lua index space (`state == 2`).
    pub fn secure(&self) -> bool {
        self.state == 2
    }
}

/// A bootstrap packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// `SMSG_ADDON_INFO`: one verdict per addon the auth session named
    /// ([`crate::types::STOCK_SECURE_ADDONS`]), then the server's banned list.
    AddonInfo {
        addons: Vec<AddonVerdict>,
        banned: Vec<BannedAddon>,
    },
    /// `SMSG_CLIENTCACHE_VERSION`: the server's cache version (the client's `WDB` caches answer to
    /// it).
    ClientCacheVersion { version: u32 },
}

impl Packet {
    /// The addons `SMSG_ADDON_INFO` marked secure, which leave the Lua index space; none for the
    /// cache version.
    pub fn secure_addons(&self) -> Vec<String> {
        match self {
            Packet::AddonInfo { addons, .. } => addons
                .iter()
                .filter(|a| a.secure())
                .map(|a| a.name.to_string())
                .collect(),
            Packet::ClientCacheVersion { .. } => Vec::new(),
        }
    }
}

/// The addons an `SMSG_ADDON_INFO` body marked secure, which leave the Lua index space; none if it
/// does not parse (the IO thread reports that as a dropped packet).
pub fn secure_addons(body: &[u8]) -> Vec<String> {
    match decode(SMSG_ADDON_INFO, body).as_deref() {
        Ok([SessionEvent::WrathBootstrap { packet }]) => packet.secure_addons(),
        _ => Vec::new(),
    }
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u32_le(r: &mut &[u8]) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn u8_le(r: &mut &[u8]) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

/// The longest `.url` string the reader takes (`0x5f79a0` reads into 256 bytes).
const MAX_URL: usize = 256;

/// One `SMSG_ADDON_INFO` record, in AzerothCore's `SendAddonsInfo` order: state, key block flag,
/// then (flagged) a key flag, the 256-byte key if that is set, and a `u32`; last a URL flag and
/// (flagged) the URL.
fn verdict(r: &mut &[u8], name: &'static str) -> io::Result<AddonVerdict> {
    let state = u8_le(r)?;
    let (mut public_key, mut key_stamp) = (None, None);
    if u8_le(r)? != 0 {
        if u8_le(r)? != 0 {
            let mut key = Box::new([0; 256]);
            r.read_exact(&mut key[..])?;
            public_key = Some(key);
        }
        key_stamp = Some(u32_le(r)?);
    }
    let url = if u8_le(r)? != 0 {
        let nul = r
            .iter()
            .take(MAX_URL)
            .position(|&b| b == 0)
            .ok_or_else(|| invalid("addon url unterminated or too long"))?;
        let text = String::from_utf8_lossy(&r[..nul]).into_owned();
        *r = &r[nul + 1..];
        Some(text)
    } else {
        None
    };
    Ok(AddonVerdict {
        name,
        state,
        public_key,
        key_stamp,
        url,
    })
}

/// The banned list a sane server could send for us; AzerothCore's own table is a handful.
const MAX_BANNED: u32 = 4096;

/// Decode one bootstrap packet. `SMSG_ADDON_INFO` carries one record per addon the auth session
/// named and no count, so it is decoded against the list ours names
/// ([`crate::types::STOCK_SECURE_ADDONS`]).
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mut r = body;
    let packet = match opcode {
        SMSG_ADDON_INFO => {
            let addons = crate::types::STOCK_SECURE_ADDONS
                .iter()
                .map(|addon| verdict(&mut r, addon.name))
                .collect::<io::Result<Vec<_>>>()?;
            let count = u32_le(&mut r)?;
            if count > MAX_BANNED {
                return Err(invalid("banned addon count out of range"));
            }
            let mut banned = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let id = u32_le(&mut r)?;
                let mut name_md5 = [0; 16];
                r.read_exact(&mut name_md5)?;
                let mut version_md5 = [0; 16];
                r.read_exact(&mut version_md5)?;
                banned.push(BannedAddon {
                    id,
                    name_md5,
                    version_md5,
                    timestamp: u32_le(&mut r)?,
                    banned: u32_le(&mut r)?,
                });
            }
            Packet::AddonInfo { addons, banned }
        }
        SMSG_CLIENTCACHE_VERSION => Packet::ClientCacheVersion {
            version: u32_le(&mut r)?,
        },
        _ => return Err(invalid("not a Wrath bootstrap packet")),
    };
    if !r.is_empty() {
        return Err(invalid("bootstrap packet trailing bytes"));
    }
    Ok(vec![SessionEvent::WrathBootstrap { packet }])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AzerothCore's record for a standard-CRC addon (`SendAddonsInfo`): state 2, key block on,
    /// no key, the zero `u32`, no URL.
    const STANDARD: [u8; 8] = [2, 1, 0, 0, 0, 0, 0, 0];

    fn banned_tail() -> Vec<u8> {
        let mut tail = 1u32.to_le_bytes().to_vec();
        tail.extend(7u32.to_le_bytes());
        tail.extend([0xaa; 16]);
        tail.extend([0xbb; 16]);
        tail.extend(1_700_000_000u32.to_le_bytes());
        tail.extend(1u32.to_le_bytes());
        tail
    }

    #[test]
    fn addon_info_answers_each_stock_addon_then_the_banned_list() {
        let mut body = STANDARD.repeat(23);
        body.extend(banned_tail());
        let [SessionEvent::WrathBootstrap {
            packet: Packet::AddonInfo { addons, banned },
        }] = &decode(SMSG_ADDON_INFO, &body).unwrap()[..]
        else {
            panic!()
        };
        assert_eq!(addons.len(), 23);
        assert!(addons.iter().all(AddonVerdict::secure));
        assert_eq!(secure_addons(&body).len(), 23);
        assert!(
            secure_addons(&body[..body.len() - 1]).is_empty(),
            "unparseable hides nothing"
        );
        assert_eq!(addons[0].name, "Blizzard_AchievementUI");
        assert_eq!(addons[22].name, "Blizzard_TrainerUI");
        assert_eq!(addons[0].key_stamp, Some(0));
        assert!(addons
            .iter()
            .all(|a| a.public_key.is_none() && a.url.is_none()));
        assert_eq!(
            banned,
            &vec![BannedAddon {
                id: 7,
                name_md5: [0xaa; 16],
                version_md5: [0xbb; 16],
                timestamp: 1_700_000_000,
                banned: 1,
            }]
        );
        for end in 0..body.len() {
            assert!(decode(SMSG_ADDON_INFO, &body[..end]).is_err(), "{end}");
        }
        assert!(decode(SMSG_ADDON_INFO, &[body.as_slice(), &[0]].concat()).is_err());
        // A reply that answers nothing (an auth session naming no addon) is not ours.
        assert!(decode(SMSG_ADDON_INFO, &0u32.to_le_bytes()).is_err());
    }

    /// A non-standard CRC brings the 256-byte key, and the reader's other branches: a state that
    /// is not secure, no key block, and a URL.
    #[test]
    fn addon_info_reads_a_key_a_url_and_a_bare_record() {
        let mut body = vec![2, 1, 1];
        body.extend([0x5c; 256]);
        body.extend(9u32.to_le_bytes());
        body.push(0);
        body.extend([1, 0, 1]);
        body.extend(b"http://x\0");
        body.extend(STANDARD.repeat(21));
        body.extend(0u32.to_le_bytes());
        let [SessionEvent::WrathBootstrap {
            packet: Packet::AddonInfo { addons, banned },
        }] = &decode(SMSG_ADDON_INFO, &body).unwrap()[..]
        else {
            panic!()
        };
        assert!(banned.is_empty());
        assert_eq!(addons[0].public_key.as_deref(), Some(&[0x5c; 256]));
        assert_eq!(addons[0].key_stamp, Some(9));
        assert!(addons[0].secure());
        assert_eq!(addons[1].name, "Blizzard_ArenaUI");
        assert!(!addons[1].secure());
        assert_eq!(addons[1].key_stamp, None);
        assert_eq!(addons[1].url.as_deref(), Some("http://x"));
        let packet = Packet::AddonInfo {
            addons: addons.clone(),
            banned: vec![],
        };
        let hidden = packet.secure_addons();
        assert_eq!(hidden.len(), 22, "all but the state-1 record");
        assert!(!hidden.iter().any(|n| n == "Blizzard_ArenaUI"));
        // An unterminated URL fails rather than eating the rest of the packet.
        let mut long = vec![2, 0, 1];
        long.extend([b'a'; 300]);
        assert!(decode(SMSG_ADDON_INFO, &long).is_err());
    }

    #[test]
    fn cache_version_is_one_word() {
        assert!(matches!(
            &decode(SMSG_CLIENTCACHE_VERSION, &3u32.to_le_bytes()).unwrap()[..],
            [SessionEvent::WrathBootstrap {
                packet: Packet::ClientCacheVersion { version: 3 }
            }]
        ));
        assert!(decode(SMSG_CLIENTCACHE_VERSION, &[3, 0, 0]).is_err());
        assert!(decode(SMSG_CLIENTCACHE_VERSION, &[3, 0, 0, 0, 0]).is_err());
    }
}
