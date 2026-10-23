//! Read-only Wrath character-composite metadata probe; never writes installed assets.

use std::path::Path;

use anyhow::Context;
use wrathplus_formats::{
    equip_blits, load_item_display_catalog, read_texture_mip_chain, Chain, CharCreateCatalog,
    CharSections,
};

fn main() -> anyhow::Result<()> {
    let data = std::env::args()
        .nth(1)
        .context("usage: wrath_appearance <Data path>")?;
    let mut chain = Chain::open_wrath(Path::new(&data), "enUS")?;
    let sections = CharSections::load(&mut chain)?;
    let outfits = CharCreateCatalog::load(&mut chain)?;
    let displays = load_item_display_catalog(&mut chain)?;
    let outfit = outfits.start_outfit(1, 1, 0);
    let mut equipment = [None; 8];
    for item in outfit {
        println!(
            "outfit inventory_type={} display_id={}",
            item.inv_type, item.display_id
        );
        let slot = match item.inv_type {
            4 => Some(0),
            5 | 20 => Some(1),
            6 => Some(2),
            7 => Some(3),
            8 => Some(4),
            9 => Some(5),
            10 => Some(6),
            19 => Some(7),
            _ => None,
        };
        if let Some(slot) = slot {
            equipment[slot] = displays.get(item.display_id);
        }
    }
    let base_path = sections
        .skin_texture(1, 0, 0)
        .context("missing human male skin")?;
    let base = read_texture_mip_chain(&mut chain, base_path)?;
    println!(
        "base={}x{} mip_count={}",
        base.width,
        base.height,
        base.mips.len()
    );
    for blit in equip_blits(&equipment, None, false) {
        for path in blit.candidates(0) {
            match read_texture_mip_chain(&mut chain, &path) {
                Ok(texture) => println!(
                    "layer={} found={}x{}",
                    blit.layer, texture.width, texture.height
                ),
                Err(_) => println!("layer={} candidate=absent", blit.layer),
            }
        }
    }
    let composite = sections
        .composite_body(&mut chain, 1, 0, 0, 0, 0, 0, 0, equipment, None, false)?
        .context("missing composite")?;
    for (name, tile) in [("torso", (128, 0, 128, 64)), ("pants", (128, 96, 128, 64))] {
        let (x, y, w, h) = tile;
        let (x, y, w, h) = (
            x * base.width / 256,
            y * base.height / 256,
            w * base.width / 256,
            h * base.height / 256,
        );
        let mut changed = 0;
        for row in y..y + h {
            for col in x..x + w {
                let offset = ((row * base.width + col) * 4) as usize;
                changed += usize::from(
                    base.mips[0][offset..offset + 4] != composite.mips[0][offset..offset + 4],
                );
            }
        }
        println!("region={name} changed_pixels={changed} total={}", w * h);
        anyhow::ensure!(
            changed > 0,
            "{name} equipment did not reach its normalized body region"
        );
    }
    let model_path = "Character\\Human\\Male\\HumanMale.m2";
    let model = chain.read_file(model_path)?;
    let skin = chain.read_file("Character\\Human\\Male\\HumanMale00.skin")?;
    let dependencies = wrathplus_formats::wrath_render_animation_dependencies(&model)?;
    let mut external = vec![
        None;
        dependencies
            .iter()
            .map(|d| d.sequence_index + 1)
            .max()
            .unwrap_or(0)
    ];
    for dependency in &dependencies {
        let path = format!(
            "Character\\Human\\Male\\HumanMale{:04}-{:02}.anim",
            dependency.animation_id, dependency.variation_index
        );
        external[dependency.sequence_index] = Some(
            chain
                .read_file(&path)
                .with_context(|| format!("reading required {path}"))?,
        );
    }
    let external_refs = external
        .iter()
        .map(|bytes| bytes.as_deref())
        .collect::<Vec<_>>();
    let semantic = wrathplus_formats::parse_wrath_render_model_with_animations(
        &model,
        &skin,
        &external_refs,
        "",
        &[],
    )?;
    let mut quaternions = 0;
    let mut invalid = 0;
    for animation in &semantic.animations {
        for bone in &animation.bones {
            for (_, quaternion) in &bone.rotation {
                let norm = quaternion.iter().map(|v| v * v).sum::<f32>().sqrt();
                quaternions += 1;
                invalid += usize::from(!norm.is_finite() || (norm - 1.0).abs() > 0.01);
            }
        }
    }
    println!(
        "external_dependencies={} sequences={} quaternion_keys={} invalid_norm={}",
        dependencies.len(),
        semantic.animations.len(),
        quaternions,
        invalid
    );
    anyhow::ensure!(
        invalid == 0,
        "installed animation keys contain invalid quaternion norms"
    );
    Ok(())
}
