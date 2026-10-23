//! Corpus scan over how a batch is blended.

use anyhow::Result;
use wrathplus_formats::Chain;

/// List the models whose material table authors blend mode 5 (Mod) or 6 (Mod2x): materials at
/// `0x84`/`0x88`, 4-byte `{u16 flags, u16 blend}` records.
pub fn blendscan(chain: &mut Chain, prefix: Option<&str>) -> Result<()> {
    let names = super::m2_names(chain, prefix)?;
    let (mut scanned, mut hits) = (0u32, 0u32);
    for name in names {
        let Ok(bytes) = chain.read_file(&name) else {
            continue;
        };
        scanned += 1;
        let at = |o: usize| -> Option<u32> {
            Some(u32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?))
        };
        let (Some(n), Some(ofs)) = (at(0x84), at(0x88)) else {
            continue;
        };
        let mats: Vec<(u16, u16)> = (0..n as usize)
            .filter_map(|i| {
                let o = ofs as usize + i * 4;
                let b = bytes.get(o..o + 4)?;
                Some((
                    u16::from_le_bytes([b[0], b[1]]),
                    u16::from_le_bytes([b[2], b[3]]),
                ))
            })
            .collect();
        if mats.iter().any(|&(_, blend)| blend == 5 || blend == 6) {
            hits += 1;
            println!("{mats:?}  {name}");
        }
    }
    eprintln!("{scanned} models scanned, {hits} with Mod/Mod2x materials");
    Ok(())
}
