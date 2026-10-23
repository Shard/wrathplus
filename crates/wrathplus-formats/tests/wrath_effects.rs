//! Opt-in read-only native-effect fixtures; no extracted assets are written.
use wrathplus_formats::Chain;
use wrathplus_m2::wrath::parse_wrath_m2;
use std::{io::Cursor, path::Path};

#[test]
fn installed_wrath_native_effect_records() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = Chain::open_wrath(Path::new(&data), "enUS").unwrap();
    let entries = chain
        .list_filtered(|path| {
            let p = path.to_ascii_lowercase();
            p.ends_with(".m2")
                && (p.contains("frosttrap") || p.contains("brazier") || p.contains("kobold"))
        })
        .unwrap();
    let mut light_models = 0;
    let mut ribbon_models = 0;
    for entry in entries {
        let bytes = chain.read_file(&entry.name).unwrap();
        if bytes.get(4..8) != Some(&264u32.to_le_bytes()) {
            continue;
        }
        let m = parse_wrath_m2(&mut Cursor::new(&bytes))
            .unwrap_or_else(|e| panic!("{}: {e}", entry.name));
        if entry.name.ends_with("DwarvenBrazier01.m2") || entry.name.ends_with("G_FrostTrap.m2") {
            let skin = chain
                .read_file(&format!("{}00.skin", &entry.name[..entry.name.len() - 3]))
                .unwrap();
            let semantic = wrathplus_formats::parse_wrath_render_model(&bytes, &skin, "", &[])
                .unwrap_or_else(|e| panic!("{} adapter: {e:#}", entry.name));
            if entry.name.ends_with("DwarvenBrazier01.m2") {
                assert_eq!(m.lights().len(), 1);
                assert_eq!(m.lights()[0].light_type, 1);
                assert!(semantic.lights[0].diffuse_intensity > 0.0);
            } else {
                assert_eq!(m.ribbons().len(), 16);
                assert!(semantic.ribbons.iter().any(|r| r
                    .visible
                    .as_ref()
                    .is_some_and(|v| v.per_anim().any(|(_, keys)| keys.len() > 1))));
            }
            assert_eq!(semantic.lights.len(), m.lights().len());
            assert_eq!(semantic.ribbons.len(), m.ribbons().len());
            for r in &semantic.ribbons {
                assert!(r.texture.is_some(), "ribbon texture");
                assert!(r.edge_lifetime >= 0.25);
                assert!(r.tile_rows > 0 && r.tile_cols > 0);
            }
        }
        if !m.lights().is_empty() {
            light_models += 1;
        }
        if !m.ribbons().is_empty() {
            ribbon_models += 1;
        }
        eprintln!(
            "{}: lights={} ribbons={} combiners={}",
            entry.name,
            m.lights().len(),
            m.ribbons().len(),
            m.texture_combiner_combos().len()
        );
    }
    assert!(light_models > 0, "installed point-light fixtures");
    assert!(ribbon_models > 0, "installed ribbon fixtures");
}

#[test]
fn installed_wrath_unsupported_ribbon_census() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let entries = chain
        .list_filtered(|p| p.to_ascii_lowercase().ends_with(".m2"))
        .unwrap();
    let (mut models, mut ribbon_models, mut cubic_models, mut global_models, mut parse_errors) =
        (0, 0, 0, 0, 0);
    for entry in entries {
        let b = chain.read_file(&entry.name).unwrap();
        if b.get(4..8) != Some(&264u32.to_le_bytes()) {
            continue;
        }
        models += 1;
        if u32::from_le_bytes(b[0x120..0x124].try_into().unwrap()) == 0 {
            continue;
        }
        ribbon_models += 1;
        let m = match parse_wrath_m2(&mut Cursor::new(&b)) {
            Ok(m) => m,
            Err(e) => {
                parse_errors += 1;
                eprintln!("CENSUS ERROR {}: {e}", entry.name);
                continue;
            }
        };
        let cubic = m.ribbons().iter().any(|r| {
            [
                r.color.interpolation,
                r.alpha.interpolation,
                r.height_above.interpolation,
                r.height_below.interpolation,
            ]
            .iter()
            .any(|&i| i > 1)
        });
        let global = m.ribbons().iter().any(|r| {
            r.visibility.global_sequence != u16::MAX
                && r.visibility.sequences.first().is_some_and(|keys| {
                    keys.first()
                        .is_some_and(|first| keys.iter().any(|k| k.value != first.value))
                })
        });
        if cubic {
            cubic_models += 1;
        }
        if global {
            global_models += 1;
        }
        if cubic || global {
            eprintln!(
                "UNSUPPORTED {} cubic={cubic} global_visibility={global}",
                entry.name
            );
        }
    }
    eprintln!("CENSUS native_models={models} ribbon_models={ribbon_models} cubic_models={cubic_models} animated_global_visibility_models={global_models} ribbon_parse_errors={parse_errors}");
    assert!(models > 0 && ribbon_models > 0);
}

use wrathplus_m2::wrath::WrathTrack;
use std::collections::BTreeMap;

/// How a native track is keyed: global or local, constant, animated or per-sequence.
fn shape<V: PartialEq + Copy>(t: &WrathTrack<V>) -> &'static str {
    let animated = |keys: &[wrathplus_m2::wrath::WrathTrackKey<V>]| {
        keys.first()
            .is_some_and(|f| keys.iter().any(|k| k.value != f.value))
    };
    if t.global_sequence != u16::MAX {
        return if t.sequences.iter().any(|k| animated(k)) {
            "global_animated"
        } else {
            "global_constant"
        };
    }
    let first: Vec<V> = t
        .sequences
        .first()
        .map(|k| k.iter().map(|k| k.value).collect())
        .unwrap_or_default();
    let differs = t
        .sequences
        .iter()
        .skip(1)
        .any(|k| !k.is_empty() && k.iter().map(|k| k.value).collect::<Vec<_>>() != first);
    match (
        first.is_empty(),
        t.sequences.first().is_some_and(|k| animated(k)),
        differs,
    ) {
        (true, _, true) => "local_seq0_empty_others_keyed",
        (_, _, true) => "local_varies_by_sequence",
        (_, true, false) => "local_seq0_animated",
        _ => "local_constant",
    }
}

/// Installed incidence of the ribbon appearance-track shapes the adapter has to represent.
#[test]
fn installed_wrath_ribbon_track_incidence() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let entries = chain
        .list_filtered(|p| p.to_ascii_lowercase().ends_with(".m2"))
        .unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, String> = BTreeMap::new();
    let mut bump = |key: String, name: &str| {
        *counts.entry(key.clone()).or_default() += 1;
        examples.entry(key).or_insert_with(|| name.to_string());
    };
    for entry in entries {
        let b = chain.read_file(&entry.name).unwrap();
        if b.get(4..8) != Some(&264u32.to_le_bytes())
            || u32::from_le_bytes(b[0x120..0x124].try_into().unwrap()) == 0
        {
            continue;
        }
        let m = parse_wrath_m2(&mut Cursor::new(&b)).unwrap();
        if m.animations().first().is_some_and(|a| a.id != 0) {
            bump("model_seq0_not_stand".into(), &entry.name);
        }
        for r in m.ribbons() {
            bump("ribbons".into(), &entry.name);
            bump(format!("textures={}", r.texture_indices.len()), &entry.name);
            bump(
                format!("materials={}", r.material_indices.len()),
                &entry.name,
            );
            for (name, s) in [
                ("color", shape(&r.color)),
                ("alpha", shape(&r.alpha)),
                ("height_above", shape(&r.height_above)),
                ("height_below", shape(&r.height_below)),
                ("texture_slot", shape(&r.texture_slot)),
                ("visibility", shape(&r.visibility)),
            ] {
                bump(format!("{name}.{s}"), &entry.name);
            }
            let slots: std::collections::BTreeSet<u16> = r
                .texture_slot
                .sequences
                .iter()
                .flatten()
                .map(|k| k.value)
                .collect();
            if slots.iter().any(|&s| s != 0) {
                bump("texture_slot_nonzero".into(), &entry.name);
            }
            if slots.iter().any(|&s| {
                u32::from(s) >= u32::from(r.tile_rows.max(1)) * u32::from(r.tile_cols.max(1))
            }) {
                bump("texture_slot_out_of_grid".into(), &entry.name);
            }
            if r.alpha.sequences.iter().flatten().any(|k| k.value < 0) {
                bump("alpha_negative".into(), &entry.name);
            }
            if r.height_above
                .sequences
                .iter()
                .flatten()
                .chain(r.height_below.sequences.iter().flatten())
                .any(|k| k.value < 0.0)
            {
                bump("height_negative".into(), &entry.name);
            }
            if r.edge_lifetime < 0.25 {
                bump("edge_lifetime_below_quarter".into(), &entry.name);
            }
            if r.priority_plane != 0 {
                bump("priority_plane_nonzero".into(), &entry.name);
            }
        }
    }
    for (k, v) in &counts {
        eprintln!("INCIDENCE {k}={v} e.g. {}", examples[k]);
    }
    assert!(counts.get("ribbons").copied().unwrap_or(0) > 0);
}

/// The adapter keeps a global-sequence look track on its period and a sequence-keyed one per slot:
/// `ChainsofIce_Low_Base.m2`'s alpha loops on the scene clock; `1HTrollSpear01.m2`'s heights are
/// keyed only outside sequence 0, so its trail still spawns with width.
#[test]
fn installed_wrath_ribbon_look_tracks_keep_their_clocks() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let mut adapt = |path: &str| {
        let bytes = chain.read_file(path).unwrap();
        let skin = chain
            .read_file(&format!("{}00.skin", &path[..path.len() - 3]))
            .unwrap();
        wrathplus_formats::parse_wrath_render_model(&bytes, &skin, "", &[])
            .unwrap_or_else(|e| panic!("{path}: {e:#}"))
    };
    let chains = adapt("SPELLS\\ChainsofIce_Low_Base.m2");
    let global = chains
        .ribbons
        .iter()
        .find(|r| r.alpha.global_period_ms.is_some())
        .expect("a global-sequence alpha ribbon");
    let period = global.alpha.global_period_ms.unwrap();
    assert!(period > 0, "a resolved global period");
    let at = |scene_ms: f64| wrathplus_formats::SeqClock {
        slot: 0,
        seq_ms: 0.0,
        scene_ms,
    };
    for phase in [0.0, f64::from(period) * 0.37, f64::from(period) * 0.81] {
        assert_eq!(
            global.alpha.sample(at(phase)),
            global.alpha.sample(at(phase + 7.0 * f64::from(period))),
            "alpha repeats every period"
        );
    }

    let spear = adapt("ITEM\\OBJECTCOMPONENTS\\WEAPON\\1HTrollSpear01.m2");
    let r = spear
        .ribbons
        .iter()
        .find(|r| {
            r.height_above
                .slots
                .first()
                .is_some_and(|l| l.keys.is_empty())
        })
        .expect("a ribbon unkeyed in sequence 0");
    assert_eq!(r.sequences.0.len(), r.height_above.slots.len());
    assert!(r
        .height_above
        .slots
        .iter()
        .skip(1)
        .any(|l| !l.keys.is_empty()));
    assert!(
        r.height_above.peak() + r.height_below.peak() > 0.0,
        "the keyed sequences give the trail width"
    );
}

/// Installed incidence of light types and light-track shapes.
#[test]
fn installed_wrath_light_track_incidence() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let entries = chain
        .list_filtered(|p| p.to_ascii_lowercase().ends_with(".m2"))
        .unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, String> = BTreeMap::new();
    let mut bump = |key: String, name: &str| {
        *counts.entry(key.clone()).or_default() += 1;
        examples.entry(key).or_insert_with(|| name.to_string());
    };
    for entry in entries {
        let b = chain.read_file(&entry.name).unwrap();
        // Lights: count @ MD20+0x108 in v264.
        if b.get(4..8) != Some(&264u32.to_le_bytes())
            || u32::from_le_bytes(b[0x108..0x10c].try_into().unwrap()) == 0
        {
            continue;
        }
        let m = parse_wrath_m2(&mut Cursor::new(&b)).unwrap();
        bump("models".into(), &entry.name);
        for l in m.lights() {
            bump(format!("type={}", l.light_type), &entry.name);
            bump(format!("bone_none={}", l.bone < 0), &entry.name);
            for (name, s) in [
                ("ambient_color", shape(&l.ambient_color)),
                ("ambient_intensity", shape(&l.ambient_intensity)),
                ("diffuse_color", shape(&l.diffuse_color)),
                ("diffuse_intensity", shape(&l.diffuse_intensity)),
                ("attenuation_start", shape(&l.attenuation_start)),
                ("attenuation_end", shape(&l.attenuation_end)),
                ("visibility", shape(&l.visibility)),
            ] {
                bump(format!("{name}.{s}"), &entry.name);
            }
            if [
                l.ambient_color.interpolation,
                l.diffuse_color.interpolation,
                l.ambient_intensity.interpolation,
                l.diffuse_intensity.interpolation,
                l.attenuation_start.interpolation,
                l.attenuation_end.interpolation,
            ]
            .iter()
            .any(|&i| i > 1)
            {
                bump("cubic".into(), &entry.name);
            }
            if l.visibility
                .sequences
                .first()
                .and_then(|k| k.first())
                .is_some_and(|k| k.value == 0)
            {
                bump("visibility_seq0_first_off".into(), &entry.name);
            }
        }
    }
    for (k, v) in &counts {
        eprintln!("LIGHTS {k}={v} e.g. {}", examples[k]);
    }
    assert!(counts.get("models").copied().unwrap_or(0) > 0);
}

/// Installed animated lights: the Northrend torch flickers on its own Stand clock, and
/// `CallLightning_Impact.m2` starts dark but still spawns, since its byte reads on later.
#[test]
fn installed_wrath_lights_keep_their_animation() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let mut adapt = |path: &str| {
        let bytes = chain.read_file(path).unwrap();
        let skin = chain
            .read_file(&format!("{}00.skin", &path[..path.len() - 3]))
            .unwrap();
        wrathplus_formats::parse_wrath_render_model(&bytes, &skin, "", &[])
            .unwrap_or_else(|e| panic!("{path}: {e:#}"))
    };
    let at = |seq_ms: f32| wrathplus_formats::SeqClock {
        slot: 0,
        seq_ms,
        scene_ms: 0.0,
    };

    let torch = adapt("ITEM\\OBJECTCOMPONENTS\\WEAPON\\Club_1h_Torch_A_03Northrend_Light.M2");
    let anim = torch
        .lights
        .iter()
        .find_map(|l| l.anim.clone())
        .expect("the torch's light is animated");
    let lane = &anim.diffuse_intensity.slots[0];
    assert!(lane.keys.len() > 1, "a keyed flicker");
    let samples: Vec<f32> = lane
        .keys
        .iter()
        .map(|&(t, _)| anim.diffuse_at(at(t as f32)).1)
        .collect();
    assert!(
        samples.iter().any(|&v| (v - samples[0]).abs() > 1e-3),
        "the intensity moves across Stand: {samples:?}"
    );

    let impact = adapt("SPELLS\\CallLightning_Impact.m2");
    let l = impact
        .lights
        .iter()
        .find(|l| l.visibility_off)
        .expect("a light dark at its first key");
    assert!(l.anim.is_some(), "its visibility is keyed");
    assert!(l.casts(), "a light that turns on later still spawns");
}

/// Installed incidence of particle-emitter tracks keyed inside external `.anim` sequences
/// (sequence flags bit 0x20 clear), which the emitter parser reads as unkeyed.
#[test]
fn installed_wrath_particle_anim_file_incidence() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let u32_at = |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let entries = chain
        .list_filtered(|p| p.to_ascii_lowercase().ends_with(".m2"))
        .unwrap();
    let (mut emitter_models, mut hit_models, mut hit_emitters, mut hit_tracks) = (0, 0, 0, 0);
    let mut examples = Vec::new();
    for entry in entries {
        let b = chain.read_file(&entry.name).unwrap();
        if b.get(4..8) != Some(&264u32.to_le_bytes()) || u32_at(&b, 0x128) == 0 {
            continue;
        }
        emitter_models += 1;
        let (nseq, oseq) = (u32_at(&b, 0x1c), u32_at(&b, 0x20));
        let external: Vec<bool> = (0..nseq)
            .map(|i| u32_at(&b, oseq + i * 64 + 0x0c) & 0x20 == 0)
            .collect();
        let (count, base) = (u32_at(&b, 0x128), u32_at(&b, 0x12c));
        let mut model_hit = false;
        for k in 0..count {
            let e = base + k * 0x1dc;
            let mut emitter_hit = false;
            for t in [
                0x34, 0x48, 0x5c, 0x70, 0x84, 0x98, 0xb0, 0xc8, 0xdc, 0xf0, 0x1c8,
            ] {
                let track = e + t;
                if u16::from_le_bytes([b[track + 2], b[track + 3]]) != u16::MAX {
                    continue; // a global sequence keeps its keys inline
                }
                let (n, ofs) = (u32_at(&b, track + 4), u32_at(&b, track + 8));
                let keyed = (0..n.min(external.len())).any(|i| {
                    external[i] && ofs + i * 8 + 4 <= b.len() && u32_at(&b, ofs + i * 8) > 0
                });
                if keyed {
                    hit_tracks += 1;
                    emitter_hit = true;
                }
            }
            if emitter_hit {
                hit_emitters += 1;
                model_hit = true;
            }
        }
        if model_hit {
            hit_models += 1;
            if examples.len() < 8 {
                examples.push(entry.name.clone());
            }
        }
    }
    eprintln!("ANIMPARTICLE emitter_models={emitter_models} models={hit_models} emitters={hit_emitters} tracks={hit_tracks} e.g. {examples:?}");
    assert!(emitter_models > 0);
}

/// `FrostLord.m2` keys emitter tracks inside external `.anim` sequences; with the files the asset
/// loader resolves, those sequences keep their keys instead of reading as unkeyed.
#[test]
fn installed_wrath_particle_tracks_read_external_anim_keys() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let path = "Creature\\FrostLord\\FrostLord.m2";
    let bytes = chain.read_file(path).unwrap();
    let stem = &path[..path.len() - 3];
    let deps = wrathplus_formats::wrath_render_animation_dependencies(&bytes).unwrap();
    assert!(!deps.is_empty(), "FrostLord has external sequences");
    let mut external: Vec<Option<Vec<u8>>> =
        vec![None; deps.iter().map(|d| d.sequence_index + 1).max().unwrap()];
    for d in &deps {
        let anim = format!("{stem}{:04}-{:02}.anim", d.animation_id, d.variation_index);
        external[d.sequence_index] = Some(chain.read_file(&anim).unwrap());
    }
    let refs: Vec<Option<&[u8]>> = external.iter().map(|b| b.as_deref()).collect();
    let without = wrathplus_formats::parse_m2_particle_emitters(&bytes).unwrap();
    let with = wrathplus_formats::parse_m2_particle_emitters_with_animations(&bytes, &refs).unwrap();
    assert_eq!(without.len(), with.len());
    let gained = without
        .iter()
        .zip(&with)
        .filter(|(a, b)| format!("{:?}", a.timing) != format!("{:?}", b.timing))
        .count();
    eprintln!(
        "FROSTLORD emitters={} gained_external_keys={gained}",
        with.len()
    );
    assert!(
        gained > 0,
        "an emitter keyed in an external sequence gains its keys"
    );
    // Inline sequences read the same either way.
    let inline_only =
        wrathplus_formats::parse_m2_particle_emitters_with_animations(&bytes, &[]).unwrap();
    assert_eq!(format!("{inline_only:?}"), format!("{without:?}"));
}
