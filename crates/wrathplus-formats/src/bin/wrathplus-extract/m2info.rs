//! `m2info`: one v264 model's sequences, rig, batches and effects, read the way the loaders read
//! it (sibling `.anim` files by sequence, then `00.skin`), for inspecting a model from the shell.

use anyhow::{Context, Result};
use wrathplus_formats::{
    parse_m2_particle_emitters_with_animations, parse_wrath_render_model_with_animations,
    wrath_render_animation_dependencies, Chain,
};

/// Print the report for `internal_path` (`.mdx`/`.mdl` resolve to `.m2`).
pub fn m2info(chain: &mut Chain, internal_path: &str) -> Result<()> {
    let path = crate::normalize(&crate::model_key(internal_path));
    let bytes = chain
        .read_file(&path)
        .with_context(|| format!("reading {path}"))?;
    let stem = path.strip_suffix(".m2").unwrap_or(&path);
    let dir = stem.rsplit_once('\\').map_or("", |(d, _)| d);

    let mut external: Vec<Option<Vec<u8>>> = Vec::new();
    for dep in wrath_render_animation_dependencies(&bytes)? {
        let anim = format!(
            "{stem}{:04}-{:02}.anim",
            dep.animation_id, dep.variation_index
        );
        let anim_bytes = chain
            .read_file(&anim)
            .with_context(|| format!("reading {anim}"))?;
        if external.len() <= dep.sequence_index {
            external.resize(dep.sequence_index + 1, None);
        }
        external[dep.sequence_index] = Some(anim_bytes);
    }
    let external: Vec<Option<&[u8]>> = external.iter().map(|e| e.as_deref()).collect();
    let skin_path = format!("{stem}00.skin");
    let skin = chain
        .read_file(&skin_path)
        .with_context(|| format!("reading {skin_path}"))?;
    let model = parse_wrath_render_model_with_animations(&bytes, &skin, &external, dir, &[])?;
    let emitters = parse_m2_particle_emitters_with_animations(&bytes, &external)?;

    println!("{path}");
    let b = &model.bounds;
    println!(
        "bounds: sphere {:.3}, box {:?}..{:?}, stand box z {:.3}",
        b.sphere_radius, b.bbox_min, b.bbox_max, b.stand_box_z
    );
    println!(
        "collision: {} vertices, {} triangles",
        model.collision.positions.len(),
        model.collision.indices.len() / 3
    );

    println!("\nsequences ({}; .anim {}):", model.animations.len(), {
        external.iter().filter(|e| e.is_some()).count()
    });
    for a in &model.animations {
        println!(
            "  [{:>3}] id {:>3}  {:>7.3} s  {}  move {:.2}",
            a.seq_index,
            a.anim_id,
            a.duration,
            if a.looping { "loop " } else { "clamp" },
            a.move_speed
        );
    }

    println!(
        "\nbones: {}, global-sequence bones: {}",
        model.skeleton.bones.len(),
        model.global_sequence_bones.len()
    );
    println!("attachments ({}):", model.attachments.len());
    for a in &model.attachments {
        println!("  id {:>2} on bone {:>3} at {:?}", a.id, a.bone, a.position);
    }
    println!("event markers ({}):", model.event_markers.len());
    for e in &model.event_markers {
        println!(
            "  {} on bone {:>3}",
            String::from_utf8_lossy(&e.ident),
            e.bone
        );
    }

    println!("\nbatches ({}):", model.submeshes.len());
    for (i, s) in model.submeshes.iter().enumerate() {
        println!(
            "  [{i:>2}] geoset {:>4}  {:>6} tris  {:?}{}  {}",
            s.geoset_id,
            s.indices.len() / 3,
            s.blend,
            if s.stage2.is_some() { " +stage2" } else { "" },
            s.texture.as_deref().unwrap_or("-")
        );
    }

    println!("\nparticle emitters ({}):", emitters.len());
    for (i, e) in emitters.iter().enumerate() {
        println!(
            "  [{i:>2}] bone {:>3}  flags {:#010x}  {:?}  {}",
            e.bone,
            e.flags,
            e.blend,
            e.texture.as_deref().unwrap_or("-")
        );
    }
    println!("ribbons ({}):", model.ribbons.len());
    for r in &model.ribbons {
        println!(
            "  bone {:>3}  {:?}  {}",
            r.bone,
            r.blend,
            r.texture.as_deref().unwrap_or("-")
        );
    }
    println!("lights ({}):", model.lights.len());
    for l in &model.lights {
        println!(
            "  type {} on bone {:>3} at {:?}",
            l.light_type, l.bone, l.position
        );
    }
    println!(
        "cameras: {} (lookup {:?})",
        model.cameras.len(),
        model.camera_lookup
    );
    Ok(())
}
