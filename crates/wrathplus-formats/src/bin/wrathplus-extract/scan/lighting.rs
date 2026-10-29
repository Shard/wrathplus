//! Corpus scans over what lights a model: dark WMO props, M2 light blocks, terrain shadow.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use wrathplus_formats::Chain;

use crate::model_key;

/// List the placed MODD props that the interior lane lights literal black. Its base light is the
/// MODD colour, ambient `cap96`, diffuse `floor112` (`0x694e90` → `0x6a77e0`), and the floor is a
/// hue-preserving scale by `112/max`, so `#000000` stays black and only the owning group's MOLR
/// lights reach the prop. Exterior wins: any exterior referrer makes the prop sky-lit
/// (`0x695aa0`); `RESCUED` counts those an interior group names first.
pub fn darkpropscan(chain: &mut Chain, prefix: Option<&str>) -> Result<()> {
    let roots = super::wmo_roots(chain, prefix)?;

    let (mut roots_scanned, mut modds_total, mut zero_colour, mut black, mut dim, mut rescued) =
        (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
    let mut by_model: BTreeMap<String, u32> = BTreeMap::new();
    for root_path in roots {
        let Ok(bytes) = chain.read_file(&root_path) else {
            continue;
        };
        let Ok(root) = wrathplus_formats::parse_wmo_root(&bytes) else {
            continue;
        };
        roots_scanned += 1;
        modds_total += root.doodads().len() as u32;
        if !root.doodads().iter().any(|d| d.color[..3] == [0, 0, 0]) {
            continue;
        }
        let lights = wrathplus_formats::parse_wmo_lights(&bytes);
        let stem = root_path
            .to_ascii_lowercase()
            .strip_suffix(".wmo")
            .unwrap_or(&root_path)
            .to_string();
        // MODD index -> referring groups, and group -> its MOLR light refs.
        let mut refs: BTreeMap<u16, Vec<u32>> = BTreeMap::new();
        let mut light_refs: BTreeMap<u32, Vec<u16>> = BTreeMap::new();
        for gi in 0..root.group_count() {
            let Ok(gbytes) = chain.read_file(&format!("{stem}_{gi:03}.wmo")) else {
                continue;
            };
            for r in wrathplus_formats::wmo_group_doodad_refs(&gbytes) {
                let e = refs.entry(r).or_default();
                if e.last() != Some(&gi) {
                    e.push(gi);
                }
            }
            light_refs.insert(gi, wrathplus_formats::wmo_group_light_refs(&gbytes));
        }

        let infos = root.group_infos();
        let mut printed_header = false;
        for (i, d) in root.doodads().iter().enumerate() {
            if d.color[..3] != [0, 0, 0] {
                continue;
            }
            zero_colour += 1;
            let Some(referrers) = refs.get(&(i as u16)) else {
                continue; // no group names it: lit as exterior
            };
            let owner = referrers[0];
            // Exterior wins over every interior referrer.
            if !referrers
                .iter()
                .all(|g| infos.get(*g as usize).is_some_and(|gi| gi.interior))
            {
                if infos.get(owner as usize).is_some_and(|g| g.interior) {
                    rescued += 1;
                }
                continue;
            }
            // The owning group's MOLR omni lights in range of the prop's origin; the spawn measures
            // from the M2's bounds reference point, within a model radius of it.
            let in_range = light_refs
                .get(&owner)
                .map(|ls| {
                    ls.iter()
                        .filter_map(|&li| lights.get(li as usize))
                        .filter(|l| l.is_omni() && l.attenuation_end > l.attenuation_start)
                        .filter(|l| {
                            let dv = [
                                l.position[0] - d.position[0],
                                l.position[1] - d.position[1],
                                l.position[2] - d.position[2],
                            ];
                            dv.iter().map(|c| c * c).sum::<f32>().sqrt() < l.attenuation_end
                        })
                        .count()
                })
                .unwrap_or(0);
            if in_range == 0 {
                black += 1;
                *by_model.entry(model_key(&d.model)).or_default() += 1;
            } else {
                dim += 1;
            }
            if !printed_header {
                println!("{root_path}");
                printed_header = true;
            }
            let ref_cell = referrers
                .iter()
                .map(|g| format!("g{g}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!(
                "  modd {i:>5}  {verdict:<5}  pos ({:>8.2}, {:>8.2}, {:>8.2})  molr {in_range} in range  refs(INT) {ref_cell}  {}",
                d.position[0],
                d.position[1],
                d.position[2],
                model_key(&d.model),
                verdict = if in_range == 0 { "BLACK" } else { "dim" },
            );
        }
    }

    eprintln!(
        "{roots_scanned} root(s), {modds_total} MODD(s): {zero_colour} carry colour #000000; \
         of those, interior-ONLY = {} — {black} BLACK (no MOLR light in range), {dim} dim \
         (a fixture light reaches them). RESCUED by exterior-wins: {rescued} (an interior group \
         names them first, an exterior group also names them — sky-lit).",
        black + dim,
    );
    if !by_model.is_empty() {
        eprintln!("BLACK props by model ({} distinct):", by_model.len());
        let mut rows: Vec<_> = by_model.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (m, n) in rows {
            eprintln!("  {n:>4}  {m}");
        }
    }
    Ok(())
}

/// Print the terrain MCSH shadow bit, which sets a doodad's sun gain (2.5 lit, 0.5 shadowed), at
/// a world position and the ±8 texels around it (`#` shadowed, `.` lit, `?` no chunk); a texel is
/// `TILE_SIZE / 1024`, about 0.52 yd.
pub fn shadeat(chain: &mut Chain, map: &str, x: f32, y: f32) -> Result<()> {
    let tiles = wrathplus_formats::load_tiles_around(chain, map, x, y, 0)
        .with_context(|| format!("loading the tile under ({x}, {y}) on {map}"))?;
    let Some((_, tile)) = tiles.first() else {
        anyhow::bail!("no tile exists under ({x}, {y}) on {map}");
    };
    let texel = wrathplus_formats::TILE_SIZE / 1024.0;
    let word = |s: Option<bool>| match s {
        Some(true) => "SHADOWED (doodad sun intensity 0.5)",
        Some(false) => "lit (doodad sun intensity 2.5)",
        None => "off-tile / no chunk",
    };
    println!(
        "MCSH at ({x:.2}, {y:.2}): {}",
        word(wrathplus_formats::mcsh_shadowed_at(
            &tile.chunks,
            [x, y, 0.0]
        ))
    );
    println!(
        "neighborhood, texel {texel:.3} yd — rows +X (north) up, cols +Y (west) left; center marked:"
    );
    for dx in (-8i32..=8).rev() {
        let mut row = String::new();
        for dy in (-8i32..=8).rev() {
            let p = [x + dx as f32 * texel, y + dy as f32 * texel, 0.0];
            let mut c = match wrathplus_formats::mcsh_shadowed_at(&tile.chunks, p) {
                Some(true) => '#',
                Some(false) => '.',
                None => '?',
            };
            if dx == 0 && dy == 0 {
                c = if c == '#' { 'S' } else { 'O' };
            }
            row.push(c);
        }
        println!("{row}");
    }
    Ok(())
}
