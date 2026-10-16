//! `hit`: frames losing strictly more than half their intended horizontal travel.
use avian3d::prelude::Collider;
use benilla_assets::trace;
use bevy::prelude::*;

/// Capsule centres before/after the whole step. `intended` is horizontal velocity × dt, zero
/// during hold/root. This is an opt-in replay, not an exact feature captured by the resolver.
pub(in crate::player) fn hit(
    world: &benilla_world::collision::WorldCollision<'_, '_>,
    capsule: &Collider,
    from: Vec3,
    to: Vec3,
    intended: Vec3,
    skin_width: f32,
) {
    // Test the tag before any geometry, allocations or formatting.
    let enabled = trace::enabled_for("hit");
    report_if_clipped(enabled, intended, to - from, |want, got| {
        let replay = world.trace_hit_witness(capsule, from, intended, skin_width);
        let detail = match replay {
            Some(w) => {
                let face = w.nearest_face.map_or_else(
                    || "nearest_face=none".to_string(),
                    |f| {
                        format!(
                            "nearest_face={} face_normal={:?} point_gap={:.6}",
                            f.index, f.normal, f.point_gap
                        )
                    },
                );
                format!("result=hit entity={:?} kind={} normal={:?} point1={:?} distance={:.6} collision_distance={:.6} {face}",
                    w.hit.entity, w.source.label(), w.hit.normal1, w.hit.point1,
                    w.hit.distance, w.hit.collision_distance)
            }
            None => "result=miss entity=none kind=unknown nearest_face=none".to_string(),
        };
        trace::line("hit", &format!(
            "replay=horizontal from={from:?} to={to:?} intended={intended:?} want={want:.6} got={got:.6} clipped={:.3} {detail}",
            1.0 - got / want,
        ));
    });
}

fn report_if_clipped(enabled: bool, intended: Vec3, achieved: Vec3, report: impl FnOnce(f32, f32)) {
    if !enabled {
        return;
    }
    let want = intended.x.hypot(intended.z);
    let got = achieved.x.hypot(achieved.z);
    // Distance retained (not directional projection): a successful sideways slide is travel.
    if want.is_finite() && got.is_finite() && want > 0.0 && got < want * 0.5 {
        report(want, got);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtered_tag_never_runs_geometry_or_formatting_callback() {
        report_if_clipped(false, Vec3::X, Vec3::ZERO, |_, _| panic!("tag off"));
    }

    #[test]
    fn strictly_more_than_half_clipped_and_positive_horizontal_intent_only() {
        for (want, got, expected) in [
            (Vec3::X, Vec3::X * 0.49, true),
            (Vec3::X, Vec3::X * 0.5, false),
            (Vec3::X, Vec3::Z * 0.6, false),
            (Vec3::ZERO, Vec3::ZERO, false),
            (Vec3::Y, Vec3::ZERO, false),
            (Vec3::X, Vec3::new(0.0, 99.0, 0.0), true),
            (Vec3::splat(f32::NAN), Vec3::ZERO, false),
        ] {
            let mut called = false;
            report_if_clipped(true, want, got, |_, _| called = true);
            assert_eq!(called, expected, "intent={want:?} achieved={got:?}");
        }
    }
}
