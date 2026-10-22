//! Read-only inventory of native two-stage constructs not fully covered by a2.
use wrathplus_formats::Chain;
use wrathplus_m2::wrath::{parse_wrath_m2, parse_wrath_skin};
use std::{io::Cursor, path::Path};

#[test]
#[ignore = "exhaustive installed-data census"]
fn installed_wrath_two_stage_gap_census() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let entries = chain
        .list_filtered(|p| p.to_ascii_lowercase().ends_with(".m2"))
        .unwrap();
    let (mut native, mut missing_skin, mut parse_errors, mut multi_models, mut multi_batches) =
        (0, 0, 0, 0, 0);
    let (
        mut highbit,
        mut over_two,
        mut independent_weight,
        mut weight_nonidentity,
        mut second_uv,
        mut environment,
    ) = (0, 0, 0, 0, 0, 0);
    let mut gap_models = [0usize; 4];
    let mut gap_batches = [0usize; 4];
    let mut examples: [Vec<String>; 4] = std::array::from_fn(|_| Vec::new());
    let mut particle_references = std::collections::HashSet::new();
    let mut animated_assets = Vec::new();
    let mut all_highbit_models = 0usize;
    let mut all_highbit_batches = 0usize;
    for entry in entries {
        let bytes = chain.read_file(&entry.name).unwrap();
        if bytes.get(4..8) != Some(&264u32.to_le_bytes()) {
            continue;
        }
        native += 1;
        if let Ok(emitters) = wrathplus_formats::parse_m2_particle_emitters(&bytes) {
            for emitter in emitters {
                if let Some(path) = emitter.geometry_model {
                    particle_references.insert(path.to_ascii_lowercase().replace(".mdx", ".m2"));
                }
            }
        }

        let skin_path = format!("{}00.skin", &entry.name[..entry.name.len() - 3]);
        let Ok(skin) = chain.read_file(&skin_path) else {
            missing_skin += 1;
            continue;
        };
        let (Ok(model), Ok(skin)) = (
            parse_wrath_m2(&mut Cursor::new(&bytes)),
            parse_wrath_skin(&skin),
        ) else {
            parse_errors += 1;
            continue;
        };
        let raw = model.model();
        let optimized_count = skin
            .batches()
            .iter()
            .filter(|b| b.shader_id & 0x8000 != 0)
            .count();
        all_highbit_models += usize::from(optimized_count > 0);
        all_highbit_batches += optimized_count;

        let mut any = false;
        let mut model_gaps = [false; 4];
        let mut animated_batches = 0;
        for batch in skin.batches().iter().filter(|b| b.texture_count > 1) {
            any = true;
            multi_batches += 1;
            highbit += usize::from(batch.shader_id & 0x8000 != 0);
            over_two += usize::from(batch.texture_count > 2);
            let at = batch.weight_combo_index as usize;
            let first = raw.transparency_lookup.get(at);
            let second = raw.transparency_lookup.get(at + 1);
            let independent = second.is_some_and(|s| Some(s) != first && *s != u16::MAX);
            independent_weight += usize::from(independent);
            let nonidentity = second
                .and_then(|i| raw.transparency_tracks.get(*i as usize))
                .is_some_and(|t| t.keys.iter().any(|(_, v)| (*v - 1.0).abs() > 0.0001));
            weight_nonidentity += usize::from(nonidentity);
            let uv = raw
                .texture_transform_lookup
                .get(batch.texture_transform_combo_index as usize + 1)
                .copied();
            second_uv += usize::from(uv.is_some_and(|v| v != u16::MAX));
            environment += usize::from(raw.stage_is_env_mapped(batch, 1));
            let animated_uv = uv
                .and_then(|i| raw.texture_transforms.get(i as usize))
                .is_some_and(|t| {
                    [
                        t.translation.keys.len() > 1 && t.translation.constant().is_none(),
                        t.rotation.keys.len() > 1 && t.rotation.constant().is_none(),
                        t.scaling.keys.len() > 1 && t.scaling.constant().is_none(),
                    ]
                    .into_iter()
                    .any(|v| v)
                });
            animated_batches += usize::from(animated_uv);
            // Lane membership is runtime-dependent: these are affected assets IF instantiated
            // through a particle/offworld lane, not a claimed census of runtime instances.
            for (i, hit) in [
                batch.texture_count == 2 && independent,
                batch.shader_id & 0x8000 != 0,
                raw.stage_is_env_mapped(batch, 1),
                animated_uv,
            ]
            .into_iter()
            .enumerate()
            {
                if hit {
                    gap_batches[i] += 1;
                    model_gaps[i] = true;
                }
            }

            if independent
                || nonidentity
                || batch.shader_id & 0x8000 != 0
                || batch.texture_count > 2
            {
                eprintln!("GAP {} section={} stages={} shader={} weights={first:?}/{second:?} second_weight_nonidentity={nonidentity} second_uv={uv:?}",entry.name,batch.skin_section_index,batch.texture_count,batch.shader_id);
            }
        }
        if animated_batches > 0 {
            animated_assets.push((entry.name.clone(), animated_batches));
        }
        multi_models += usize::from(any);
        for i in 0..4 {
            if model_gaps[i] {
                gap_models[i] += 1;
                if examples[i].len() < 5 {
                    examples[i].push(entry.name.clone());
                }
            }
        }
    }
    eprintln!("CENSUS native_models={native} missing_skin={missing_skin} parse_errors={parse_errors} multi_models={multi_models} multi_batches={multi_batches} highbit_batches={highbit} over_two_batches={over_two} independent_weight_lookup_batches={independent_weight} second_weight_nonidentity_batches={weight_nonidentity} second_uv_lookup_batches={second_uv} environment_stage2_batches={environment}");
    for i in 0..4 {
        eprintln!(
            "GAP_CATEGORY {i} models={} batches={} examples={:?}",
            gap_models[i], gap_batches[i], examples[i]
        );
    }
    let particle_assets: Vec<_> = animated_assets
        .iter()
        .filter(|(p, _)| particle_references.contains(&p.to_ascii_lowercase()))
        .collect();
    eprintln!(
        "PARTICLE_REFERENCED_ANIMATED_STAGE2 models={} batches={} examples={:?}",
        particle_assets.len(),
        particle_assets.iter().map(|(_, n)| *n).sum::<usize>(),
        particle_assets
            .iter()
            .take(5)
            .map(|(p, _)| p)
            .collect::<Vec<_>>()
    );
    eprintln!("ALL_BATCH_OPTIMIZED models={all_highbit_models} batches={all_highbit_batches}");
    assert!(multi_models > 0);
    // A different lookup is only a structural signal, not proof of native per-stage use.
    // Missing skins and parser failures stay visible: they are never counted as supported.
}
