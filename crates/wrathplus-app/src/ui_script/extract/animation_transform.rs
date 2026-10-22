use crate::ui_pass::UiQuad;
use bevy::prelude::*;
pub(super) fn apply(out: &mut [UiQuad], transform: Option<[f32; 6]>, s: f32, h: f32) {
    if let Some([a, b, c, d, tx, ty]) = transform {
        for q in out {
            // Match the pass's existing clip-before-rotation contract; explicit corners
            // otherwise bypass its CPU clipping path entirely.
            if q.corners.is_none() {
                if let Some(clip) = q.clip {
                    if let Some((rect, uv)) = crate::ui_pass::clip_quad(q.rect, q.uv, clip) {
                        q.rect = rect;
                        q.uv = uv;
                    } else {
                        q.color[3] = 0.0;
                    }
                    q.clip = None;
                }
            }
            let rect = q.rect;
            let mut points = q.corners.unwrap_or([
                rect.min,
                Vec2::new(rect.max.x, rect.min.y),
                rect.max,
                Vec2::new(rect.min.x, rect.max.y),
            ]);
            if q.corners.is_none() && q.rotation != 0.0 {
                let centre = rect.center();
                let (sin, cos) = q.rotation.sin_cos();
                for p in &mut points {
                    let v = *p - centre;
                    *p = centre + Vec2::new(cos * v.x - sin * v.y, sin * v.x + cos * v.y);
                }
            }
            for p in &mut points {
                let (x, y) = (p.x / s, (h - p.y) / s);
                *p = Vec2::new((a * x + c * y + tx) * s, h - (b * x + d * y + ty) * s);
            }
            q.corners = Some(points);
            q.rotation = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn affine_corners_preserve_uv_winding_and_screen_flip() {
        let mut q = [UiQuad {
            rect: Rect::new(10.0, 40.0, 30.0, 60.0),
            ..Default::default()
        }];
        // x'=2x; y'=3y, in y-up UI coordinates, screen height100.
        apply(&mut q, Some([2.0, 0.0, 0.0, 3.0, 0.0, 0.0]), 1.0, 100.0);
        assert_eq!(
            q[0].corners.unwrap(),
            [
                Vec2::new(20.0, -80.0),
                Vec2::new(60.0, -80.0),
                Vec2::new(60.0, -20.0),
                Vec2::new(20.0, -20.0)
            ]
        );
        assert_eq!(q[0].rotation, 0.0);
    }
    #[test]
    fn explicit_wedge_and_existing_rotation_are_composed() {
        let mut q = [UiQuad {
            rect: Rect::new(0.0, 0.0, 2.0, 2.0),
            rotation: std::f32::consts::FRAC_PI_2,
            ..Default::default()
        }];
        apply(&mut q, Some([1.0, 0.0, 0.0, 1.0, 5.0, 0.0]), 1.0, 100.0);
        assert!((q[0].corners.unwrap()[0] - Vec2::new(7.0, 0.0)).length() < 0.001);
        let wedge = [Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::Y];
        q[0].corners = Some(wedge);
        apply(&mut q, Some([1.0, 0.0, 0.0, 1.0, 5.0, 0.0]), 1.0, 100.0);
        assert_eq!(q[0].corners.unwrap()[0], Vec2::new(5.0, 0.0));
    }
}
