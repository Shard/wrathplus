//! The addon-info block ending `CMSG_AUTH_SESSION`, written by 12340's `0x5f5440` after the
//! 20-byte proof: a `u32` uncompressed size, then a zlib (RFC1950) stream to the end of the packet.
//! Uncompressed, it is a `u32` count, then one record per addon whose `.toc` sets `## Secure:`
//! non-zero (`0x5f86a0` sets the record's sent byte), enabled or not: `CString` name, `u8` flags
//! (`.pub` byte 0), `u32` CRC-32 of the 256 modulus bytes (`.pub` bytes 1..=256), `u32` CRC-32 of
//! the `.url` string (0 if none). A `u32` trailer follows: the newest timestamp in the client's
//! banned-addon cache (`0x5f3960`), 0 with no cache. AzerothCore reads the same layout
//! (`WorldSession::ReadAddonsInfo`) and only logs the trailer.

/// CRC-32 of the stock Blizzard public-key modulus, AzerothCore's `STANDARD_ADDON_CRC`; a server
/// seeing it does not send the 256-byte modulus back in `SMSG_ADDON_INFO`.
pub const STANDARD_MODULUS_CRC: u32 = 0x4C1C_776D;

/// One record of the addon block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecureAddon<'a> {
    /// The addon's folder name (`Blizzard_AuctionUI`), sent as a `CString`.
    pub name: &'a str,
    /// Byte 0 of the `.pub`: 1 for a stock signature, 0 for none usable.
    pub flags: u8,
    /// CRC-32 of the `.pub`'s 256 modulus bytes; 0 when `flags` is 0.
    pub modulus_crc: u32,
    /// CRC-32 of the addon's `.url` string. `0` for every stock addon (none ship one).
    pub url_crc: u32,
}

impl SecureAddon<'_> {
    /// A stock signed built-in, the shape all twenty-three take on an unmodified install.
    const fn stock(name: &str) -> SecureAddon<'_> {
        SecureAddon {
            name,
            flags: 1,
            modulus_crc: STANDARD_MODULUS_CRC,
            url_crc: 0,
        }
    }
}

/// The twenty-three `Blizzard_*` built-ins whose 3.3.5 `.toc` says `## Secure: 1`, in ascending
/// ASCII order. Deviation: a fixed list, not read off the install, because no third-party addon
/// sets `## Secure:`, so the two differ only where the `.pub` files were altered. The order is
/// ours to pick: the server answers record for record in the order it read.
pub const STOCK_SECURE_ADDONS: [SecureAddon<'static>; 23] = [
    SecureAddon::stock("Blizzard_AchievementUI"),
    SecureAddon::stock("Blizzard_ArenaUI"),
    SecureAddon::stock("Blizzard_AuctionUI"),
    SecureAddon::stock("Blizzard_BarbershopUI"),
    SecureAddon::stock("Blizzard_BattlefieldMinimap"),
    SecureAddon::stock("Blizzard_BindingUI"),
    SecureAddon::stock("Blizzard_Calendar"),
    SecureAddon::stock("Blizzard_CombatLog"),
    SecureAddon::stock("Blizzard_CombatText"),
    SecureAddon::stock("Blizzard_DebugTools"),
    SecureAddon::stock("Blizzard_GMChatUI"),
    SecureAddon::stock("Blizzard_GMSurveyUI"),
    SecureAddon::stock("Blizzard_GlyphUI"),
    SecureAddon::stock("Blizzard_GuildBankUI"),
    SecureAddon::stock("Blizzard_InspectUI"),
    SecureAddon::stock("Blizzard_ItemSocketingUI"),
    SecureAddon::stock("Blizzard_MacroUI"),
    SecureAddon::stock("Blizzard_RaidUI"),
    SecureAddon::stock("Blizzard_TalentUI"),
    SecureAddon::stock("Blizzard_TimeManager"),
    SecureAddon::stock("Blizzard_TokenUI"),
    SecureAddon::stock("Blizzard_TradeSkillUI"),
    SecureAddon::stock("Blizzard_TrainerUI"),
];

/// The uncompressed addon buffer: the count, the records, then the banned-cache timestamp.
pub fn addon_block(addons: &[SecureAddon], banned_cache_stamp: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + addons.iter().map(|a| a.name.len() + 10).sum::<usize>());
    out.extend_from_slice(&(addons.len() as u32).to_le_bytes());
    for addon in addons {
        out.extend_from_slice(addon.name.as_bytes());
        out.push(0);
        out.push(addon.flags);
        out.extend_from_slice(&addon.modulus_crc.to_le_bytes());
        out.extend_from_slice(&addon.url_crc.to_le_bytes());
    }
    out.extend_from_slice(&banned_cache_stamp.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The block parses as AzerothCore's `ReadAddonsInfo` reads it: a count, twenty-three sorted
    /// stock-signed records, and the trailer landing exactly on the end.
    #[test]
    fn stock_block_reads_as_twenty_three_sorted_stock_records() {
        let plain = addon_block(&STOCK_SECURE_ADDONS, 0);
        let names: usize = STOCK_SECURE_ADDONS.iter().map(|a| a.name.len()).sum();
        assert_eq!(plain.len(), 4 + names + 23 * 10 + 4);

        assert_eq!(&plain[4..27], b"Blizzard_AchievementUI\0");
        assert_eq!(&plain[27..36], &[1, 0x6d, 0x77, 0x1c, 0x4c, 0, 0, 0, 0]);
        let (count, mut rest) = plain.split_at(4);
        assert_eq!(u32::from_le_bytes(count.try_into().unwrap()), 23);
        let mut seen = Vec::new();
        for _ in 0..23 {
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
        assert_eq!(rest, 0u32.to_le_bytes(), "the banned-cache trailer ends it");
        assert!(seen.iter().all(|n| n.starts_with("Blizzard_")));
        let mut sorted = seen.clone();
        sorted.sort();
        assert_eq!(seen, sorted, "ascending ASCII order");
    }
}
