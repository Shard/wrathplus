//! Read-only Wrath chain probe. Prints metadata only; never prints proprietary file contents.

use std::path::Path;

use anyhow::{bail, Context};
use wrathplus_formats::Chain;

const SENTINELS: &[(&str, &str)] = &[
    ("DBC", "DBFilesClient\\Map.dbc"),
    ("WDT", "World\\Maps\\Azeroth\\Azeroth.wdt"),
    ("BLP", "Interface\\Icons\\INV_Misc_QuestionMark.blp"),
    ("M2", "Character\\Human\\Male\\HumanMale.m2"),
    ("SKIN", "Character\\Human\\Male\\HumanMale00.skin"),
    ("FrameXML", "Interface\\FrameXML\\FrameXML.toc"),
];

fn magic(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let data = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: wrath_inspect <Data path> <locale>"))?;
    let locale = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: wrath_inspect <Data path> <locale>"))?;
    if args.next().is_some() {
        bail!("usage: wrath_inspect <Data path> <locale>");
    }
    let locale = locale.to_str().context("locale must be valid UTF-8")?;
    let chain = Chain::open_wrath(Path::new(&data), locale)?;

    for (kind, name) in SENTINELS {
        let archive = match chain.find_file_archive(name) {
            Some(path) => path,
            None => {
                println!("{kind}: error=file not in chain");
                continue;
            }
        };
        let archive_name = archive
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<non-UTF-8 archive name>");
        match chain.read(name) {
            Ok(bytes) => {
                println!(
                    "{kind}: size={} magic={} archive={archive_name}",
                    bytes.len(),
                    magic(&bytes)
                );
                if kind == &"M2" {
                    match wrathplus_m2::wrath::parse_wrath_m2(&mut std::io::Cursor::new(
                        bytes.as_slice(),
                    )) {
                        Ok(_) => println!("M2: Wrath semantic parser accepted"),
                        Err(error) => println!("M2: semantic parser error={error}"),
                    }
                }
                if kind == &"SKIN" {
                    match wrathplus_m2::wrath::parse_wrath_skin(&bytes) {
                        Ok(_) => println!("SKIN: Wrath semantic parser accepted"),
                        Err(error) => println!("SKIN: semantic parser error={error}"),
                    }
                }
            }
            Err(error) => println!("{kind}: archive={archive_name} error={error:#}"),
        }
    }
    let model = chain.read("Character\\Human\\Male\\HumanMale.m2")?;
    let skin = chain.read("Character\\Human\\Male\\HumanMale00.skin")?;
    let rendered = wrathplus_formats::parse_wrath_render_model(&model, &skin, "", &[])?;
    println!(
        "Wrath render adapter: batches={} bones={} animations={} cameras={}",
        rendered.submeshes.len(),
        rendered.skeleton.bones.len(),
        rendered.animations.len(),
        rendered.cameras.len()
    );
    let wdt = chain.read("World\\Maps\\Azeroth\\Azeroth.wdt")?;
    let mut remaining = wdt.as_slice();
    let mut flags = None;
    while !remaining.is_empty() {
        anyhow::ensure!(remaining.len() >= 8, "truncated WDT header");
        let size = u32::from_le_bytes(remaining[4..8].try_into()?) as usize;
        anyhow::ensure!(size <= remaining.len() - 8, "truncated WDT payload");
        if &remaining[..4] == b"DHPM" {
            anyhow::ensure!(size >= 4, "short MPHD flags");
            flags = Some(u32::from_le_bytes(remaining[8..12].try_into()?));
        }
        remaining = &remaining[8 + size..];
    }
    let flags = flags.context("missing MPHD")?;
    let parsed = wrathplus_formats::WdtReader::new(
        std::io::Cursor::new(wdt),
        wrathplus_formats::WowVersion::Classic,
    )
    .read()?;
    let (x, y) = wrathplus_formats::world_to_tile(-8949.95, -132.49);
    anyhow::ensure!(
        parsed
            .get_tile(x as usize, y as usize)
            .is_some_and(|tile| tile.has_adt),
        "Northshire tile missing"
    );
    let tile = chain.read(&format!("World\\Maps\\Azeroth\\Azeroth_{x}_{y}.adt"))?;
    let mesh = wrathplus_formats::adt_to_tile_mesh_with_options(
        &tile,
        wrathplus_formats::TerrainDecodeOptions {
            big_alpha: flags & (0x4 | 0x80) != 0,
            fix_alpha: true,
            honor_do_not_fix_alpha: true,
        },
    )?;
    println!(
        "Northshire terrain: tile={x},{y} chunks={} MH2O={}",
        mesh.chunks.len(),
        mesh.mh2o.is_some()
    );
    let mut chain = chain;
    let maps = wrathplus_formats::load_map_catalog(&mut chain)?;
    println!(
        "Wrath Map: map0 directory={:?} name={:?}",
        maps.directory(0),
        maps.name(0)
    );
    let _areas = wrathplus_formats::load_area_table_catalog(&mut chain)?;
    let _lights = wrathplus_formats::LightCatalog::load(&mut chain)?;
    println!("Wrath Area/Light catalogs parsed");
    Ok(())
}
