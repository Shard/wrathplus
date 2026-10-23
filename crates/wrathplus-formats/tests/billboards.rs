//! An M2 batch of billboard cards on several bones: build 12340 keeps it whole on the joint
//! palette, where each card's own billboard bone turns it to the camera about its pivot.

use wrathplus_formats::{load_m2_mesh, load_wrath_render_model, open_chain, RenderSubmesh};

#[test]
fn candelabra_glow_cards_ride_one_billboard_bone_each() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = open_chain(&data).expect("open patch chain");
    // Five candles: one glow batch, each card's quad skinned to its own billboard bone, which the
    // joint palette turns to the camera (build 12340 keeps them in the batch).
    let path = "World\\Generic\\PassiveDoodads\\Lights\\CandelabraTallWall01.m2";
    let subs = load_m2_mesh(&mut chain, path).expect("load CandelabraTallWall01.m2");
    let model = load_wrath_render_model(&mut chain, path).expect("render model");

    let glow: Vec<&RenderSubmesh> = subs.iter().filter(|s| s.welded_billboard).collect();
    assert_eq!(glow.len(), 1, "one glow batch rides the billboard bones");
    let g = glow[0];
    assert_eq!(g.positions.len(), 20, "five quads");
    let mut bones: Vec<u16> = g
        .joints
        .iter()
        .zip(&g.weights)
        .flat_map(|(j, w)| j.iter().zip(w).filter(|(_, w)| **w > 0.0).map(|(j, _)| *j))
        .collect();
    bones.sort_unstable();
    bones.dedup();
    assert_eq!(bones.len(), 5, "one bone per candle: {bones:?}");
    for b in &bones {
        assert!(
            model.skeleton.bones[usize::from(*b)].billboard.is_some(),
            "bone {b} billboards"
        );
    }
    let mut pivots: Vec<[f32; 3]> = bones
        .iter()
        .map(|b| model.skeleton.bones[usize::from(*b)].pivot)
        .collect();
    pivots.dedup();
    assert_eq!(pivots.len(), 5, "five distinct candle pivots");
    assert!(
        subs.iter().any(|s| !s.welded_billboard),
        "the candelabra body is ordinary geometry"
    );
}
