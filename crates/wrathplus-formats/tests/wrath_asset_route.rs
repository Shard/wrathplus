//! Read-only effective-enUS route through the production format adapters. No GPU/client launch.
//! Visits waypoint tiles (not a continuous walking simulation), all their placed models, all WMO
//! groups and doodad sets, required external animations, and resolved texture dependencies.
use anyhow::{Context, Result};
use wrathplus_formats::*;
use std::{collections::BTreeSet, path::Path};

#[derive(Default)]
struct Route {
    models: BTreeSet<String>,
    buildings: BTreeSet<String>,
    textures: BTreeSet<String>,
    errors: Vec<String>,
    groups: usize,
    ribbons: usize,
    lights: usize,
    stages: usize,
}
impl Route {
    fn texture(&mut self, chain: &mut Chain, path: &str) -> Result<()> {
        if self.textures.insert(path.to_ascii_lowercase()) {
            blp_bytes_to_native_chain(&chain.read_file(path)?)
                .with_context(|| format!("texture {path}"))?;
        }
        Ok(())
    }
    fn meshes(&mut self, chain: &mut Chain, meshes: &[RenderSubmesh]) -> Result<()> {
        for mesh in meshes {
            if let Some(t) = &mesh.texture {
                self.texture(chain, t)?;
            }
            if let Some(stage) = &mesh.stage2 {
                self.stages += 1;
                if let Some(t) = &stage.texture {
                    self.texture(chain, t)?;
                }
            }
        }
        Ok(())
    }
    fn model(&mut self, chain: &mut Chain, raw: &str) -> Result<()> {
        let lower = raw.replace('/', "\\").to_ascii_lowercase();
        let path = if lower.ends_with(".mdx") || lower.ends_with(".mdl") {
            format!("{}.m2", &lower[..lower.len() - 4])
        } else {
            lower
        };
        if !self.models.insert(path.clone()) {
            return Ok(());
        }
        let bytes = chain.read_file(&path).with_context(|| path.clone())?;
        let stem = path.strip_suffix(".m2").context("M2 extension")?;
        let skin = chain.read_file(&format!("{stem}00.skin"))?;
        let dependencies = wrath_render_animation_dependencies(&bytes)?;
        let mut external = vec![
            None;
            dependencies
                .iter()
                .map(|d| d.sequence_index + 1)
                .max()
                .unwrap_or(0)
        ];
        for d in dependencies {
            external[d.sequence_index] = Some(chain.read_file(&format!(
                "{stem}{:04}-{:02}.anim",
                d.animation_id, d.variation_index
            ))?);
        }
        let refs = external.iter().map(|b| b.as_deref()).collect::<Vec<_>>();
        let dir = path.rsplit_once('\\').map_or("", |(d, _)| d);
        let model = parse_wrath_render_model_with_animations(&bytes, &skin, &refs, dir, &[])?;
        self.meshes(chain, &model.submeshes)?;
        self.lights += model.lights.len();
        self.ribbons += model.ribbons.len();
        for ribbon in &model.ribbons {
            if let Some(t) = &ribbon.texture {
                self.texture(chain, t)?;
            }
        }
        Ok(())
    }
    fn building(&mut self, chain: &mut Chain, path: &str) -> Result<()> {
        let path = path.to_ascii_lowercase();
        if !self.buildings.insert(path.clone()) {
            return Ok(());
        }
        let root = parse_wmo_root(&chain.read_file(&path)?)?;
        let stem = path.strip_suffix(".wmo").context("WMO extension")?;
        for group in 0..root.group_count() {
            let name = format!("{stem}_{group:03}.wmo");
            // Deliberately do not use load_wmo, which silently skips unreadable group files.
            let meshes = wmo_group_submeshes(
                &chain.read_file(&name).with_context(|| name.clone())?,
                &root,
            )?;
            self.meshes(chain, &meshes)?;
            self.groups += 1;
        }
        for doodad in root.doodads() {
            if let Err(e) = self.model(chain, &doodad.model) {
                self.errors
                    .push(format!("{path} doodad {}: {e:#}", doodad.model));
            }
        }
        Ok(())
    }
}
fn mphd(bytes: &[u8]) -> Result<u32> {
    let mut at = 0;
    while at + 8 <= bytes.len() {
        let n = u32::from_le_bytes(bytes[at + 4..at + 8].try_into()?) as usize;
        let end = at + 8 + n;
        anyhow::ensure!(end <= bytes.len(), "truncated WDT chunk");
        if &bytes[at..at + 4] == b"DHPM" {
            return Ok(u32::from_le_bytes(bytes[at + 8..at + 12].try_into()?));
        }
        at = end;
    }
    anyhow::bail!("missing MPHD")
}
#[test]
fn installed_wrath_asset_route() -> Result<()> {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return Ok(());
    }
    let mut chain = Chain::open_wrath(Path::new(&std::env::var("WOW_DATA")?), "enUS")?;
    let mut route = Route::default();
    let mut tiles = BTreeSet::new();
    for (label, map, x, y) in [
        ("Northshire", "Azeroth", -8949.95, -132.49),
        ("Goldshire", "Azeroth", -9464.0, 62.0),
        ("Stormwind", "Azeroth", -8840.56, 489.7),
        ("ValianceKeep", "Northrend", 2275.0, 5173.0),
        ("HowlingFjord", "Northrend", 682.0, -3970.0),
        ("Dalaran", "Northrend", 5807.0, 588.0),
    ] {
        let map_tiles = MapTiles::load(&mut chain, map)?;
        let (tx, ty) = map_tiles.tile_at(x, y);
        if !tiles.insert((map, tx, ty)) {
            continue;
        }
        let flags = mphd(&chain.read_file(&format!("World\\Maps\\{map}\\{map}.wdt"))?)?;
        let path = format!("World\\Maps\\{map}\\{map}_{tx}_{ty}.adt");
        let tile = adt_to_tile_mesh_with_options(
            &chain.read_file(&path)?,
            TerrainDecodeOptions {
                big_alpha: flags & (4 | 128) != 0,
                fix_alpha: true,
                honor_do_not_fix_alpha: true,
            },
        )
        .with_context(|| path.clone())?;
        anyhow::ensure!(!tile.chunks.is_empty(), "{path}: no terrain");
        for chunk in &tile.chunks {
            for texture in &chunk.layer_textures {
                if let Err(e) = route.texture(&mut chain, texture) {
                    route.errors.push(format!("{path}: {e:#}"));
                }
            }
        }
        for d in tile.doodads {
            if let Err(e) = route.model(&mut chain, &d.model) {
                route.errors.push(format!("{path} {}: {e:#}", d.model));
            }
        }
        for w in tile.wmos {
            if let Err(e) = route.building(&mut chain, &w.model) {
                route.errors.push(format!("{path} {}: {e:#}", w.model));
            }
        }
        eprintln!("ROUTE {label} {map}_{tx}_{ty} cumulative models={} buildings={} groups={} textures={} ribbons={} lights={} stage2={} failures={}", route.models.len(),route.buildings.len(),route.groups,route.textures.len(),route.ribbons,route.lights,route.stages,route.errors.len());
    }
    for error in &route.errors {
        eprintln!("ROUTE FAILURE {error}");
    }
    assert!(
        route.errors.is_empty(),
        "{} route load failures (see diagnostics)",
        route.errors.len()
    );
    Ok(())
}
