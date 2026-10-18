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

/// A bootstrap packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// `SMSG_ADDON_INFO` for an auth session that named no addon: no per-addon records, then the
    /// server's banned list.
    AddonInfo { banned: Vec<BannedAddon> },
    /// `SMSG_CLIENTCACHE_VERSION`: the server's cache version (the client's `WDB` caches answer to
    /// it).
    ClientCacheVersion { version: u32 },
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u32_le(r: &mut &[u8]) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

/// The banned list a sane server could send for us; AzerothCore's own table is a handful.
const MAX_BANNED: u32 = 4096;

/// Decode one bootstrap packet. `SMSG_ADDON_INFO` carries one record per addon the auth session
/// named and no count, so it is decoded for our auth session, which names none
/// (`wrath_world::empty_addon_block`).
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mut r = body;
    let packet = match opcode {
        SMSG_ADDON_INFO => {
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
            Packet::AddonInfo { banned }
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

    #[test]
    fn addon_info_is_the_banned_list_for_an_empty_auth_list() {
        let mut body = 1u32.to_le_bytes().to_vec();
        body.extend(7u32.to_le_bytes());
        body.extend([0xaa; 16]);
        body.extend([0xbb; 16]);
        body.extend(1_700_000_000u32.to_le_bytes());
        body.extend(1u32.to_le_bytes());
        let [SessionEvent::WrathBootstrap {
            packet: Packet::AddonInfo { banned },
        }] = &decode(SMSG_ADDON_INFO, &body).unwrap()[..]
        else {
            panic!()
        };
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
        // AzerothCore's usual reply: no banned addon.
        assert!(matches!(
            &decode(SMSG_ADDON_INFO, &0u32.to_le_bytes()).unwrap()[..],
            [SessionEvent::WrathBootstrap { packet: Packet::AddonInfo { banned } }] if banned.is_empty()
        ));
        for end in 0..body.len() {
            assert!(decode(SMSG_ADDON_INFO, &body[..end]).is_err(), "{end}");
        }
        assert!(decode(SMSG_ADDON_INFO, &[body.as_slice(), &[0]].concat()).is_err());
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
