//! Explicit serverless recorder. Nothing is installed when the path knob is absent.
use super::{CameraSample, ShowcasePath, FPS, HEIGHT, WIDTH};
use benilla_world::dev_state::DebugState;
use benilla_world::schedule::WorldStage;
use benilla_world::terrain_stream::WorldLoadProgress;
use benilla_world::view::WorldCamera;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::tasks::IoTaskPool;
use bevy::time::{TimeSystems, TimeUpdateStrategy};
use bevy::window::PrimaryWindow;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SETTLE_FRAMES: u32 = 8;
const DEADLINE: Duration = Duration::from_secs(120);

#[derive(Resource)]
struct Recording {
    path: ShowcasePath,
    out: PathBuf,
    frame: u32,
    stable: u32,
    advanced: bool,
    inflight: bool,
    finished: bool,
    started: Instant,
    tx: crossbeam_channel::Sender<Result<u32, String>>,
    rx: crossbeam_channel::Receiver<Result<u32, String>>,
}

/// Called before plugin construction, and therefore before the net thread could start.
pub(crate) fn prepare(app: &mut App) -> Result<(), String> {
    let Some(file) = std::env::var_os("WOW_SHOWCASE_PATH") else {
        return Ok(());
    };
    if std::env::var_os("WOW_CAPTURE").is_some() {
        return Err("WOW_SHOWCASE_PATH and WOW_CAPTURE are mutually exclusive".into());
    }
    let path = ShowcasePath::parse(&std::fs::read_to_string(&file).map_err(|e| e.to_string())?)?;
    let out = PathBuf::from(
        std::env::var_os("WOW_SHOWCASE_OUT").unwrap_or_else(|| "target/showcase".into()),
    );
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    if std::fs::read_dir(&out)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err(
            "showcase output directory must be empty (never overwrite a previous clip)".into(),
        );
    }
    // The world startup's existing serverless map seam; never changed outside this explicit run.
    std::env::set_var("WOW_MAP", path.map.to_string());
    let (tx, rx) = crossbeam_channel::unbounded();
    let manifest = format!("status = \"recording\"\nmap = {}\nminute = {}\neffective_minute = {}\nfps = {FPS}\nwidth = {WIDTH}\nheight = {HEIGHT}\nframes = {}\nexperience = \"{:?}\"\n", path.map, path.minute, path.minute as u32, path.frame_count(), app.world().get_resource::<crate::experience::Experience>().map(|e| e.profile()).unwrap_or_default());
    std::fs::write(out.join("manifest.toml"), manifest).map_err(|e| e.to_string())?;
    std::fs::copy(file, out.join("path.toml")).map_err(|e| e.to_string())?;
    app.insert_resource(crate::run_mode::CaptureMode)
        .insert_resource(Recording {
            path,
            out,
            frame: 0,
            stable: 0,
            advanced: false,
            inflight: false,
            finished: false,
            started: Instant::now(),
            tx,
            rx,
        });
    Ok(())
}

pub(crate) struct ShowcasePlugin;
impl Plugin for ShowcasePlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<Recording>() {
            return;
        }
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
            .add_systems(First, clock.before(TimeSystems))
            // Input/cinematic writers finish first; pin before the next stream reads the camera.
            .add_systems(
                Update,
                pin.after(WorldStage::Input).before(WorldStage::Stream),
            )
            .add_systems(Last, shutter);
    }
}

fn clock(mut state: ResMut<Recording>, mut strategy: ResMut<TimeUpdateStrategy>) {
    let advance = state.advanced;
    state.advanced = false;
    *strategy = TimeUpdateStrategy::ManualDuration(if advance {
        Duration::from_secs_f64(1.0 / f64::from(FPS))
    } else {
        Duration::ZERO
    });
}

fn pose(sample: CameraSample) -> Transform {
    let eye = benilla_assets::coords::wow_to_bevy(sample.eye.map(|v| v as f32));
    let target = benilla_assets::coords::wow_to_bevy(sample.target.map(|v| v as f32));
    let direction = (target - eye).normalize();
    let up = if direction.dot(Vec3::Y).abs() > 0.999 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    Transform::from_translation(eye).looking_at(target, up)
}

fn pin(
    state: Res<Recording>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<WorldCamera>>,
    mut debug: ResMut<DebugState>,
    mut perf: ResMut<crate::perf::PerfHud>,
) {
    let Ok(sample) = state.path.sample_frame(state.frame) else {
        return;
    };
    debug.lighting.follow_server_time = false;
    debug.lighting.manual_minute = state.path.minute as u32;
    perf.visible = false;
    for (mut transform, mut projection) in &mut cameras {
        *transform = pose(sample);
        if let Projection::Perspective(p) = &mut *projection {
            p.fov = (sample.fov_degrees as f32).to_radians();
        }
    }
}

fn finish(state: &mut Recording, result: Result<(), String>, exit: &mut MessageWriter<AppExit>) {
    state.finished = true;
    let text = match &result {
        Ok(()) => "status = \"complete\"\n".into(),
        Err(e) => format!("status = \"failed\"\nerror = {e:?}\n"),
    };
    let saved = std::fs::write(state.out.join("result.toml"), text);
    if result.is_err() || saved.is_err() {
        error!("showcase failed: {result:?} {saved:?}");
        exit.write(AppExit::error());
    } else {
        info!("showcase complete: {} frames", state.path.frame_count());
        exit.write(AppExit::Success);
    }
}

fn shutter(
    mut state: ResMut<Recording>,
    progress: Option<Res<WorldLoadProgress>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if state.finished {
        return;
    }
    if state.started.elapsed() > DEADLINE {
        finish(
            &mut state,
            Err("stream/readback/write timeout".into()),
            &mut exit,
        );
        return;
    }
    if state.inflight {
        match state.rx.try_recv() {
            Ok(Ok(index)) if index == state.frame => {
                if state.frame + 1 == state.path.frame_count() {
                    finish(&mut state, Ok(()), &mut exit);
                    return;
                }
                state.frame += 1;
                state.inflight = false;
                state.stable = 0;
                state.advanced = true;
                state.started = Instant::now();
            }
            Ok(other) => finish(
                &mut state,
                Err(format!("frame write failed: {other:?}")),
                &mut exit,
            ),
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                finish(&mut state, Err("writer disconnected".into()), &mut exit)
            }
            Err(_) => (),
        }
        return;
    }
    let size_ok = windows.single().is_ok_and(|w| {
        w.resolution.physical_width() == WIDTH && w.resolution.physical_height() == HEIGHT
    });
    if !size_ok {
        state.stable = 0;
        return;
    }
    if !progress
        .as_deref()
        .is_some_and(|p| p.focus_resident && p.presentable())
    {
        state.stable = 0;
        return;
    }
    state.stable += 1;
    if state.stable < SETTLE_FRAMES {
        return;
    }
    state.inflight = true;
    let index = state.frame;
    let path = state.out.join(format!("frame{index:06}.png"));
    let tx = state.tx.clone();
    commands
        .spawn(Screenshot::primary_window())
        .observe(move |shot: On<ScreenshotCaptured>| {
            let tx = tx.clone();
            let path = path.clone();
            let image = shot
                .image
                .clone()
                .try_into_dynamic()
                .map(|i| i.to_rgb8())
                .map_err(|e| e.to_string());
            IoTaskPool::get()
                .spawn(async move {
                    let outcome = image.and_then(|image| {
                        if image.dimensions() != (WIDTH, HEIGHT) {
                            return Err("readback dimensions are not 1080p".into());
                        }
                        image
                            .save_with_format(&path, image::ImageFormat::Png)
                            .map_err(|e| e.to_string())?;
                        Ok(index)
                    });
                    let _ = tx.send(outcome);
                })
                .detach();
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_path_preparation_has_no_world_or_environment_side_effects() {
        use crate::local_state::test_env::{EnvGuard, ENV_LOCK};
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _path = EnvGuard::unset("WOW_SHOWCASE_PATH");
        let _map = EnvGuard::set("WOW_MAP", "1");
        let mut app = App::new();
        prepare(&mut app).unwrap();
        assert!(!app.world().contains_resource::<Recording>());
        assert!(!app
            .world()
            .contains_resource::<crate::run_mode::CaptureMode>());
        assert!(!app.world().contains_resource::<TimeUpdateStrategy>());
        assert_eq!(std::env::var("WOW_MAP").unwrap(), "1");
    }

    fn recording_app() -> App {
        let mut app = App::new();
        let path = ShowcasePath::parse(super::super::tests::PATH).unwrap();
        let (tx, rx) = crossbeam_channel::unbounded();
        app.insert_resource(Recording {
            path,
            out: std::env::temp_dir().join("azb-showcase-test-unused"),
            frame: 0,
            stable: 0,
            advanced: false,
            inflight: false,
            finished: false,
            started: Instant::now(),
            tx,
            rx,
        });
        app.insert_resource(WorldLoadProgress {
            focus_resident: true,
            scene_ready: true,
            ..Default::default()
        });
        app.add_message::<AppExit>();
        app.world_mut().spawn((
            Window {
                resolution: bevy::window::WindowResolution::new(WIDTH, HEIGHT)
                    .with_scale_factor_override(1.0),
                ..Default::default()
            },
            PrimaryWindow,
        ));
        app.add_systems(Last, shutter);
        app
    }

    #[test]
    fn writer_backpressure_keeps_exactly_one_frame_inflight() {
        let mut app = recording_app();
        for _ in 0..SETTLE_FRAMES {
            app.update();
        }
        assert!(app.world().resource::<Recording>().inflight);
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(app.world().resource::<Recording>().frame, 0);
        let count = app
            .world_mut()
            .query::<&Screenshot>()
            .iter(app.world())
            .count();
        assert_eq!(count, 1);
        app.world().resource::<Recording>().tx.send(Ok(0)).unwrap();
        app.update();
        let state = app.world().resource::<Recording>();
        assert_eq!(state.frame, 1);
        assert!(state.advanced);
        assert!(!state.inflight);
        assert_eq!(state.stable, 0);
    }

    #[test]
    fn unready_or_wrong_size_world_never_requests_a_frame() {
        let mut app = recording_app();
        app.world_mut()
            .resource_mut::<WorldLoadProgress>()
            .scene_ready = false;
        for _ in 0..12 {
            app.update();
        }
        assert!(!app.world().resource::<Recording>().inflight);
        app.world_mut()
            .resource_mut::<WorldLoadProgress>()
            .scene_ready = true;
        let mut windows = app.world_mut().query::<&mut Window>();
        windows
            .single_mut(app.world_mut())
            .unwrap()
            .resolution
            .set_physical_resolution(100, 100);
        for _ in 0..12 {
            app.update();
        }
        assert!(!app.world().resource::<Recording>().inflight);
    }

    #[test]
    fn clock_advances_once_after_ack_not_while_waiting() {
        let mut app = recording_app();
        app.add_plugins(bevy::time::TimePlugin);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        app.add_systems(First, clock.before(TimeSystems));
        for _ in 0..15 {
            app.update();
        }
        assert_eq!(app.world().resource::<Time>().elapsed(), Duration::ZERO);
        app.world().resource::<Recording>().tx.send(Ok(0)).unwrap();
        app.update(); // Ack is in Last; the next First consumes it.
        assert_eq!(app.world().resource::<Time>().elapsed(), Duration::ZERO);
        app.update();
        let elapsed = app.world().resource::<Time>().elapsed();
        assert_eq!(elapsed, Duration::from_secs_f64(1.0 / 60.0));
        for _ in 0..15 {
            app.update();
        }
        assert_eq!(app.world().resource::<Time>().elapsed(), elapsed);
    }

    #[test]
    fn wrong_ack_and_write_error_exit_without_advancing() {
        for outcome in [Ok(99), Err("disk full".into())] {
            let mut app = recording_app();
            app.world_mut().resource_mut::<Recording>().inflight = true;
            app.world()
                .resource::<Recording>()
                .tx
                .send(outcome)
                .unwrap();
            app.update();
            assert_eq!(app.world().resource::<Recording>().frame, 0);
            assert_eq!(app.world().resource::<Messages<AppExit>>().len(), 1);
        }
    }

    #[test]
    fn timeout_exits_without_requesting_a_frame() {
        let mut app = recording_app();
        app.world_mut().resource_mut::<Recording>().started =
            Instant::now() - DEADLINE - Duration::from_secs(1);
        app.update();
        assert!(!app.world().resource::<Recording>().inflight);
        assert_eq!(app.world().resource::<Messages<AppExit>>().len(), 1);
    }

    #[test]
    fn vertical_and_normal_poses_have_finite_rotation() {
        for target in [[0.0, 0.0, 10.0], [10.0, 0.0, 0.0]] {
            let transform = pose(CameraSample {
                seconds: 0.0,
                eye: [0.0; 3],
                target,
                fov_degrees: 60.0,
            });
            assert!(transform.rotation.is_finite());
            assert!((transform.rotation.length() - 1.0).abs() < 1e-5);
        }
    }
    #[test]
    fn absent_recording_plugin_does_not_install_clock_or_systems() {
        let mut app = App::new();
        app.add_plugins(ShowcasePlugin);
        assert!(!app.world().contains_resource::<TimeUpdateStrategy>());
        assert!(!app
            .world()
            .contains_resource::<crate::run_mode::CaptureMode>());
    }
}
