//! The world map's blackout through a resize.

use wrathplus_ui::script::{QuadContent, UiScript};

/// `BlackoutWorld` is anchored to the map's own corners (WorldMapFrame.xml:8-14) and the map to
/// the screen (`setAllPoints`, :5): its 1.12 sizing in `WorldMapFrame_OnLoad` is commented out
/// (WorldMapFrame.lua:88-99), so the blackout follows a resize through the anchors alone.
#[test]
fn the_blackout_follows_the_map_and_a_resize() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1600.0, 900.0);
    let failures = super::super::load_default_ui(&mut s);
    assert!(failures.is_empty(), "manifest load errors: {failures:#?}");
    s.run("ToggleFrame(WorldMapFrame)").unwrap();
    s.resolve();
    // The drawn quad, the map's colour-only BACKGROUND texture: its extent comes from its two
    // anchors. (`GetWidth` on an unsized region answers its declared size here, not the
    // anchored extent; that getter is untraced for 3.3.5.)
    let size = |s: &UiScript| {
        let rect = s
            .extract()
            .into_iter()
            .filter(|q| {
                matches!(q.content, QuadContent::Texture { path: None, .. })
                    && s.quad_owner_name(q.target).as_deref() == Some("WorldMapFrame")
            })
            .max_by(|a, b| {
                let area = |q: &wrathplus_ui::script::ExtractedQuad| {
                    q.rect.map_or(0.0, |r| r.width() * r.height())
                };
                area(a).total_cmp(&area(b))
            })
            .and_then(|q| q.rect)
            .expect("the blackout draws");
        (f64::from(rect.width()), f64::from(rect.height()))
    };
    assert_eq!(
        size(&s),
        (1600.0, 900.0),
        "the open map's blackout spans the screen"
    );

    assert!(s.set_screen_size(1920.0, 1080.0), "the size changed");
    super::super::manifest::on_screen_resized(&s);
    s.resolve();
    assert_eq!(
        size(&s),
        (1920.0, 1080.0),
        "and the new screen after a resize"
    );
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}
