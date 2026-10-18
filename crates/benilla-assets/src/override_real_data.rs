//! Asset packs against the installed Wrath chain: a real pack replaces a real model (M2 with its
//! SKIN/ANIM siblings) and a real terrain texture through the production `mpq://` loaders, while
//! untouched paths fall back to the archives. The pack is assembled from installed files into a
//! temporary folder (or, on demand, a caller's `mods/<pack>`); nothing is written to the repo.

use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::{register_asset_loaders, register_mpq_source_overrides, texture_url, M2Model};
use benilla_formats::{AssetOverrides, Chain, DataProfile};

/// Northshire/Goldshire road lamps, replaced by the Elwynn ballista.
const MODEL_TARGET: &str = "World\\Azeroth\\Elwynn\\PassiveDoodads\\Lamppost\\Lamppost.m2";
const MODEL_SOURCE: &str = "World\\Azeroth\\Elwynn\\PassiveDoodads\\Ballista\\Ballista.m2";
/// An untouched model: must still come from the archives.
const MODEL_FALLBACK: &str = "World\\Azeroth\\Elwynn\\PassiveDoodads\\Campfire\\ElwynnCampfire.m2";

fn wrath() -> Option<(PathBuf, DataProfile, Chain)> {
    let data = benilla_formats::wow_data_or_skip!(None);
    let profile = DataProfile::from_env().expect("valid WOW_CLIENT_PROFILE/WOW_LOCALE");
    if !matches!(profile, DataProfile::Wrath { .. }) {
        eprintln!("skipped: WOW_CLIENT_PROFILE is not wrath");
        return None;
    }
    let chain = Chain::open_profile(&data, &profile).expect("open the Wrath chain");
    Some((data, profile, chain))
}

fn normal(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

/// The first listed `.blp` under `dir` whose file name contains `needle`, in sorted order.
fn first_texture(chain: &Chain, dir: &str, needle: &str) -> String {
    let dir = normal(dir);
    let mut names: Vec<String> = chain
        .list_filtered(|n| {
            let n = normal(n);
            n.starts_with(&dir)
                && n.ends_with(".blp")
                && n.rsplit('\\').next().is_some_and(|f| f.contains(needle))
        })
        .expect("list textures")
        .into_iter()
        .map(|e| e.name)
        .collect();
    names.sort_by_key(|n| normal(n));
    names
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no {needle} texture under {dir}"))
}

/// `(pack path, installed source)` pairs: the model with every sibling sharing its stem, and the
/// terrain texture pair, as `(target, source)`.
fn demo_pack(chain: &Chain) -> (Vec<(String, String)>, (String, String)) {
    let stem = |p: &str| normal(p).strip_suffix(".m2").unwrap().to_string();
    let (from, to) = (stem(MODEL_SOURCE), stem(MODEL_TARGET));
    let mut files: Vec<(String, String)> = chain
        .list_filtered(|n| {
            let n = normal(n);
            n.starts_with(&from) && (n.ends_with(".skin") || n.ends_with(".anim"))
        })
        .expect("list model siblings")
        .into_iter()
        .map(|e| {
            let tail = normal(&e.name)[from.len()..].to_string();
            (format!("{to}{tail}"), e.name)
        })
        .collect();
    assert!(
        files.iter().any(|(t, _)| t.ends_with("00.skin")),
        "the source model's profile-zero skin is installed"
    );
    files.push((normal(MODEL_TARGET), MODEL_SOURCE.to_string()));
    let target = first_texture(chain, "Tileset\\Elwynn\\", "grass");
    let source = first_texture(chain, "Tileset\\Durotar\\", "");
    files.push((normal(&target), source.clone()));
    (files, (target, source))
}

/// Copy the pairs into `root`, mirroring archive paths (lower-cased, `/` separated).
fn write_pack(chain: &Chain, root: &Path, files: &[(String, String)]) {
    for (target, source) in files {
        let path = root.join(target.replace('\\', "/"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, chain.read(source).expect("installed source")).unwrap();
    }
}

fn app_with(data: &Path, profile: &DataProfile, overrides: AssetOverrides) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    register_mpq_source_overrides(&mut app, data, profile, overrides).expect("register source");
    app.add_plugins(bevy::asset::AssetPlugin::default());
    app.init_asset::<Image>();
    app.init_asset::<Mesh>();
    app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>();
    app.init_asset::<bevy::animation::AnimationClip>();
    app.init_asset::<bevy::animation::graph::AnimationGraph>();
    register_asset_loaders(&mut app);
    app
}

/// A model's comparable shape: batch vertex counts and authored bounds.
fn shape(m: &M2Model) -> (Vec<usize>, String) {
    (
        m.submeshes
            .iter()
            .map(|s| s.geometry.positions.len())
            .collect(),
        format!("{:?}", m.bounds),
    )
}

fn load_all(app: &mut App, models: &[&str], images: &[&str]) -> (Vec<M2Model>, Vec<Image>) {
    let server = app.world().resource::<AssetServer>().clone();
    let m: Vec<Handle<M2Model>> = models
        .iter()
        .map(|p| server.load(format!("mpq://{}", p.replace('\\', "/"))))
        .collect();
    let i: Vec<Handle<Image>> = images
        .iter()
        .map(|p| server.load(texture_url(p, (true, true))))
        .collect();
    for _ in 0..3000 {
        app.update();
        let (ms, is) = (
            app.world().resource::<Assets<M2Model>>(),
            app.world().resource::<Assets<Image>>(),
        );
        if m.iter().all(|h| ms.contains(h)) && i.iter().all(|h| is.contains(h)) {
            break;
        }
        for h in m
            .iter()
            .map(|h| h.id().untyped())
            .chain(i.iter().map(|h| h.id().untyped()))
        {
            if let Some(bevy::asset::LoadState::Failed(e)) = server.get_load_state(h) {
                panic!("asset failed: {e}");
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let (ms, is) = (
        app.world().resource::<Assets<M2Model>>(),
        app.world().resource::<Assets<Image>>(),
    );
    (
        m.iter()
            .map(|h| ms.get(h).expect("model loaded").clone())
            .collect(),
        i.iter()
            .map(|h| is.get(h).expect("image loaded").clone())
            .collect(),
    )
}

#[test]
fn a_real_pack_replaces_a_real_model_and_texture_with_archive_fallback() {
    let Some((data, profile, chain)) = wrath() else {
        return;
    };
    let (files, (tex_target, tex_source)) = demo_pack(&chain);
    let root = std::env::temp_dir().join(format!("azb-real-pack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write_pack(&chain, &root, &files);
    let overrides = AssetOverrides::from_roots([root.clone()]).expect("mount pack");
    let probe = Chain::open_profile(&data, &profile)
        .unwrap()
        .with_overrides(overrides.clone());
    assert!(
        probe.override_path(MODEL_TARGET).is_some(),
        "provenance names the pack file"
    );
    assert!(probe.override_path(MODEL_FALLBACK).is_none());

    let mut packed = app_with(&data, &profile, overrides);
    let (models, images) = load_all(&mut packed, &[MODEL_TARGET, MODEL_FALLBACK], &[&tex_target]);
    let mut stock = app_with(&data, &profile, AssetOverrides::default());
    let (originals, sources) = load_all(
        &mut stock,
        &[MODEL_SOURCE, MODEL_TARGET, MODEL_FALLBACK],
        &[&tex_source, &tex_target],
    );
    eprintln!(
        "pack: {} file(s); {MODEL_TARGET} <- {MODEL_SOURCE}; {tex_target} <- {tex_source}",
        files.len()
    );
    assert_eq!(
        shape(&models[0]),
        shape(&originals[0]),
        "lamp is now the ballista"
    );
    assert_ne!(
        shape(&originals[1]),
        shape(&originals[0]),
        "the stock lamp differs"
    );
    assert_eq!(
        shape(&models[1]),
        shape(&originals[2]),
        "untouched model falls back"
    );
    assert_eq!(
        images[0].data, sources[0].data,
        "terrain texture is the pack's"
    );
    assert_ne!(
        sources[1].data, sources[0].data,
        "the stock texture differs"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// `AZB_DEMO_PACK_OUT=<home>/mods/<pack>`: writes the same pack for a live session, refusing a
/// non-empty folder. Enable with `[experience] profile = "plus"`, `packs = ["<pack>"]`.
#[test]
#[ignore = "on demand: writes installed assets to a local mods folder"]
fn write_demo_pack() {
    let Some((_, _, chain)) = wrath() else {
        return;
    };
    let out = PathBuf::from(std::env::var("AZB_DEMO_PACK_OUT").expect("AZB_DEMO_PACK_OUT"));
    if out.exists() {
        assert!(
            std::fs::read_dir(&out).unwrap().next().is_none(),
            "{} is not empty",
            out.display()
        );
    }
    let (files, (tex_target, tex_source)) = demo_pack(&chain);
    write_pack(&chain, &out, &files);
    println!("wrote {} file(s) to {}", files.len(), out.display());
    println!("model: {MODEL_TARGET} <- {MODEL_SOURCE}");
    println!("texture: {tex_target} <- {tex_source}");
}
