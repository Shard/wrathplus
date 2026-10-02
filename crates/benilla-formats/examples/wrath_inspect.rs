//! Read-only Wrath chain probe. Prints metadata only; never prints proprietary file contents.

use std::path::Path;

use anyhow::{bail, Context};
use benilla_formats::Chain;

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
            Ok(bytes) => println!(
                "{kind}: size={} magic={} archive={archive_name}",
                bytes.len(),
                magic(&bytes)
            ),
            Err(error) => println!("{kind}: archive={archive_name} error={error:#}"),
        }
    }
    Ok(())
}
