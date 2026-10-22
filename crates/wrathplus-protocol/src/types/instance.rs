//! `SMSG_RAID_GROUP_ONLY`, the warning that a raid instance wants a raid group. 1.12's lockout
//! messages (`SMSG_RAID_INSTANCE_MESSAGE`, `SMSG_INSTANCE_SAVE_CREATED`, `SMSG_INSTANCE_RESET`,
//! `SMSG_INSTANCE_RESET_FAILED`, `SMSG_UPDATE_LAST_INSTANCE`, `SMSG_UPDATE_INSTANCE_OWNERSHIP`) are
//! not here: 3.3.5's instance control (`wrath_instance_control`) carries them.

/// `SMSG_RAID_GROUP_ONLY` (reference `0x5e48f1`): a nonzero `delay_ms` arms the instance-boot
/// deadline (`INSTANCE_BOOT_START`); `0` clears it (`INSTANCE_BOOT_STOP`), and then `reason` 1 or
/// 2 shows `ERR_RAID_GROUP_ONLY` or `ERR_RAID_GROUP_FULL`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaidGroupOnly {
    pub delay_ms: u32,
    pub reason: u32,
}
