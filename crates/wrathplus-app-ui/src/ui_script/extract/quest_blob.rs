//! A `QuestPOIFrame`'s quest blobs ([`QuestBlobDraw`]): each polygon's inside as a fan of the
//! fill art tiled across the frame, then its rim as one strip of the border art per edge.
//! Labelled: 12340's geometry builder (`0x58fef0`/`0x58f1a0`) is unread, so the art's scale (one
//! tile per [`TILE`] frame units) and the rim's width ([`RIM`] units times the border scalar) are
//! ours.

use bevy::prelude::*;
use wrathplus_assets::WorldAssets;
use wrathplus_ui::script::QuestBlobDraw;

use crate::overlay::{UiQuad, UvRect};

/// Frame units per repeat of the fill art.
const TILE: f32 = 64.0;
/// The rim's width in frame units at border scalar 1.
const RIM: f32 = 16.0;

/// Appends the blobs over `rect` (screen px, y down) at `z`; `alpha` is the frame's, `s` the UI
/// scale. Missing art draws nothing.
pub(super) fn push(
    draw: &QuestBlobDraw,
    (z, alpha): (u64, f32),
    (rect, s): (Rect, f32),
    assets: &mut Option<ResMut<WorldAssets>>,
    images: &mut Assets<Image>,
    out: &mut Vec<UiQuad>,
) {
    let Some(assets) = assets.as_mut() else {
        return;
    };
    let at = |p: [f32; 2]| rect.min + Vec2::new(p[0], p[1]) * rect.size();
    let tile = TILE * s;
    let mut quad = |texture: &Handle<Image>, a: f32, corners: [Vec2; 4], uvs: [[f32; 2]; 4]| {
        out.push(UiQuad {
            rect,
            z_key: z,
            texture: Some(texture.clone()),
            uv: UvRect::from_corners(uvs),
            color: [1.0, 1.0, 1.0, alpha * a],
            corners: Some(corners),
            ..default()
        });
    };
    let fill = draw
        .fill
        .as_deref()
        .and_then(|p| assets.sprite_texture_tiled(p, images));
    if let Some(texture) = fill {
        let a = f32::from(draw.fill_alpha) / 255.0;
        let uv = |p: Vec2| [(p.x - rect.min.x) / tile, (p.y - rect.min.y) / tile];
        for ring in &draw.polygons {
            let pts: Vec<Vec2> = ring.iter().map(|&p| at(p)).collect();
            for i in 1..pts.len().saturating_sub(1) {
                let (o, b, c) = (pts[0], pts[i], pts[i + 1]);
                quad(&texture, a, [o, b, c, c], [uv(o), uv(b), uv(c), uv(c)]);
            }
        }
    }
    let border = draw
        .border
        .as_deref()
        .and_then(|p| assets.sprite_texture_tiled(p, images));
    if let Some(texture) = border {
        let a = f32::from(draw.border_alpha) / 255.0;
        let half = RIM * draw.border_scalar * s / 2.0;
        for ring in &draw.polygons {
            let pts: Vec<Vec2> = ring.iter().map(|&p| at(p)).collect();
            let mut run = 0.0;
            for (i, &p) in pts.iter().enumerate() {
                let q = pts[(i + 1) % pts.len()];
                let n = (q - p).perp().normalize_or_zero() * half;
                let len = p.distance(q) / tile;
                let (u0, u1) = (run, run + len);
                run = u1;
                let corners = [p - n, q - n, q + n, p + n];
                quad(
                    &texture,
                    a,
                    corners,
                    [[u0, 0.0], [u1, 0.0], [u1, 1.0], [u0, 1.0]],
                );
            }
        }
    }
}
