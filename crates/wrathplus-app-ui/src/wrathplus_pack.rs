//! The built-in "wrathplus" asset pack: files compiled into the client and mounted below every
//! loose pack in every profile ([`wrathplus_formats::BuiltinPack`]). It rides the boot asset
//! policy, so the `mpq://` reader, the world's chain and the stock UI's chain all serve it, and
//! an enabled pack's file of the same path still wins. Its first content is our login logo,
//! served for the stock glue logo paths (`AccountLoginLogo`'s `file=`, AccountLogin.xml:130, and
//! the classic logo beside it).

use std::sync::{Arc, OnceLock};

use wrathplus_formats::{AssetOverrides, BuiltinPack};

/// The pack's name, as asset-override provenance reports it.
pub const PACK_NAME: &str = "wrathplus";

/// The committed art (512×256 RGBA PNG).
const LOGO_PNG: &[u8] = include_bytes!("../assets/branding/wrathplus-logo.png");

/// The archive paths the logo replaces.
pub const LOGO_PATHS: [&str; 2] = [
    "Interface\\Glues\\Common\\Glues-WoW-WotLKLogo.blp",
    "Interface\\Glues\\Common\\Glues-WoW-Logo.blp",
];

/// The pack's files.
pub fn pack() -> BuiltinPack {
    let logo = logo_blp();
    LOGO_PATHS
        .iter()
        .fold(BuiltinPack::new(PACK_NAME), |p, path| {
            p.with_file(path, logo.clone())
        })
}

/// `overrides` with the pack mounted below every loose pack.
pub fn mount(overrides: AssetOverrides) -> AssetOverrides {
    overrides.with_builtin_pack(pack())
}

/// The logo as the BLP every texture consumer already decodes, encoded once.
pub fn logo_blp() -> Arc<[u8]> {
    static BLP: OnceLock<Arc<[u8]>> = OnceLock::new();
    BLP.get_or_init(|| {
        let image = image::load_from_memory_with_format(LOGO_PNG, image::ImageFormat::Png)
            .expect("the committed logo is a PNG")
            .to_rgba8();
        encode_raw3(image.width(), image.height(), image.as_raw()).into()
    })
    .clone()
}

/// A one-level BLP2 of uncompressed BGRA texels (compression 3, 8-bit alpha), the layout
/// `wrathplus_blp`'s Raw3 decoder reads: the 148-byte header, an unused 256-entry palette, then
/// level 0.
fn encode_raw3(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    const HEADER: usize = 148;
    const PALETTE: usize = 256 * 4;
    let mut out = Vec::with_capacity(HEADER + PALETTE + rgba.len());
    out.extend_from_slice(b"BLP2");
    out.extend_from_slice(&1u32.to_le_bytes()); // content: direct
    out.extend_from_slice(&[3, 8, 8, 0]); // compression, alpha bits, alpha type, no mips
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    let mut offsets = [0u32; 16];
    let mut sizes = [0u32; 16];
    offsets[0] = (HEADER + PALETTE) as u32;
    sizes[0] = rgba.len() as u32;
    offsets
        .iter()
        .chain(&sizes)
        .for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
    out.resize(HEADER + PALETTE, 0);
    for &[r, g, b, a] in rgba.as_chunks::<4>().0 {
        out.extend_from_slice(&[b, g, r, a]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_round_trips_through_the_blp_decoder() {
        let png = image::load_from_memory(LOGO_PNG).unwrap().to_rgba8();
        let decoded = wrathplus_blp::decode(&logo_blp()).expect("a BLP the decoder reads");
        assert_eq!((decoded.width, decoded.height), png.dimensions());
        assert_eq!(decoded.mips[0].rgba, png.as_raw().as_slice());
    }

    #[test]
    fn the_pack_serves_both_logo_paths_and_a_loose_pack_still_wins() {
        let chain = wrathplus_formats::Chain::from_overrides(
            mount(AssetOverrides::default()),
            wrathplus_formats::DataProfile::default(),
        );
        for path in LOGO_PATHS {
            let any_case = path.to_ascii_lowercase().replace('\\', "/");
            assert_eq!(chain.read(&any_case).unwrap(), &logo_blp()[..]);
        }
        assert!(
            chain.override_path(LOGO_PATHS[0]).is_none(),
            "no file to stream"
        );

        let root =
            std::env::temp_dir().join(format!("wrathplus-brand-pack-{}", std::process::id()));
        let dir = root.join("Interface/Glues/Common");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Glues-WoW-WotLKLogo.blp"), b"pack art").unwrap();
        let packed = mount(AssetOverrides::from_roots([root.clone()]).unwrap());
        let chain = wrathplus_formats::Chain::from_overrides(
            packed,
            wrathplus_formats::DataProfile::default(),
        );
        assert_eq!(
            chain.read(LOGO_PATHS[0]).unwrap(),
            b"pack art",
            "the pack wins"
        );
        assert_eq!(chain.read(LOGO_PATHS[1]).unwrap(), &logo_blp()[..]);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod installed_tests {
    use super::*;

    /// The stock login screen's logo (`AccountLoginLogo`, AccountLogin.xml:130) names a path the
    /// built-in pack serves: the installed chain with the pack mounted reads our BLP for it, where
    /// the archives alone read Blizzard's.
    #[test]
    fn the_stock_account_login_logo_resolves_to_our_asset() {
        let data = wrathplus_formats::wow_data_or_skip!();
        let stock = wrathplus_formats::open_configured_chain(&data).expect("the installed chain");
        let xml = stock
            .read("Interface\\GlueXML\\AccountLogin.xml")
            .expect("AccountLogin.xml");
        let xml = String::from_utf8_lossy(&xml);
        let tag = xml
            .split('<')
            .find(|t| t.contains("name=\"AccountLoginLogo\""))
            .expect("the AccountLoginLogo texture");
        let file = tag
            .split("file=\"")
            .nth(1)
            .and_then(|r| r.split('"').next());
        let path = format!("{}.blp", file.expect("its file attribute"));
        assert!(
            LOGO_PATHS.iter().any(|p| p.eq_ignore_ascii_case(&path)),
            "{path}"
        );
        let original = stock.read(&path).expect("the stock logo");
        assert_ne!(original, &logo_blp()[..]);
        let branded = wrathplus_formats::open_configured_chain(&data)
            .unwrap()
            .with_overrides(mount(AssetOverrides::default()));
        assert_eq!(branded.read(&path).unwrap(), &logo_blp()[..]);
    }
}
