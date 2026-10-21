//! The plate's widget constructors: its two bars, its textures and its two FontStrings, in the
//! shapes build 12340's ctor gives them (`0x98f790`).

use super::super::model::Model;
use super::super::{BlendMode, JustifyV};
use super::{BAR_FILL_TEXTURE, BAR_LEVEL, PLATE_FONT};
use crate::order::DrawLayer;
use crate::widget::{FrameHandle, FrameKind, KindState, RegionHandle, RegionKind};

/// One of the plate's two bars, the health bar's `CGSimpleHealthBar` and the cast bar's
/// `CSimpleStatusBar`: one level below the plate so its border draws over the fill
/// (`0x4910a0(level - 1)`), filled with `UI-TargetingFrame-BarFill` on the default BLEND.
pub(super) fn status_bar(
    model: &mut Model,
    plate: FrameHandle,
    strata: crate::order::Strata,
) -> (FrameHandle, RegionHandle) {
    let bar = model.arena.create(FrameKind::StatusBar, None, Some(plate));
    model.arena.set_frame_strata(bar, strata);
    model.arena.set_frame_level(bar, BAR_LEVEL, true);
    let fill = model
        .arena
        .create_region(bar, RegionKind::Texture, DrawLayer::Artwork, 0)
        .expect("live bar");
    let fill_data = model.region_data.entry(fill).or_default();
    fill_data.texture = Some(BAR_FILL_TEXTURE.to_string());
    fill_data.blend = BlendMode::Blend;
    if let Some(KindState::StatusBar(sb)) = model.arena.frame_mut(bar).map(|f| &mut f.kind_state) {
        sb.bar = Some(fill);
        sb.min = 0.0;
        sb.max = 1.0;
        sb.value = 0.0;
    }
    (bar, fill)
}

/// One of the plate's textures, with its path and the blend mode the ctor sets (`0x7703f0`).
pub(super) fn texture(
    model: &mut Model,
    frame: FrameHandle,
    layer: DrawLayer,
    path: &str,
    blend: BlendMode,
) -> RegionHandle {
    let rh = model
        .arena
        .create_region(frame, RegionKind::Texture, layer, 0)
        .expect("live plate");
    let data = model.region_data.entry(rh).or_default();
    data.texture = Some(path.to_string());
    data.blend = blend;
    rh
}

/// One of the plate's two FontStrings, OVERLAY as the ctor re-layers them (`0x7cb438`,
/// `0x7cb4ea`). `justify_v` seats the name by its ink: our shaper's Friz line box is ascent-heavy
/// and would drop it ~8 px below where the reference's metrics put it.
pub(super) fn font_string(
    model: &mut Model,
    frame: FrameHandle,
    justify_v: JustifyV,
) -> RegionHandle {
    let rh = model
        .arena
        .create_region(frame, RegionKind::FontString, DrawLayer::Overlay, 0)
        .expect("live plate");
    let data = model.region_data.entry(rh).or_default();
    data.font_path = Some(PLATE_FONT.to_string());
    data.font_explicit.face = true;
    data.justify.set_v(justify_v);
    rh
}
