use super::*;

pub(super) const PATH: &str = r#"
map = 0
minute = 720.0
[[keyframes]]
at = 0.0
eye = [0.0, 0.0, 10.0]
target = [0.0, 10.0, 10.0]
[[keyframes]]
at = 1.0
eye = [100.0, 0.0, 10.0]
target = [100.0, 10.0, 10.0]
fov_degrees = 90.0
"#;

#[test]
fn schema_defaults_and_fixed_light_time() {
    let path = ShowcasePath::parse(PATH).unwrap();
    assert_eq!(path.map, 0);
    assert_eq!(path.minute, 720.0);
    assert_eq!((WIDTH, HEIGHT, FPS), (1920, 1080, 60));
    assert_eq!(path.sample_frame(0).unwrap().fov_degrees, 60.0);
}

#[test]
fn smooth_bounded_interpolation_and_exact_endpoints() {
    let path = ShowcasePath::parse(PATH).unwrap();
    assert_eq!(path.sample_time(0.0).unwrap().eye, [0.0, 0.0, 10.0]);
    assert_eq!(path.sample_time(1.0).unwrap().eye, [100.0, 0.0, 10.0]);
    let midpoint = path.sample_frame(30).unwrap();
    assert_eq!(midpoint.eye, [50.0, 0.0, 10.0]);
    assert_eq!(midpoint.target, [50.0, 10.0, 10.0]);
    assert_eq!(midpoint.fov_degrees, 75.0);
    for index in 0..path.frame_count() {
        let sample = path.sample_frame(index).unwrap();
        assert!((0.0..=100.0).contains(&sample.eye[0]));
        assert!((60.0..=90.0).contains(&sample.fov_degrees));
    }
    // Zero endpoint velocity (difference over epsilon tends to zero).
    assert!(path.sample_time(1e-6).unwrap().eye[0] / 1e-6 < 0.001);
    assert!((100.0 - path.sample_time(1.0 - 1e-6).unwrap().eye[0]) / 1e-6 < 0.001);
}

#[test]
fn frame_times_are_random_access_repeatable_and_have_no_duplicate_endpoint() {
    let path = ShowcasePath::parse(PATH).unwrap();
    assert_eq!(path.frame_count(), 60);
    for index in (0..60).rev() {
        let sample = path.sample_frame(index).unwrap();
        assert_eq!(sample.seconds, f64::from(index) / 60.0);
        assert_eq!(sample, path.sample_frame(index).unwrap());
    }
    assert!(path.sample_frame(60).is_err());
    assert!(path.sample_frame(u32::MAX).is_err());
}

#[test]
fn invalid_times_coordinates_fov_light_time_and_unknown_keys_are_refused() {
    for invalid in [
        PATH.replace("minute = 720.0", "minute = 1440.0"),
        PATH.replace("minute = 720.0", "minute = nan"),
        PATH.replace("at = 0.0", "at = 0.1"),
        PATH.replace("at = 1.0", "at = 0.0"),
        PATH.replace("at = 1.0", "at = 601.0"),
        PATH.replace("at = 1.0", "at = 1.001"),
        PATH.replace("at = 1.0", "at = inf"),
        PATH.replace("fov_degrees = 90.0", "fov_degrees = 180.0"),
        PATH.replace("eye = [0.0, 0.0, 10.0]", "eye = [nan, 0.0, 10.0]"),
        PATH.replace("eye = [0.0, 0.0, 10.0]", "eye = [100001.0, 0.0, 10.0]"),
        PATH.replace("target = [0.0, 10.0, 10.0]", "target = [0.0, 0.0, 10.0]"),
        format!("unknown = true\n{PATH}"),
    ] {
        assert!(ShowcasePath::parse(&invalid).is_err(), "accepted {invalid}");
    }
}

#[test]
fn interpolated_coincident_view_is_refused_before_capture() {
    let crossing = PATH.replace("target = [0.0, 10.0, 10.0]", "target = [0.0, -10.0, 10.0]");
    assert!(ShowcasePath::parse(&crossing)
        .unwrap_err()
        .contains("coincide"));
}

#[test]
fn multiple_waypoints_are_continuous_with_no_overshoot() {
    let text = format!("{PATH}\n[[keyframes]]\nat = 2.0\neye = [50.0, 0.0, 10.0]\ntarget = [50.0, 10.0, 10.0]\nfov_degrees = 30.0\n");
    let path = ShowcasePath::parse(&text).unwrap();
    assert_eq!(path.frame_count(), 120);
    assert_eq!(path.sample_frame(60).unwrap().eye[0], 100.0);
    let left = path.sample_time(1.0 - 1e-6).unwrap().eye[0];
    let right = path.sample_time(1.0 + 1e-6).unwrap().eye[0];
    assert!((left - right).abs() < 1e-8);
    assert_eq!(path.sample_frame(90).unwrap().eye[0], 75.0);
}

#[test]
fn f32_camera_conversion_cannot_collapse_a_validated_direction() {
    let collapsed = PATH
        .replace("eye = [0.0, 0.0, 10.0]", "eye = [99999.0, 0.0, 10.0]")
        .replace(
            "target = [0.0, 10.0, 10.0]",
            "target = [99999.001, 0.0, 10.0]",
        );
    assert!(ShowcasePath::parse(&collapsed).is_err());
}
