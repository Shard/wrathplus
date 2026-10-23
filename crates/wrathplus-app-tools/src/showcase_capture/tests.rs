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

#[test]
fn curated_showcase_paths_pass_the_production_schema() {
    let paths = [
        include_str!("../../../../docs/showcase/northshire-abbey.toml"),
        include_str!("../../../../docs/showcase/goldshire-dusk.toml"),
        include_str!("../../../../docs/showcase/stormwind-gate.toml"),
        include_str!("../../../../docs/showcase/elwynn-lake-sunrise.toml"),
        include_str!("../../../../docs/showcase/westfall-farm.toml"),
    ];
    for text in paths {
        let path = ShowcasePath::parse(text).expect("curated route schema");
        assert_eq!(path.map, 0);
        assert_eq!(path.frame_count(), 12 * FPS);
        for frame in 0..path.frame_count() {
            let pose = path.sample_frame(frame).unwrap();
            assert!(pose.eye[2] > pose.target[2]);
        }
    }
}

#[test]
fn optional_visual_actor_defaults_do_not_change_camera_samples() {
    let empty = ShowcasePath::parse(PATH).unwrap();
    assert!(empty.actors.is_empty());
    let populated = ShowcasePath::parse(&format!(
        "{PATH}\n[[actors]]\ndisplay_id = 42\nposition = [1.0, 2.0, 3.0]\n"
    ))
    .unwrap();
    assert_eq!(populated.actors[0].yaw, 0.0);
    assert_eq!(populated.actors[0].scale, 1.0);
    for frame in 0..empty.frame_count() {
        assert_eq!(
            empty.sample_frame(frame).unwrap(),
            populated.sample_frame(frame).unwrap()
        );
    }
}

#[test]
fn visual_actor_bounds_and_unknown_fields_are_rejected() {
    let actor = "\n[[actors]]\ndisplay_id = 42\nposition = [1.0, 2.0, 3.0]\n";
    for invalid in [
        actor.replace("42", "0"),
        actor.replace("1.0", "nan"),
        actor.replace("1.0", "100001.0"),
        format!("{actor}yaw = inf\n"),
        format!("{actor}yaw = 7.0\n"),
        format!("{actor}scale = 0.0\n"),
        format!("{actor}scale = 4.1\n"),
        format!("{actor}scale = nan\n"),
        format!("{actor}guid = 42\n"),
        format!("{actor}creature_entry = 42\n"),
        actor.repeat(17),
    ] {
        assert!(
            ShowcasePath::parse(&format!("{PATH}{invalid}")).is_err(),
            "{invalid}"
        );
    }
    assert!(ShowcasePath::parse(&format!("{PATH}{}", actor.repeat(16))).is_ok());
}

#[test]
fn two_curated_routes_have_bounded_plain_wildlife_and_other_routes_stay_scenery_only() {
    let northshire = ShowcasePath::parse(include_str!(
        "../../../../docs/showcase/northshire-abbey.toml"
    ))
    .unwrap();
    let elwynn = ShowcasePath::parse(include_str!(
        "../../../../docs/showcase/elwynn-lake-sunrise.toml"
    ))
    .unwrap();
    assert_eq!(
        northshire
            .actors
            .iter()
            .map(|a| a.display_id)
            .collect::<Vec<_>>(),
        [347, 328]
    );
    assert_eq!(
        elwynn
            .actors
            .iter()
            .map(|a| a.display_id)
            .collect::<Vec<_>>(),
        [161, 193, 193]
    );
    for path in [&northshire, &elwynn] {
        for actor in &path.actors {
            assert_eq!(actor.scale, 1.0);
            assert!(
                actor.position[2] > 0.0 && actor.position[2] < path.sample_frame(0).unwrap().eye[2]
            );
        }
    }
    for text in [
        include_str!("../../../../docs/showcase/goldshire-dusk.toml"),
        include_str!("../../../../docs/showcase/stormwind-gate.toml"),
        include_str!("../../../../docs/showcase/westfall-farm.toml"),
    ] {
        assert!(ShowcasePath::parse(text).unwrap().actors.is_empty());
    }
}
