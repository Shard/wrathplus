//! Animated M2 point lights: a torch's flicker, a spell light keyed on partway in. The 3.3.5
//! animate pass samples each light's diffuse colour, intensity and visibility byte every frame
//! (`0x82f0f0`) and re-gates the hardware light from that byte (`0x828a00`); a static light keeps
//! the [`WorldPointLight`] it spawned with.

use std::sync::Arc;

use wrathplus_formats::M2LightAnim;
use bevy::prelude::*;

use crate::lighting::WorldPointLight;
use crate::ribbons::RibbonSeq;

/// Drives a spawned [`WorldPointLight`] from its model light's tracks.
#[derive(Component)]
pub struct LightAnimator {
    anim: Arc<M2LightAnim>,
    /// The owner's sequence, as a ribbon reads it.
    seq: RibbonSeq,
    /// Seconds since spawn: a fixed sequence's clock.
    age: f32,
}

impl LightAnimator {
    pub fn new(anim: Arc<M2LightAnim>, seq: RibbonSeq) -> Self {
        Self {
            anim,
            seq,
            age: 0.0,
        }
    }
}

/// Per frame: re-samples every animated light on its owner's clock. No clock yet holds it dark,
/// as a host still being built has no armed sequence.
fn animate_lights(
    time: Res<Time>,
    hosts: Query<(&AnimationPlayer, &wrathplus_assets::ModelAnimations)>,
    mut lights: Query<(&mut LightAnimator, &mut WorldPointLight)>,
) {
    let dt = time.delta_secs().min(0.1);
    let scene_ms = time.elapsed_secs_f64() * 1000.0;
    for (mut a, mut light) in &mut lights {
        a.age += dt;
        let playing = crate::ribbons::playing_of(a.seq, &a.anim.sequences, a.age, &hosts);
        let (color, intensity) = match playing {
            Some(_) => a.anim.diffuse_at(crate::ribbons::seq_clock(
                playing,
                &a.anim.sequences,
                a.age,
                scene_ms,
            )),
            None => ([0.0; 3], 0.0),
        };
        let next = crate::terrain_stream::point_light(color, intensity);
        if *light != next {
            *light = next;
        }
    }
}

/// Registers the per-frame light animation; spawn sites insert [`LightAnimator`].
pub struct LightAnimPlugin;

impl Plugin for LightAnimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, animate_lights);
    }
}
