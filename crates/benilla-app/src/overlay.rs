//! The world-layer overlay helpers: what world systems that draw over the 3D scene (floating
//! combat text, chat bubbles, V-plates, the minimap composite, the pipeline warm-up) share with
//! the player-UI pass. `ui_pass` re-exports them.

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

/// The append lane's z bands, far below the scripted UI's keys (a [`benilla_ui::order::ZKey`]
/// region sets `1 << 20`), so the whole lane paints under the player UI.
pub(crate) mod overlay_z {
    /// Floating combat, XP and honor numbers, drawn by the client in the world scene under all UI.
    pub(crate) const WORLD_TEXT: u64 = 0;
    /// The chat bubbles: one frame level per live bubble, farthest from the camera first.
    pub(crate) const BUBBLE: u64 = 1 << 8;
    /// Keys per bubble level: its bg, edge, tail and text.
    pub(crate) const BUBBLE_STRIDE: u64 = 4;
    /// The V-plates, over every bubble; a unit never shows both, so this orders only across units.
    pub(crate) const VPLATE: u64 = 1 << 16;
    /// The highest bubble level below [`VPLATE`], a guard rail far past any real count (a bubble
    /// needs a speaker within 20 yd).
    pub(crate) const BUBBLE_MAX_LEVEL: u64 = (VPLATE - BUBBLE) / BUBBLE_STRIDE - 1;
}

/// The render layer this pass's camera and batches share, disjoint from the world camera's 0.
const UI_RENDER_LAYER: usize = 1;

pub(crate) fn ui_render_layers() -> RenderLayers {
    RenderLayers::layer(UI_RENDER_LAYER)
}

/// The mesh every `UiQuadMaterial` quad outside the batches draws with: a 1×1 rectangle placed by
/// its Transform. Its layout (POSITION + NORMAL + UV_0) keys a second pipeline beside the batch
/// mesh's (POSITION + UV_0 + COLOR), which `pipe_warm` warms by drawing this same mesh.
pub(crate) fn tile_quad_mesh() -> Mesh {
    Rectangle::new(1.0, 1.0).into()
}

/// Project a world point for a world-anchored overlay, with the reference projector's verdict:
/// `None` does not draw. `0x483ee0` rejects a point behind the near plane (`0x483f7a`) or outside
/// the WorldFrame's region (`[0xb4b2bc]`; `0x484075`, `0x484086`, `0x484096`, `0x4840a6`), which
/// `0x483970` mirrors ÷G44/÷G48 so the aspect cancels: exactly the viewport, inclusive. A viewport
/// rejection has already written a plausible out-param (`0x484065`, `0x484067`), so the verdict
/// must be honoured; plate and world-text callers destroy what they were placing. The chat bubble
/// does not call this: its seat ignores the verdict, and a bubble slides off with its speaker.
pub(crate) fn project_overlay(
    cam: &Camera,
    cam_tf: &GlobalTransform,
    world: Vec3,
    viewport: Vec2,
) -> Option<Vec2> {
    // Bevy's `Err` is the near-plane half (`0x483f7a`); `accepts` is the viewport half.
    let p = cam.world_to_viewport(cam_tf, world).ok()?;
    accepts(p, viewport).then_some(p)
}

fn accepts(p: Vec2, viewport: Vec2) -> bool {
    (0.0..=viewport.x).contains(&p.x) && (0.0..=viewport.y).contains(&p.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The WorldFrame's region mirrored ÷G44/÷G48 (`0x483970`) is the viewport, inclusive. The
    /// last row is a measured unit two thirds of a screen below the bottom edge.
    #[test]
    fn the_accept_region_is_the_viewport_inclusive() {
        let vp = Vec2::new(1440.0, 810.0);
        assert!(accepts(Vec2::new(720.0, 405.0), vp));
        assert!(accepts(Vec2::ZERO, vp), "the corner is in view");
        assert!(accepts(vp, vp), "so is the far corner");
        for out in [
            Vec2::new(-0.5, 405.0),
            Vec2::new(1440.5, 405.0),
            Vec2::new(720.0, -0.5),
            Vec2::new(720.0, 810.5),
            Vec2::new(1409.0, 1332.0),
        ] {
            assert!(!accepts(out, vp), "{out:?} is not on screen");
        }
    }
}
