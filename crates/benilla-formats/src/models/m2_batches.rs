//! M2 chain loading: render submeshes through the v264 reader with its `00.skin`, bone spins,
//! sibling `.anim` files, and the owner-reach LOD rungs.

use anyhow::{Context, Result};

use crate::Chain;

use super::model_path;
use super::{ModelBlend, RenderSubmesh};

fn parent_dir(path: &str) -> &str {
    match path.rfind(['\\', '/']) {
        Some(i) => &path[..i],
        None => "",
    }
}

/// Load an M2 from the chain as render submeshes; creatures use [`load_m2_mesh_skinned`].
pub fn load_m2_mesh(chain: &mut Chain, raw_path: &str) -> Result<Vec<RenderSubmesh>> {
    load_m2_mesh_skinned(chain, raw_path, &[])
}

/// [`load_m2_mesh`] with `Monster1/2/3` filled from `skins` (`CreatureDisplayInfo` variations).
pub fn load_m2_mesh_skinned(
    chain: &mut Chain,
    raw_path: &str,
    skins: &[Option<String>],
) -> Result<Vec<RenderSubmesh>> {
    let path = model_path(raw_path);
    let dir = parent_dir(&path).to_string();
    let bytes = chain
        .read_file(&path)
        .with_context(|| format!("reading M2 {path}"))?;
    parse_chain_m2(chain, &path, &bytes, &dir, skins).with_context(|| format!("parsing M2 {path}"))
}

/// Parse a chain model with its sibling `00.skin`, as every build-12340 model needs.
fn parse_chain_m2(
    chain: &mut Chain,
    path: &str,
    bytes: &[u8],
    dir: &str,
    skins: &[Option<String>],
) -> Result<Vec<RenderSubmesh>> {
    let stem = path
        .strip_suffix(".m2")
        .with_context(|| format!("Wrath M2 path has no .m2 suffix: {path}"))?;
    let skin_path = format!("{stem}00.skin");
    let skin = chain
        .read_file(&skin_path)
        .with_context(|| format!("reading Wrath skin {skin_path}"))?;
    Ok(super::wrath::parse_wrath_render_model(bytes, &skin, dir, skins)?.submeshes)
}

/// [`crate::m2_bone_spins`] for a model read from the chain, with its sibling `.anim` files.
pub fn load_m2_bone_spins(
    chain: &mut Chain,
    raw_path: &str,
) -> Result<std::collections::HashMap<u16, crate::BoneSpin>> {
    let path = model_path(raw_path);
    let bytes = chain
        .read_file(&path)
        .with_context(|| format!("reading M2 {path}"))?;
    let external = chain_wrath_animations(chain, &path, &bytes)?;
    let external: Vec<Option<&[u8]>> = external.iter().map(|e| e.as_deref()).collect();
    Ok(crate::m2_bone_spins(&bytes, &external))
}

/// A v264 model's sibling `<stem><id:04>-<variation:02>.anim` payloads off the chain, indexed by
/// sequence: every sequence without its keys in the `.m2` names one.
pub(crate) fn chain_wrath_animations(
    chain: &mut Chain,
    m2_path: &str,
    bytes: &[u8],
) -> Result<Vec<Option<Vec<u8>>>> {
    let stem = m2_path.strip_suffix(".m2").unwrap_or(m2_path);
    let dependencies = benilla_m2::wrath::wrath_animation_dependencies(bytes)
        .map_err(|e| anyhow::anyhow!("reading {m2_path} animation dependencies: {e}"))?;
    let mut external: Vec<Option<Vec<u8>>> = Vec::new();
    for dep in dependencies {
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
    Ok(external)
}

/// How far a model's transparent batches sort from its origin, yards: the reference draws a
/// model's emitters after its batches, which one sorted list gets by biasing past them. A batch
/// sorts at its bind-pose box centre, not [`super::M2Bounds::vert_max_from_origin`].
pub fn m2_owner_reach(subs: &[RenderSubmesh]) -> f32 {
    subs.iter()
        .filter(|s| {
            s.additive
                || matches!(
                    s.blend,
                    ModelBlend::Blend | ModelBlend::Mod | ModelBlend::Mod2x
                )
        })
        .filter(|s| !s.positions.is_empty())
        .map(|s| {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in &s.positions {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            // A length, so the same in WoW and Bevy axes (a signed axis permutation apart).
            let c: [f32; 3] = std::array::from_fn(|k| 0.5 * (lo[k] + hi[k]));
            (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
        })
        .fold(0.0f32, f32::max)
}

/// The `Transparent3d` depth bias sorting an effect after its owner's transparent batches and no
/// further: the next whole yard above the reach, since Bevy keys pipelines on `depth_bias as i32`
/// and a batch centred at the reach must not tie.
pub fn owner_last_rung(reach_world: f32) -> f32 {
    // Deviation: owners past the ceiling (Maraudon's waterfalls, the glue screens) keep the
    // interleave with their emitters, because a bias past a 200-yard owner would jump every
    // transparent surface in the zone.
    const MAX_RUNG: f32 = 32.0;
    (reach_world.max(0.0).floor() + 1.0).min(MAX_RUNG)
}

/// The rungs a mesh-shard material may carry, a pipeline-key axis through `depth_bias`: a closed
/// set `pipe_warm` compiles behind the loading cover. Most spell-kit owners fit the first.
pub const OWNER_RUNG_BUCKETS: [f32; 3] = [4.0, 12.0, 32.0];

/// Snap a rung up to its bucket, the last one past the end, so the key space stays closed.
pub fn owner_last_rung_bucket(rung: f32) -> f32 {
    for b in OWNER_RUNG_BUCKETS {
        if rung <= b {
            return b;
        }
    }
    OWNER_RUNG_BUCKETS[OWNER_RUNG_BUCKETS.len() - 1]
}

#[cfg(test)]
mod tests {
    use super::super::{BillboardKind, CharSkinSlot};
    use super::*;

    /// A bucket never sorts under the exact rung, which its sibling quad cloud keeps.
    #[test]
    fn owner_rung_buckets_cover_every_reachable_rung() {
        let max = OWNER_RUNG_BUCKETS[OWNER_RUNG_BUCKETS.len() - 1];
        assert_eq!(owner_last_rung(f32::MAX), max, "one shared ceiling");
        let mut prev = 0.0f32;
        for r in 1..=32 {
            let b = owner_last_rung_bucket(r as f32);
            assert!(b >= r as f32, "bucket({r}) sorts under the exact rung");
            assert!(b >= prev, "bucket must be monotonic");
            assert!(
                OWNER_RUNG_BUCKETS.contains(&b),
                "bucket({r}) not in the set"
            );
            prev = b;
        }
        // The snap-up scan relies on the set ascending.
        assert!(OWNER_RUNG_BUCKETS.windows(2).all(|w| w[0] < w[1]));
    }

    /// `TaurenMale.m2`'s type-8 fur, opaque core and alpha-cut two-sided fringe alike, surfaces as
    /// [`CharSkinSlot::SkinExtra`]; unmapped, it draws flat white.
    #[test]
    fn tauren_fur_batches_carry_the_skin_extra_slot() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let subs = load_m2_mesh(&mut chain, "Character\\Tauren\\Male\\TaurenMale.m2")
            .expect("parse TaurenMale");
        let extra: Vec<_> = subs
            .iter()
            .filter(|s| s.char_slot == Some(CharSkinSlot::SkinExtra))
            .collect();
        assert!(
            extra.len() > 20,
            "the tauren fur is many batches, got {}",
            extra.len()
        );
        assert!(
            extra.iter().all(|s| s.texture.is_none()),
            "type 8 has no embedded path — the spawn site fills it"
        );
        assert!(
            extra
                .iter()
                .any(|s| matches!(s.blend, ModelBlend::Opaque) && !s.two_sided),
            "the opaque single-sided fur core flavor exists"
        );
        assert!(
            extra
                .iter()
                .any(|s| matches!(s.blend, ModelBlend::AlphaTest) && s.two_sided),
            "the alpha-cut two-sided fringe flavor exists"
        );
    }

    /// `Axe_2H_Horde_C_01.m2`'s ARMORREFLECT layer is material `{flags 0x10, blendMode 6}`.
    #[test]
    fn whirlwind_axe_reflect_layer_is_mod2x() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let subs = load_m2_mesh(
            &mut chain,
            "Item\\ObjectComponents\\Weapon\\Axe_2H_Horde_C_01.m2",
        )
        .expect("parse Axe_2H_Horde_C_01");
        assert_eq!(subs.len(), 3, "handle + blade + reflect layer");
        let object: Vec<_> = subs
            .iter()
            .filter(|s| s.char_slot == Some(CharSkinSlot::Object))
            .collect();
        assert_eq!(
            object.len(),
            2,
            "handle + blade bind the item's Object skin"
        );
        let reflect = subs
            .iter()
            .find(|s| {
                s.texture
                    .as_deref()
                    .is_some_and(|t| t.to_ascii_uppercase().contains("ARMORREFLECT"))
            })
            .expect("the ARMORREFLECT layer exists");
        assert_eq!(reflect.blend, ModelBlend::Mod2x);
        assert!(!reflect.additive, "Mod2x is a multiply, not an add");
        assert!(reflect.no_depth_write, "render flag 0x10");
    }

    /// The questgiver `?` bobs z 0.000 → −0.089 in anim 0 and +0.517 → +0.427 in anim 190, the
    /// raised bob the client arms under an overhead name (`0x6076c0`).
    #[test]
    fn questionmark_marker_batch_carries_both_bob_loops() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let subs = load_m2_mesh(&mut chain, "Interface\\Buttons\\TalkToMeQuestionMark.mdx")
            .expect("parse TalkToMeQuestionMark");
        let bb = subs
            .iter()
            .find_map(|s| s.billboard.as_ref())
            .expect("the ? marker rides a billboard bone");
        assert!(matches!(bb.kind, BillboardKind::LockZ));
        let bob = |id: u16| {
            bb.seq_translations
                .iter()
                .find(|(a, _)| *a == id)
                .map(|(_, l)| l)
        };
        let low = bob(0).expect("the load-arm loop bakes");
        assert_eq!(low.duration_ms, 1533, "the first sequence's band");
        assert!(low.keys.len() >= 2, "a moving track: {:?}", low.keys);
        let (a, b) = (low.sample(0), low.sample(low.duration_ms / 2));
        assert_ne!(a, b, "the sampled offset varies across the loop");
        assert!(
            low.keys.iter().all(|(_, v)| v[2] <= 0.0),
            "anim 0 bobs at/below the attach point: {:?}",
            low.keys
        );
        let raised = bob(190).expect("the raised loop bakes");
        assert_eq!(raised.duration_ms, 1533, "same period as the low bob");
        assert!(
            raised.keys.iter().all(|(_, v)| v[2] > 0.4),
            "anim 190 is the authored raised bob: {:?}",
            raised.keys
        );
    }
}
