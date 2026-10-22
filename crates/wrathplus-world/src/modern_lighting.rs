//! Renderer policy, deliberately independent of the app's Experience registry.
//! The plain renderer defaults to Classic; the app publishes resolved typed features each frame.

use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, ExtractResource)]
pub struct ModernLighting {
    pub shadows: bool,
    pub fog: bool,
    pub bloom: bool,
    pub tone_mapping: bool,
    pub colour_grading: bool,
}

impl ModernLighting {
    pub fn post_process(self) -> bool {
        self.bloom || self.tone_mapping || self.colour_grading
    }
}

/// Occupies only previously zero spare lanes. Classic is a byte-identical no-op.
/// Also packs the shadow opt-in into row 12.w; all disabled lanes stay untouched.
/// Height is relative to the eye, avoiding a sea-level assumption on elevated continents.
pub(crate) fn pack_fog(rows: &mut [[f32; 4]; 21], policy: ModernLighting, eye_y: f32) {
    if policy.shadows {
        rows[12][3] = 1.0;
    }
    if policy.fog {
        rows[5][2] = 1.0;
        rows[19][2] = eye_y - 8.0;
        rows[19][3] = 0.025;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_does_not_touch_a_single_light_byte() {
        let mut rows = [[3.0; 4]; 21];
        let before = rows;
        pack_fog(&mut rows, ModernLighting::default(), 100.0);
        assert_eq!(rows, before);
        assert!(!ModernLighting::default().post_process());
    }

    #[test]
    fn shadows_touch_only_row_12_w_and_do_not_enable_postprocess() {
        let mut rows = [[3.0; 4]; 21];
        let mut expected = rows;
        expected[12][3] = 1.0;
        let policy = ModernLighting {
            shadows: true,
            ..default()
        };
        pack_fog(&mut rows, policy, 100.0);
        assert_eq!(rows, expected);
        assert!(!policy.post_process());
        // The world pack starts from zero each frame, so disabling clears the lane.
        let mut fresh = [[0.0; 4]; 21];
        pack_fog(&mut fresh, ModernLighting::default(), 100.0);
        assert_eq!(fresh[12][3], 0.0);
    }

    #[test]
    fn fog_uses_only_spare_lanes_and_not_postprocess_switches() {
        let mut rows = [[0.0; 4]; 21];
        pack_fog(
            &mut rows,
            ModernLighting {
                fog: true,
                ..default()
            },
            100.0,
        );
        assert_eq!(rows[5], [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(rows[19], [0.0, 0.0, 92.0, 0.025]);
        rows[5][2] = 0.0;
        rows[19][2] = 0.0;
        rows[19][3] = 0.0;
        assert_eq!(rows, [[0.0; 4]; 21]);
    }
}
