//! WDT loader: a map's 64×64 `MAIN` tile-existence grid, which the terrain streamer checks before
//! requesting an `.adt` (open ocean authors none), and its global WMO, the whole world on a map
//! with no terrain.

use wrathplus_formats::{GlobalWmo, WdtFile, WdtReader, WowVersion};
use bevy::asset::io::Reader;
use bevy::asset::{Asset, AssetLoader, LoadContext};
use bevy::reflect::TypePath;

/// A map's parsed WDT: which of the 64×64 ADT tiles exist.
#[derive(Asset, TypePath)]
pub struct WdtIndex {
    file: WdtFile,
    flags: u32,
}

impl WdtIndex {
    /// Whether tile `(tile_x, tile_y)` has an `.adt` (`MAIN` flag bit 0), in the order of the
    /// `Map_<tile_x>_<tile_y>.adt` names.
    pub fn has_tile(&self, tile_x: u32, tile_y: u32) -> bool {
        self.file
            .get_tile(tile_x as usize, tile_y as usize)
            .is_some_and(|t| t.has_adt)
    }

    /// The single global building of a map with no terrain (`MPHD` bit 0); `None` on an ADT map.
    pub fn global_wmo(&self) -> Option<&GlobalWmo> {
        self.file.global_wmo()
    }

    /// Raw first dword of the WDT's `MPHD` chunk.
    pub fn flags(&self) -> u32 {
        self.flags
    }
}

/// Loads a `*.wdt` into a [`WdtIndex`].
#[derive(Default, TypePath)]
pub struct WdtIndexLoader;

impl AssetLoader for WdtIndexLoader {
    type Asset = WdtIndex;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _ctx: &mut LoadContext<'_>,
    ) -> Result<WdtIndex, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let flags = mphd_flags(&bytes)?;
        let wdt = WdtReader::new(std::io::Cursor::new(bytes), WowVersion::Classic).read()?;
        Ok(WdtIndex { file: wdt, flags })
    }

    fn extensions(&self) -> &[&str] {
        &["wdt"]
    }
}

/// Read the first dword of `MPHD`, rejecting any chunk whose declared payload is truncated.
pub(crate) fn mphd_flags(bytes: &[u8]) -> std::io::Result<u32> {
    let mut offset = 0usize;
    while offset < bytes.len() {
        let header_end = offset.checked_add(8).ok_or_else(truncated_chunk)?;
        let header = bytes.get(offset..header_end).ok_or_else(truncated_chunk)?;
        let size = u32::from_le_bytes(header[4..8].try_into().expect("four-byte chunk size"));
        let payload_end = header_end
            .checked_add(size as usize)
            .ok_or_else(truncated_chunk)?;
        let payload = bytes
            .get(header_end..payload_end)
            .ok_or_else(truncated_chunk)?;
        if &header[..4] == b"DHPM" {
            let flags = payload.get(..4).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "WDT MPHD chunk has no flags dword",
                )
            })?;
            return Ok(u32::from_le_bytes(
                flags.try_into().expect("four-byte MPHD flags"),
            ));
        }
        offset = payload_end;
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "WDT missing MPHD chunk",
    ))
}

fn truncated_chunk() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        "WDT chunk runs past the end of the stream",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The on-disk grid is row-major, `y * 64 + x`.
    #[test]
    fn parses_main_grid_in_tile_index_order() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"REVM"); // magics are reversed on disk
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(&18u32.to_le_bytes());
        buf.extend_from_slice(b"NIAM");
        buf.extend_from_slice(&(64u32 * 64 * 8).to_le_bytes());
        let mut grid = vec![0u8; 64 * 64 * 8];
        for (x, y) in [(3usize, 5usize), (63, 0)] {
            grid[(y * 64 + x) * 8] = 1; // flags bit 0 = has_adt
        }
        buf.extend_from_slice(&grid);

        let wdt = WdtReader::new(std::io::Cursor::new(buf), WowVersion::Classic)
            .read()
            .expect("synthetic WDT parses");
        let index = WdtIndex {
            file: wdt,
            flags: 0,
        };
        assert!(index.has_tile(3, 5));
        assert!(index.has_tile(63, 0));
        assert!(
            !index.has_tile(5, 3),
            "index order is (tile_x, tile_y), not swapped"
        );
        assert!(!index.has_tile(0, 0));
        assert!(
            !index.has_tile(64, 64),
            "out of range is absent, not a panic"
        );
    }

    #[test]
    fn reads_mphd_flags_and_rejects_truncated_chunks() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"REVM");
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(&18u32.to_le_bytes());
        buf.extend_from_slice(b"DHPM");
        buf.extend_from_slice(&8u32.to_le_bytes());
        buf.extend_from_slice(&0x84u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(mphd_flags(&buf).unwrap(), 0x84);

        buf.pop();
        assert_eq!(
            mphd_flags(&buf).unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );

        let short_mphd = [b'D', b'H', b'P', b'M', 3, 0, 0, 0, 1, 2, 3];
        assert_eq!(
            mphd_flags(&short_mphd).unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }
}
