//! The view-class census and the warm pass's release verdict.
use super::{verdict, view_class, Verdict, ViewClass, WARM_HARD_CAP_SECS, WARM_STALL_SECS};
use bevy::camera::{OrthographicProjection, PerspectiveProjection, Projection};
use bevy::render::view::Msaa;

fn perspective() -> Projection {
    Projection::Perspective(PerspectiveProjection::default())
}

fn orthographic() -> Projection {
    Projection::Orthographic(OrthographicProjection::default_3d())
}

/// A warm set with no orthographic arm: the world camera's Perspective and the two
/// `Msaa::Off` booth classes.
fn warm_set_without_an_orthographic_arm() -> Vec<ViewClass> {
    vec![
        view_class(Some(&perspective()), Some(&Msaa::Off), true),
        view_class(Some(&perspective()), Some(&Msaa::Sample4), true),
        view_class(None, Some(&Msaa::Off), true),
    ]
}

/// The tile camera is `Msaa::Off` like every booth, so a census keyed on sample count alone
/// would miss it.
#[test]
fn the_census_fires_for_an_unwarmed_orthographic_camera() {
    let warm = warm_set_without_an_orthographic_arm();
    let tile_cam = view_class(Some(&orthographic()), Some(&Msaa::Off), true);
    assert!(
        !warm.contains(&tile_cam),
        "the ui_models tile camera's view key must read as unwarmed against a warm set that \
         has no orthographic arm — the census must report it"
    );
    assert_eq!(
        tile_cam.samples, 1,
        "the tile camera is Msaa::Off, like every booth"
    );
}

#[test]
fn the_census_is_silent_for_every_warmed_class() {
    let warm = warm_set_without_an_orthographic_arm();
    for class in &warm {
        assert!(warm.contains(class));
    }
    let booth_after_first_bake = view_class(None, Some(&Msaa::Off), true);
    assert!(
        warm.contains(&booth_after_first_bake),
        "a booth's runtime-installed custom projection is the NONSTANDARD class, warmed by \
         the twin booth"
    );
}

/// `Hdr` is a key bit and gates two more (`bevy_pbr` `mesh.rs:418`).
#[test]
fn dropping_hdr_is_a_different_view_key() {
    let with = view_class(Some(&perspective()), Some(&Msaa::Off), true);
    let without = view_class(Some(&perspective()), Some(&Msaa::Off), false);
    assert_ne!(with, without);
    assert!(!warm_set_without_an_orthographic_arm().contains(&without));
}

/// A slow machine keeps the cover while the pass progresses, well past the old 10 s timeout; a
/// stall or the cap releases it, and a drain always wins.
#[test]
fn the_cover_waits_on_progress_not_on_wall_time() {
    // 25 s in, still revealing (progress a frame ago): wait.
    assert_eq!(verdict(25.0, 0.0, 24.9, false), Verdict::Wait);
    // Nothing revealed or settled for the stall window: release.
    assert_eq!(
        verdict(25.0, 0.0, 25.0 - WARM_STALL_SECS, false),
        Verdict::Stalled
    );
    // Progressing, but at the hard cap: release.
    assert_eq!(
        verdict(WARM_HARD_CAP_SECS, 0.0, WARM_HARD_CAP_SECS - 0.1, false),
        Verdict::Capped
    );
    // Drained wins over both.
    assert_eq!(
        verdict(WARM_HARD_CAP_SECS + 1.0, 0.0, 0.0, true),
        Verdict::Drained
    );
}
