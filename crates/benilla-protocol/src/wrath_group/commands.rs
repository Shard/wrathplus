//! Build-12340 raid command bodies, without client-side policy or optimistic state changes.
use std::io;

pub const CMSG_LOOT_METHOD: u32 = 0x07a;
pub const CMSG_GROUP_CHANGE_SUB_GROUP: u32 = 0x27e;
pub const CMSG_GROUP_SWAP_SUB_GROUP: u32 = 0x280;
pub const CMSG_GROUP_RAID_CONVERT: u32 = 0x28e;
pub const CMSG_GROUP_ASSISTANT_LEADER: u32 = 0x28f;
pub const MSG_RAID_TARGET_UPDATE: u32 = 0x321;
pub const MSG_RAID_READY_CHECK: u32 = 0x322;

fn name(value: &str) -> io::Result<Vec<u8>> {
    if value.as_bytes().contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"));
    }
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    Ok(bytes)
}
pub fn loot_method(method: u32, master: u64, threshold: u32) -> Vec<u8> {
    let mut bytes = method.to_le_bytes().to_vec();
    bytes.extend(master.to_le_bytes());
    bytes.extend(threshold.to_le_bytes());
    bytes
}
pub fn raid_target(icon: u8, guid: u64) -> Vec<u8> {
    let mut bytes = vec![icon];
    // 0xff is the server's full-list request, not an icon assignment.
    if icon != 0xff {
        bytes.extend(guid.to_le_bytes());
    }
    bytes
}
pub fn change_subgroup(member: &str, subgroup: u8) -> io::Result<Vec<u8>> {
    let mut bytes = name(member)?;
    bytes.push(subgroup);
    Ok(bytes)
}
pub fn swap_subgroup(first: &str, second: &str) -> io::Result<Vec<u8>> {
    let mut bytes = name(first)?;
    bytes.extend(name(second)?);
    Ok(bytes)
}
pub fn assistant(guid: u64, grant: bool) -> Vec<u8> {
    let mut bytes = guid.to_le_bytes().to_vec();
    bytes.push(u8::from(grant));
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_native_bodies_preserve_widths_order_and_clear_edges() {
        assert_eq!(
            loot_method(4, 0x0807060504030201, 6),
            [4, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 6, 0, 0, 0]
        );
        assert_eq!(
            raid_target(7, 0x0807060504030201),
            [7, 1, 2, 3, 4, 5, 6, 7, 8]
        );
        assert_eq!(raid_target(0xff, u64::MAX), [0xff]);
        assert_eq!(raid_target(0, 0), [0; 9]);
        assert_eq!(change_subgroup("Ally", 7).unwrap(), b"Ally\0\x07");
        assert_eq!(swap_subgroup("Ally", "Other").unwrap(), b"Ally\0Other\0");
        assert_eq!(
            assistant(0x0807060504030201, true),
            [1, 2, 3, 4, 5, 6, 7, 8, 1]
        );
        assert_eq!(assistant(0, false), [0; 9]);
        assert!(change_subgroup("A\0B", 0).is_err());
        assert!(swap_subgroup("A", "B\0C").is_err());
        assert!(swap_subgroup("A\0B", "C").is_err());
    }
}
