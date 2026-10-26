//! [`super::uv_clamp_window`] and [`super::tiling_axes`] on shapes the stock UI draws.

use super::{tiling_axes, uv_clamp_window, UvRect};

/// `POIIcons` is 128², and a world-map POI samples its cell (7, 1).
#[test]
fn a_poi_icons_cell_stops_half_a_texel_inside_itself() {
    let uv = UvRect::from_tex_coords([0.875, 1.0, 0.125, 0.25]);
    let w = uv_clamp_window(&uv, (128, 128)).expect("an atlas cell is clamped");
    let half = 0.5 / 128.0;
    assert!((w[0] - (0.875 + half)).abs() < 1e-6, "u_min {}", w[0]);
    assert!((w[1] - (0.125 + half)).abs() < 1e-6, "v_min {}", w[1]);
    assert!((w[2] - (1.0 - half)).abs() < 1e-6, "u_max {}", w[2]);
    assert!((w[3] - (0.25 - half)).abs() < 1e-6, "v_max {}", w[3]);
}

#[test]
fn the_whole_texture_asks_for_no_window() {
    assert!(uv_clamp_window(&UvRect::FULL, (128, 128)).is_none());
}

/// The stance bar's middle strip, `SetTexCoord(0, n-2, 0, 1)` (`BonusActionBarFrame.lua:173`).
#[test]
fn a_one_axis_strip_wraps_that_axis_only() {
    assert_eq!(
        tiling_axes(&UvRect::from_tex_coords([0.0, 2.0, 0.0, 1.0])),
        (true, false)
    );
    assert_eq!(
        tiling_axes(&UvRect::from_tex_coords([0.0, 1.0, 0.0, 3.0])),
        (false, true)
    );
}

#[test]
fn a_bounded_mapping_tiles_nothing() {
    assert_eq!(
        tiling_axes(&UvRect::from_tex_coords([0.0, 1.0, 0.0, 1.0])),
        (false, false)
    );
    assert_eq!(tiling_axes(&UvRect::FULL), (false, false));
    assert_eq!(
        tiling_axes(&UvRect::from_tex_coords([0.453125, 0.875, 0.0, 1.0])),
        (false, false)
    );
}

#[test]
fn a_two_axis_tile_wraps_both() {
    assert_eq!(
        tiling_axes(&UvRect::from_tex_coords([0.0, 4.0, 0.0, 2.5])),
        (true, true)
    );
}

#[test]
fn a_tiling_axis_is_left_alone_while_its_bounded_partner_is_not() {
    let uv = UvRect::from_tex_coords([0.0, 3.0, 0.0, 0.5]);
    let w = uv_clamp_window(&uv, (64, 64)).expect("the v axis is a bounded crop");
    assert!(w[0] > w[2], "u tiles, so its axis must read as OFF: {w:?}");
    assert!((w[1] - 0.5 / 64.0).abs() < 1e-6);
    assert!((w[3] - (0.5 - 0.5 / 64.0)).abs() < 1e-6);
}

#[test]
fn a_mirrored_slice_is_clamped_by_its_extents() {
    let uv = UvRect::from_tex_coords([0.5, 0.25, 0.0, 1.0]);
    let w = uv_clamp_window(&uv, (64, 64)).expect("a mirrored cell is still a cell");
    assert!((w[0] - (0.25 + 0.5 / 64.0)).abs() < 1e-6);
    assert!((w[2] - (0.5 - 0.5 / 64.0)).abs() < 1e-6);
    assert!(
        w[1] > w[3],
        "v spans the whole texture, so it reads as OFF: {w:?}"
    );
}

#[test]
fn a_sub_texel_crop_pins_to_the_texel_it_names() {
    let uv = UvRect::from_tex_coords([0.5, 0.505, 0.0, 1.0]);
    let w = uv_clamp_window(&uv, (64, 64)).expect("still a crop");
    assert!((w[0] - 0.5025).abs() < 1e-6);
    assert!(w[0] <= w[2], "the axis must stay enabled: {w:?}");
    assert!((w[2] - 0.5025).abs() < 1e-6);
}
