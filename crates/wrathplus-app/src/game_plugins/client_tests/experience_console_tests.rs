//! The `/console experience` switch end to end, from `experience.rs`. It drives the console
//! registry, a ui module, so it lives in the root layer.

use bevy::prelude::*;

/// The console switch end to end: `/console experience …` through the registry to the
/// renderer's policy resource, and back to Classic's all-off policy.
#[test]
fn console_switch_drives_the_renderer_policy() {
    use crate::app_core::console::ConsoleCommandApp;
    use crate::console::{execute, ConsolePlugin};
    use crate::experience::{console_command, sync_lighting, Experience};
    use wrathplus_world::modern_lighting::ModernLighting;
    let mut app = App::new();
    app.add_plugins(ConsolePlugin)
        .console_command("experience", "test", console_command)
        .insert_resource(Experience::default())
        .init_resource::<ModernLighting>()
        .add_systems(Update, sync_lighting);
    let mut run = |line: &str| {
        let out = execute(app.world_mut(), line);
        app.update();
        (out, *app.world().resource::<ModernLighting>())
    };
    let (out, policy) = run("experience");
    assert_eq!(out[0], "experience: classic (session only)");
    assert_eq!(policy, ModernLighting::default());
    let (out, policy) = run("experience PLUS");
    assert!(out.contains(&"  fog: on".to_string()), "{out:?}");
    assert!(out.contains(&"  ambient_occlusion: off (fixed at startup)".to_string()));
    assert!(policy.fog && policy.bloom && policy.shadows && policy.post_process());
    let (_, policy) = run("experience bloom off");
    assert!(!policy.bloom && policy.tone_mapping);
    let (out, policy) = run("experience classic");
    assert!(out.contains(&"  bloom: off".to_string()));
    assert_eq!(policy, ModernLighting::default());
    // A Classic feature write is remembered for Plus but stays masked.
    let (out, policy) = run("experience fog on");
    assert!(out.contains(&"  fog: off".to_string()));
    assert_eq!(policy, ModernLighting::default());
    for bad in [
        "experience blooom on",
        "experience fog maybe",
        "experience a b c",
    ] {
        let (out, _) = run(bad);
        assert_eq!(out.len(), 1, "{bad}: {out:?}");
    }
}
