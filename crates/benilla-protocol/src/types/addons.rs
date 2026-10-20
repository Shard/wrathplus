//! The addon-info block ending `CMSG_AUTH_SESSION`, written by `0x51d910` after the 20-byte
//! proof: a `u32` uncompressed size, then a zlib (RFC1950) stream to the end of the packet.
//! Uncompressed, it is one record per addon whose `.toc` sets `## Secure:` non-zero, enabled or
//! not, with no count or trailer: `CString` name, `u8` flags (`.pub` byte 0), `u32` CRC-32 of the
//! 256 modulus bytes (`.pub` bytes 1..=256), `u32` CRC-32 of the `.url` string (0 if none).
//!
//! With no secure addons the client appends nothing. Both emulators refuse a zero size: vmangos
//! then skips `SMSG_ADDON_INFO` (`WorldSocket.cpp:447`), cmangos-classic kicks the session
//! (`WorldSocket.cpp:562`).

/// CRC-32 of the stock Blizzard public-key modulus, the emulators' "standard addon CRC"; a server
/// seeing it does not send the 256-byte modulus back in `SMSG_ADDON_INFO`.
pub const STANDARD_MODULUS_CRC: u32 = 0x4C1C_776D;

/// One record of the addon block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecureAddon<'a> {
    /// The addon's folder name (`Blizzard_AuctionUI`), sent as a `CString`.
    pub name: &'a str,
    /// Byte 0 of the `.pub`: 1 for a stock signature, 0 for none usable. A server may write another
    /// back in `SMSG_ADDON_INFO`, which the reference saves to the `.pub` and echoes next logon.
    pub flags: u8,
    /// CRC-32 of the `.pub`'s 256 modulus bytes; 0 when `flags` is 0.
    pub modulus_crc: u32,
    /// CRC-32 of the addon's `.url` string. `0` for every stock addon (none ship one).
    pub url_crc: u32,
}

impl SecureAddon<'_> {
    /// A stock signed built-in, the shape all twelve take on an unmodified install.
    const fn stock(name: &str) -> SecureAddon<'_> {
        SecureAddon {
            name,
            flags: 1,
            modulus_crc: STANDARD_MODULUS_CRC,
            url_crc: 0,
        }
    }
}

/// The twelve `Blizzard_*` built-ins a stock 1.12.1 install reports, in the client's order
/// (ascending ASCII). Deviation: a fixed list, not read off the install, because no third-party
/// addon sets `## Secure:`, so the two differ only where the `.pub` files were altered.
pub const STOCK_SECURE_ADDONS: [SecureAddon<'static>; 12] = [
    SecureAddon::stock("Blizzard_AuctionUI"),
    SecureAddon::stock("Blizzard_BattlefieldMinimap"),
    SecureAddon::stock("Blizzard_BindingUI"),
    SecureAddon::stock("Blizzard_CombatText"),
    SecureAddon::stock("Blizzard_CraftUI"),
    SecureAddon::stock("Blizzard_GMSurveyUI"),
    SecureAddon::stock("Blizzard_InspectUI"),
    SecureAddon::stock("Blizzard_MacroUI"),
    SecureAddon::stock("Blizzard_RaidUI"),
    SecureAddon::stock("Blizzard_TalentUI"),
    SecureAddon::stock("Blizzard_TradeSkillUI"),
    SecureAddon::stock("Blizzard_TrainerUI"),
];

/// The uncompressed addon buffer: the records concatenated, nothing else.
pub fn addon_block(addons: &[SecureAddon]) -> Vec<u8> {
    let mut out = Vec::with_capacity(addons.iter().map(|a| a.name.len() + 10).sum());
    for addon in addons {
        out.extend_from_slice(addon.name.as_bytes());
        out.push(0);
        out.push(addon.flags);
        out.extend_from_slice(&addon.modulus_crc.to_le_bytes());
        out.extend_from_slice(&addon.url_crc.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The block has retail's 342-byte length and parses as twelve sorted, stock-signed records.
    #[test]
    fn stock_block_has_the_retail_size_and_twelve_sorted_stock_records() {
        let plain = addon_block(&STOCK_SECURE_ADDONS);
        assert_eq!(plain.len(), 342, "retail sent 342 uncompressed bytes");

        // Parse it as a server does; landing exactly on the end proves no count and no trailer.
        let mut rest = &plain[..];
        let mut seen = Vec::new();
        while !rest.is_empty() {
            let nul = rest
                .iter()
                .position(|&b| b == 0)
                .expect("record has a name");
            let name = std::str::from_utf8(&rest[..nul]).expect("name is utf-8");
            rest = &rest[nul + 1..];
            let (flags, tail) = rest.split_first().expect("record has flags");
            let modulus_crc = u32::from_le_bytes(tail[0..4].try_into().unwrap());
            let url_crc = u32::from_le_bytes(tail[4..8].try_into().unwrap());
            assert_eq!(*flags, 1, "{name} is a stock signed addon");
            assert_eq!(modulus_crc, STANDARD_MODULUS_CRC, "{name} modulus crc");
            assert_eq!(url_crc, 0, "{name} ships no .url");
            seen.push(name.to_string());
            rest = &tail[8..];
        }
        assert_eq!(seen.len(), 12);
        assert!(seen.iter().all(|n| n.starts_with("Blizzard_")));
        let mut sorted = seen.clone();
        sorted.sort();
        assert_eq!(
            seen, sorted,
            "the client sends them in ascending ASCII order"
        );
    }
}
