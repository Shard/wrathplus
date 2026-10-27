use super::*;

#[test]
fn the_cover_counts_frames_to_the_glass_and_resets_the_moment_it_drops() {
    let mut cover = EntryCover::default();
    assert!(
        cover.presented(),
        "no cover up: nothing is watching the previous present, so nothing waits"
    );
    assert!(!cover.owes_a_present(), "no cover owes no present");

    cover.tick(true);
    assert_eq!(cover.frames(), 1);
    assert!(
        !cover.presented(),
        "the cover's own render has not committed yet"
    );
    assert!(cover.owes_a_present());

    cover.tick(true);
    assert!(!cover.presented(), "one render committed, not two");

    cover.tick(true);
    assert!(
        cover.presented(),
        "at COVER_PRESENT_FRAMES the cover is provably on the glass"
    );
    assert!(!cover.owes_a_present());

    // A reveal drops the count to zero in one frame; the next raise earns its own.
    cover.tick(false);
    assert_eq!(cover.frames(), 0);
    assert!(cover.presented());
    cover.tick(true);
    assert!(!cover.presented(), "the next raise starts its own count");
}
/// vmangos can relocate a character out of an expired instance inside `Player::LoadFromDB`, so
/// `SMSG_LOGIN_VERIFY_WORLD` names another map than the roster did.
#[test]
fn a_relocating_snap_cannot_move_the_art_the_screen_was_raised_with() {
    let mut screen = LoadingScreen::default();
    // The pick edge: art from the roster row, Shadowfang Keep, snap still to come.
    screen.raise("world entry", true, Some(33), 0.0);
    screen.tip_edge = Some(TipEdge::Pick);
    screen.displayed = 0.4;

    // `SMSG_LOGIN_VERIFY_WORLD`: the server seats us on map 0, at the instance's exit.
    assert!(
        !screen.far_snap(0),
        "a screen is already up — its own snap must not raise a second one"
    );
    assert_eq!(
        screen.map,
        Some(33),
        "the backdrop is the raise's and holds the whole way — the report"
    );
    assert!(
        !screen.awaiting_snap,
        "the snap the raise was waiting for has landed"
    );
    assert_eq!(
        screen.tip_edge,
        Some(TipEdge::Pick),
        "and the tip just picked survives it — a raise is the only thing that could \
         have cleared it, and no raise happened"
    );
    assert!(
        (screen.displayed - 0.4).abs() < f32::EPSILON,
        "nor does the bar restart mid-load"
    );
    assert!(screen.active, "the screen stays up across its own snap");
}

/// A cross-map port with no announcing edge still gets a cover: our backstop, which the
/// reference's blocking world load does not need.
#[test]
fn a_snap_with_nothing_covering_it_asks_for_a_raise() {
    let mut screen = LoadingScreen::default();
    assert!(screen.far_snap(1), "nothing is covering this load");
    assert!(
        !screen.active,
        "far_snap answers the question; the caller does the raising, with its own reason"
    );
}

#[test]
fn only_the_dismiss_clears_the_tip() {
    let mut screen = LoadingScreen::default();
    screen.raise("world entry", true, Some(0), 0.0);
    screen.tip_edge = Some(TipEdge::Pick);

    screen.raise("transfer pending", true, Some(389), 0.0);
    assert_eq!(
        screen.tip_edge,
        Some(TipEdge::Pick),
        "no raise clears a tip"
    );

    screen.dismiss();
    assert_eq!(screen.tip_edge, Some(TipEdge::Clear), "the dismiss does");
    assert!(!screen.active);
}

/// The reference's raises (`0x406640`/`0x4072c0`) store the map id with no guard.
#[test]
fn a_raise_onto_a_live_screen_repoints_the_map_but_never_the_picture() {
    let mut screen = LoadingScreen::default();
    screen.raise("world entry", true, Some(33), 0.0);
    screen.art_resolved = true; // the first plain-draw frame resolved Shadowfang Keep's art

    screen.raise("transfer pending", true, Some(389), 0.0);
    assert_eq!(screen.map, Some(389), "the map id is re-pointed, unguarded");
    assert!(
        screen.art_resolved,
        "…and the picture is not: the texture is still the one this screen resolved"
    );

    // The dismiss is the only thing that lets the next screen resolve its own.
    screen.dismiss();
    assert!(!screen.art_resolved);
}
