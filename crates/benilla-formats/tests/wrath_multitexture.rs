//! Read-only regression for models formerly rejected by the one-stage render adapter.

use std::{io::Cursor, path::Path};

use benilla_formats::{parse_wrath_render_model, Chain};
use benilla_m2::wrath::{parse_wrath_m2, parse_wrath_skin};
use benilla_m2::M2TextureType;

#[test]
fn installed_wrath_multitexture_models_retain_both_stages() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        eprintln!("SKIP installed Wrath multi-texture test: select WOW_CLIENT_PROFILE=wrath");
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA for installed Wrath test");
    let mut chain = Chain::open_wrath(Path::new(&data), "enUS").expect("Wrath archives");
    let names = [
        "generalweaponrack01.m2",
        "stormwindthrone.m2",
        "hordezepanimation.m2",
        "icebreaker_paddlewheel.m2",
    ];
    let entries = chain
        .list_filtered(|path| {
            let lower = path.to_ascii_lowercase().replace('/', "\\");
            names
                .iter()
                .any(|name| lower.ends_with(&format!("\\{name}")))
        })
        .expect("installed archive model names");
    for name in names {
        let path = &entries
            .iter()
            .find(|entry| entry.name.to_ascii_lowercase().ends_with(name))
            .unwrap_or_else(|| panic!("missing installed model {name}"))
            .name;
        let bytes = chain
            .read_file(path)
            .unwrap_or_else(|error| panic!("{path}: {error:#}"));
        let skin_path = format!("{}00.skin", &path[..path.len() - 3]);
        let skin = chain.read_file(&skin_path).expect("external skin profile");
        let batches = parse_wrath_skin(&skin).expect("skin batches");
        assert!(
            batches
                .batches()
                .iter()
                .any(|batch| batch.texture_count > 1),
            "{path}: multi-stage fixture"
        );
        let model = parse_wrath_render_model(&bytes, &skin, "", &[])
            .unwrap_or_else(|error| panic!("{path}: {error:#}"));
        assert!(!model.submeshes.is_empty(), "{path}: admitted geometry");
        assert!(
            model.submeshes.iter().any(|sub| sub.texture.is_some()),
            "{path}: base texture"
        );
        let raw = parse_wrath_m2(&mut Cursor::new(bytes.as_slice())).expect("raw model");
        let raw = raw.model();
        for batch in batches
            .batches()
            .iter()
            .filter(|batch| batch.texture_count > 1)
        {
            let texture_index =
                raw.raw_data.texture_lookup_table[batch.texture_combo_index as usize];
            let texture = &raw.textures[texture_index as usize];
            assert!(
                matches!(texture.texture_type, M2TextureType::Hardcoded),
                "{path}: fixed base texture for this doodad regression"
            );
            let base = texture.filename.string.to_string_lossy();
            assert!(
                model.submeshes.iter().any(|sub| {
                    sub.section == Some(batch.skin_section_index)
                        && sub.texture.as_deref() == Some(base.as_ref())
                        && sub.wrap_x == texture.wrap_x
                        && sub.wrap_y == texture.wrap_y
                        && sub.env_map == raw.stage_is_env_mapped(batch, 0)
                }),
                "{path}: multi-stage batch must draw stage 0, not its additional layer"
            );
            let layer_index =
                raw.raw_data.texture_lookup_table[batch.texture_combo_index as usize + 1];
            let layer = &raw.textures[layer_index as usize];
            let layer_path = layer.filename.string.to_string_lossy();
            let combos = parse_wrath_m2(&mut Cursor::new(bytes.as_slice())).unwrap();
            let op = combos.texture_combiner_combos()[batch.shader_id as usize + 1];
            let stage = model
                .submeshes
                .iter()
                .find(|s| {
                    s.section == Some(batch.skin_section_index)
                        && s.texture.as_deref() == Some(base.as_ref())
                })
                .unwrap()
                .stage2
                .as_ref()
                .unwrap();
            assert_eq!(stage.texture.as_deref(), Some(layer_path.as_ref()));
            assert_eq!(
                (stage.op, stage.coord, stage.wrap_x, stage.wrap_y),
                (
                    op,
                    raw.texture_unit_lookup[batch.texture_coord_combo_index as usize + 1],
                    layer.wrap_x,
                    layer.wrap_y
                )
            );
            assert!(!chain.read_file(layer_path.as_ref()).unwrap().is_empty());
            eprintln!(
                "{path}: section {} draws stage 0 texture {base}",
                batch.skin_section_index
            );
        }
        eprintln!(
            "{path}: {} submeshes, {} triangles (combined stages)",
            model.submeshes.len(),
            model
                .submeshes
                .iter()
                .map(|sub| sub.indices.len() / 3)
                .sum::<usize>()
        );
    }
}
