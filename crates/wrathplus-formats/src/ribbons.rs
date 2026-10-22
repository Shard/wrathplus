//! M2 ribbon emitter definitions (weapon trails, wisp streamers, missile trails). Build 12340
//! models are read by `models::wrath_effects`; the visibility track sets the enable byte, and a
//! cleared byte skips the ribbon's whole draw.

use crate::value_track::{SeqTable, SeqTrack};
use crate::ParticleBlend;

/// One M2 ribbon emitter, as the renderer needs it.
#[derive(Debug, Clone)]
pub struct RibbonEmitterDef {
    pub bone: u16,
    /// The origin in the host bone's frame, times the bone matrix every frame (`0x718960`).
    pub position: [f32; 3],
    /// `textureIndices[0]`'s path; `None` if unresolved, and the ribbon is skipped.
    pub texture: Option<String>,
    /// `materialIndices[0]`'s render-flags blend, folded into a `ParticleBlend`.
    pub blend: ParticleBlend,
    /// The raw blend behind the lossy [`Self::blend`] fold, when the material resolves.
    pub blend_mode: Option<u16>,
    /// Trail tint (colorTrack, RGB 0..1; constant white when unkeyed).
    pub color: SeqTrack<[f32; 3]>,
    /// Trail opacity (alphaTrack, int16 / 32767; 1.0 when unkeyed, the instance state's load
    /// value at `0x832ea0`).
    pub alpha: SeqTrack,
    /// Half-widths (yards) above and below the path; each edge keeps the width it was born with.
    pub height_above: SeqTrack,
    pub height_below: SeqTrack,
    /// Edges per second; with `edge_lifetime`, sizes the ring.
    pub edges_per_second: f32,
    /// Edge lifetime in seconds, clamped to at least 0.25 at load as the reference does.
    pub edge_lifetime: f32,
    /// Downward sag applied to live edges (the reference's `2·g·dt` per-frame term).
    pub gravity: f32,
    /// Texture atlas tiling (1×1 is the whole texture) and the slot index.
    pub tile_rows: u16,
    pub tile_cols: u16,
    pub tex_slot: u16,
    /// The `+0xc0` enable track, `None` when always on; otherwise [`RibbonVisibility::at`] is
    /// sampled on the host's playing sequence every frame.
    pub visible: Option<RibbonVisibility>,
    /// The model's sequences, the look tracks' slot clocks. Empty for the single-lane 1.12
    /// parse, whose keys are rebased onto sequence 0 and sampled on seconds since spawn.
    pub sequences: SeqTable,
    /// The first material's flag `0x1`: the ribbon draws unlit (`0x980b70` sets lighting off,
    /// `0x8731c0`); otherwise the scene lights it. 1,445 of the install's 1,502 ribbons.
    pub unlit: bool,
    /// The first material's flag `0x2`: no fog (`0x873390`); 1,181 of 1,502.
    pub unfogged: bool,
}

/// The `+0xc0` enable track per `AnimationData.dbc` id: step keys in band-local seconds, the
/// first being the value the band opens on.
#[derive(Debug, Clone, PartialEq)]
pub struct RibbonVisibility {
    by_anim: std::collections::HashMap<u16, Vec<(f32, bool)>>,
}

impl RibbonVisibility {
    pub(crate) fn from_native(by_anim: std::collections::HashMap<u16, Vec<(f32, bool)>>) -> Self {
        Self { by_anim }
    }

    /// Whether the trail emits `t` seconds into `anim`, stepped. An unauthored sequence takes
    /// Stand's answer, and without one the load default, on (`0x70f80e`).
    pub fn at(&self, anim: u16, t: f32) -> bool {
        let Some(keys) = self.by_anim.get(&anim).or_else(|| self.by_anim.get(&0)) else {
            return true;
        };
        keys.iter()
            .take_while(|&&(kt, _)| kt <= t)
            .last()
            .or(keys.first())
            .is_some_and(|&(_, on)| on)
    }

    /// Every `(anim id, keys)` pair, for instruments that census the gate.
    pub fn per_anim(&self) -> impl Iterator<Item = (u16, &[(f32, bool)])> {
        self.by_anim.iter().map(|(&a, k)| (a, k.as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_samples_step_and_falls_back_to_stand() {
        let vis = RibbonVisibility {
            by_anim: [
                (0u16, vec![(0.0, false)]),
                (153u16, vec![(0.0, false), (0.2, true), (1.4, false)]),
            ]
            .into_iter()
            .collect(),
        };
        assert!(!vis.at(153, 0.0));
        assert!(!vis.at(153, 0.199));
        assert!(vis.at(153, 0.2), "step: the key's value takes effect AT it");
        assert!(vis.at(153, 1.399));
        assert!(!vis.at(153, 1.4));
        assert!(!vis.at(147, 0.0), "unlisted sequence borrows Stand");
        let no_stand = RibbonVisibility {
            by_anim: [(153u16, vec![(0.0, false)])].into_iter().collect(),
        };
        assert!(no_stand.at(147, 0.0), "no answer at all ⇒ the load default");
    }
}
