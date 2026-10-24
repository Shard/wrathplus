use super::*;

#[test]
fn showcase_disabled_preserves_window_policy_and_enabled_owns_resolution() {
    use crate::local_state::test_env::{EnvGuard, ENV_LOCK};
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _path = EnvGuard::unset("WOW_SHOWCASE_PATH");
    assert!(!crate::run_mode::showcase_active());
    assert_eq!(
        crate::run_mode::offline_active(),
        crate::run_mode::scenario_active()
    );
    assert_eq!(
        crate::run_mode::start_state(),
        if crate::run_mode::scenario_active() {
            crate::app_core::state::ClientState::InWorld
        } else {
            crate::app_core::state::ClientState::Login
        }
    );
    let baseline = (
        windowed_env(),
        boot_window_mode(),
        boot_windowed_size(),
        requested_dpi(),
    );
    {
        let _set = EnvGuard::set("WOW_SHOWCASE_PATH", "path.toml");
        if crate::run_mode::dev_affordances() {
            assert!(windowed_env());
            assert_eq!(boot_window_mode(), WindowMode::Windowed);
            let mut app = App::new();
            app.insert_resource(VideoConfig {
                display: DisplayMode::Fullscreen,
                maximize: true,
                windowed: UVec2::new(320, 200),
                ..Default::default()
            });
            app.init_resource::<GxRestarts>();
            app.add_systems(Update, apply_window_mode);
            app.world_mut().spawn((
                Window {
                    resolution: WindowResolution::new(1920, 1080).with_scale_factor_override(1.0),
                    ..Default::default()
                },
                PrimaryWindow,
            ));
            app.update();
            let mut windows = app.world_mut().query::<&Window>();
            let window = windows.single(app.world()).unwrap();
            assert_eq!(window.mode, WindowMode::Windowed);
            assert_eq!(
                (
                    window.resolution.physical_width(),
                    window.resolution.physical_height()
                ),
                (1920, 1080)
            );
        }
    }
    assert_eq!(
        baseline,
        (
            windowed_env(),
            boot_window_mode(),
            boot_windowed_size(),
            requested_dpi()
        )
    );
}

#[test]
fn display_modes_are_seeded_before_framexml_and_on_replacement_vms() {
    let mut world = World::new();
    world.spawn((
        Window {
            resolution: WindowResolution::new(1600, 1000),
            ..Default::default()
        },
        PrimaryWindow,
    ));
    for _ in 0..2 {
        let mut script = wrathplus_ui::script::UiScript::new().unwrap();
        seed_display_modes(&mut world, &mut script);
        script.run("local r = ({GetScreenResolutions()})[GetCurrentResolution()] assert(r == '1600x1000')").unwrap();
    }
}

/// Synced is `Fifo`, not `AutoVsync` (`FifoRelaxed`, which tears on a late frame).
#[test]
fn the_default_is_synced_like_the_window_literal() {
    assert!(VideoConfig::default().vsync);
    assert_eq!(present_mode(true), PresentMode::default());
    assert_eq!(present_mode(true), PresentMode::Fifo);
}

/// On Metal `Immediate` takes ~1 s `nextDrawable` stalls.
#[test]
fn uncapped_is_autonovsync_not_immediate() {
    assert_eq!(present_mode(false), PresentMode::AutoNoVsync);
}

/// `WindowMode::Fullscreen` is ignored on Wayland and panics without a monitor.
#[test]
fn the_default_is_borderless_fullscreen_not_exclusive() {
    assert_eq!(DisplayMode::default(), DisplayMode::Fullscreen);
    assert!(matches!(
        window_mode(DisplayMode::Fullscreen, false, MonitorSelection::Primary),
        WindowMode::BorderlessFullscreen(_)
    ));
    assert_eq!(
        window_mode(DisplayMode::Windowed, false, MonitorSelection::Primary),
        WindowMode::Windowed
    );
}

/// The reference's polarity: the row is "Windowed Mode".
#[test]
fn gxwindow_one_is_windowed() {
    assert_eq!(display_from_flag(0.0), DisplayMode::Fullscreen);
    assert_eq!(display_from_flag(1.0), DisplayMode::Windowed);
}

/// A window born fullscreen is not re-asserted on the first frame, and leaving fullscreen
/// hands `gxResolution` back rather than the monitor's size.
#[test]
fn the_fullscreen_round_trip_keeps_the_monitor_out_of_the_windowed_size() {
    use crate::local_state::test_env::{EnvGuard, ENV_LOCK};
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _path = EnvGuard::unset("WOW_SHOWCASE_PATH");
    let mut app = App::new();
    app.insert_resource(VideoConfig {
        vsync: true,
        display: DisplayMode::Fullscreen,
        maximize: false,
        windowed: UVec2::new(1024, 768),
    })
    // Seated by hand: this test runs the one system, not the plugin.
    .init_resource::<GxRestarts>()
    .add_systems(Update, apply_window_mode);
    let win = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    // Born as `lib.rs` builds it: fullscreen, on the boot monitor selection.
    app.world_mut()
        .entity_mut(win)
        .get_mut::<Window>()
        .unwrap()
        .mode = window_mode(DisplayMode::Fullscreen, false, MonitorSelection::Primary);

    app.update();
    assert!(
        matches!(
            app.world().entity(win).get::<Window>().unwrap().mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
        ),
        "an already-fullscreen window must not be re-asserted onto another monitor selection"
    );

    // Stand in for the compositor: fullscreen makes the resolution the monitor's. Then leave.
    app.world_mut()
        .entity_mut(win)
        .get_mut::<Window>()
        .unwrap()
        .resolution
        .set(3440.0, 1440.0);
    app.world_mut().resource_mut::<VideoConfig>().display = DisplayMode::Windowed;
    app.update();
    let w = app.world().entity(win).get::<Window>().unwrap();
    assert_eq!(w.mode, WindowMode::Windowed);
    assert_eq!(
        (w.resolution.width(), w.resolution.height()),
        (1024.0, 768.0),
        "leaving fullscreen restores gxResolution, never the monitor's size"
    );

    // Back in, on `Current`: the monitor the window is on by now.
    app.world_mut().resource_mut::<VideoConfig>().display = DisplayMode::Fullscreen;
    app.update();
    assert!(matches!(
        app.world().entity(win).get::<Window>().unwrap().mode,
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    ));
}

/// `gxMaximize` counts only while windowed, and there it is the reference's popup over the
/// whole screen (`0x58cf10`: style `0x90000000`, `GetSystemMetrics` 0 and 1).
#[test]
fn a_maximized_window_is_borderless_over_the_monitor() {
    let m = MonitorSelection::Current;
    assert_eq!(
        window_mode(DisplayMode::Windowed, true, m),
        WindowMode::BorderlessFullscreen(m)
    );
    assert_eq!(
        window_mode(DisplayMode::Windowed, false, m),
        WindowMode::Windowed
    );
    assert_eq!(
        window_mode(DisplayMode::Fullscreen, true, m),
        WindowMode::BorderlessFullscreen(m),
        "fullscreen ignores it"
    );
}

/// The `RestartGx` rebuild applies a committed `gxMaximize`, and un-maximizing hands the
/// windowed size back.
#[test]
fn the_restart_rebuild_applies_a_committed_maximize() {
    use crate::local_state::test_env::{EnvGuard, ENV_LOCK};
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _path = EnvGuard::unset("WOW_SHOWCASE_PATH");
    let mut app = App::new();
    app.insert_resource(VideoConfig {
        vsync: true,
        display: DisplayMode::Windowed,
        maximize: false,
        windowed: UVec2::new(1024, 768),
    })
    .init_resource::<GxRestarts>()
    .add_systems(Update, apply_window_mode);
    let win = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    app.update();
    assert_eq!(
        app.world().entity(win).get::<Window>().unwrap().mode,
        WindowMode::Windowed
    );
    // The commit's observer write, then the restart the stock Okay asks for.
    app.world_mut().resource_mut::<VideoConfig>().maximize = true;
    app.world_mut().resource_mut::<GxRestarts>().0 += 1;
    app.update();
    assert!(matches!(
        app.world().entity(win).get::<Window>().unwrap().mode,
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    ));
    app.world_mut()
        .entity_mut(win)
        .get_mut::<Window>()
        .unwrap()
        .resolution
        .set(2560.0, 1440.0);
    app.world_mut().resource_mut::<VideoConfig>().maximize = false;
    app.world_mut().resource_mut::<GxRestarts>().0 += 1;
    app.update();
    let w = app.world().entity(win).get::<Window>().unwrap();
    assert_eq!(w.mode, WindowMode::Windowed);
    assert_eq!(
        (w.resolution.width(), w.resolution.height()),
        (1024.0, 768.0)
    );
}

#[test]
fn gxresolution_parses_the_reference_spelling() {
    assert_eq!(parse_resolution("1280x800"), Some(UVec2::new(1280, 800)));
    assert_eq!(parse_resolution("1600X900"), Some(UVec2::new(1600, 900)));
    assert_eq!(parse_resolution("1024 x 768"), Some(UVec2::new(1024, 768)));
    assert_eq!(parse_resolution("0x600"), None);
    assert_eq!(parse_resolution("1280x"), None);
    assert_eq!(parse_resolution("fullscreen"), None);
}
