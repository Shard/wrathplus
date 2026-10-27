//! While the window is covered, draw nothing to it, and pace the loop at the stock background rate.
//!
//! macOS throttles a covered window: the drawable request behind Bevy's `prepare_windows`
//! (`-[CAMetalLayer nextDrawable]`) waits up to about a second. So a Cmd-Tab or a screenshot
//! overlay held whole frames there, and every frame-time instrument read it as a hitch.
//!
//! Bevy presents a window only when an active camera draws to it (`present_frames`), and keeps an
//! unpresented drawable rather than ask for another. So with the window's cameras switched off
//! nothing waits on the window server. Only the cameras this module switched off are switched
//! back on, and offscreen cameras keep rendering.
//!
//! The game simulation, the network and the audio threads keep running; the loop is paced at 30
//! fps instead, 3.3.5's `maxFPSBk` default, since nothing else would bound it with no present.

use std::time::{Duration, Instant};

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowOccluded, WindowRef};

/// The covered window's frame rate: the stock client's background cap (`maxFPSBk` 30).
const BACKGROUND_FPS: f64 = 30.0;

/// The covered-window state: whether it is covered, the cameras switched off for it, and when
/// the last paced frame ended.
#[derive(Resource, Default)]
pub struct OccludedFrames {
    occluded: bool,
    parked: Vec<Entity>,
    last_frame: Option<Instant>,
}

impl OccludedFrames {
    /// Whether the window's cameras are switched off now.
    pub fn parked(&self) -> bool {
        !self.parked.is_empty()
    }
}

pub struct OccludedFramesPlugin;

impl Plugin for OccludedFramesPlugin {
    fn build(&self, app: &mut App) {
        // Registered here too, so a headless app without the window plugin still runs.
        app.add_message::<WindowOccluded>()
            .init_resource::<OccludedFrames>()
            .add_systems(Last, park_while_occluded);
    }
}

fn draws_to(target: &RenderTarget, primary: Option<Entity>) -> bool {
    match target {
        RenderTarget::Window(WindowRef::Primary) => true,
        RenderTarget::Window(WindowRef::Entity(e)) => Some(*e) == primary,
        _ => false,
    }
}

fn park_while_occluded(
    mut state: ResMut<OccludedFrames>,
    mut occlusions: MessageReader<WindowOccluded>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut cameras: Query<(Entity, &mut Camera, &RenderTarget)>,
) {
    let primary = primary.single().ok();
    for e in occlusions.read() {
        if primary.is_none_or(|p| p == e.window) {
            state.occluded = e.occluded;
        }
    }
    if state.occluded && state.parked.is_empty() {
        for (entity, mut camera, target) in &mut cameras {
            if camera.is_active && draws_to(target, primary) {
                camera.is_active = false;
                state.parked.push(entity);
            }
        }
        if !state.parked.is_empty() {
            info!(
                "window covered: {} camera(s) off, the loop paced at {BACKGROUND_FPS} fps",
                state.parked.len()
            );
        }
    } else if !state.occluded && !state.parked.is_empty() {
        let parked = std::mem::take(&mut state.parked);
        for entity in &parked {
            if let Ok((_, mut camera, _)) = cameras.get_mut(*entity) {
                camera.is_active = true;
            }
        }
        info!("window uncovered: {} camera(s) back on", parked.len());
    }
    if state.parked() {
        let period = Duration::from_secs_f64(1.0 / BACKGROUND_FPS);
        if let Some(since) = state.last_frame.map(|t| t.elapsed()) {
            if since < period {
                std::thread::sleep(period - since);
            }
        }
        state.last_frame = Some(Instant::now());
    } else {
        state.last_frame = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::camera::ImageRenderTarget;

    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(OccludedFramesPlugin);
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        (app, window)
    }

    fn cover(app: &mut App, window: Entity, occluded: bool) {
        app.world_mut()
            .write_message(WindowOccluded { window, occluded });
        app.update();
    }

    fn active(app: &App, e: Entity) -> bool {
        app.world().get::<Camera>(e).unwrap().is_active
    }

    #[test]
    fn a_covered_window_switches_off_its_cameras_and_back_on_only_those() {
        let (mut app, window) = app();
        let world_cam = app
            .world_mut()
            .spawn((Camera::default(), RenderTarget::Window(WindowRef::Primary)))
            .id();
        let already_off = app
            .world_mut()
            .spawn((
                Camera {
                    is_active: false,
                    ..default()
                },
                RenderTarget::Window(WindowRef::Entity(window)),
            ))
            .id();
        let offscreen = app
            .world_mut()
            .spawn((
                Camera::default(),
                RenderTarget::Image(ImageRenderTarget::from(Handle::<Image>::default())),
            ))
            .id();
        app.update();
        assert!(active(&app, world_cam) && active(&app, offscreen));

        cover(&mut app, window, true);
        assert!(app.world().resource::<OccludedFrames>().parked());
        assert!(!active(&app, world_cam), "the window's camera is off");
        assert!(
            active(&app, offscreen),
            "an offscreen camera keeps rendering"
        );

        cover(&mut app, window, false);
        assert!(!app.world().resource::<OccludedFrames>().parked());
        assert!(active(&app, world_cam), "switched back on");
        assert!(
            !active(&app, already_off),
            "a camera it never switched off stays off"
        );
        assert!(active(&app, offscreen));
    }

    #[test]
    fn another_windows_cover_changes_nothing_and_a_covered_loop_is_paced() {
        let (mut app, _) = app();
        let cam = app
            .world_mut()
            .spawn((Camera::default(), RenderTarget::Window(WindowRef::Primary)))
            .id();
        let other = app.world_mut().spawn(Window::default()).id();
        cover(&mut app, other, true);
        assert!(active(&app, cam), "only the primary window's cover counts");

        let primary = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(app.world())
            .unwrap();
        cover(&mut app, primary, true);
        let t = Instant::now();
        for _ in 0..3 {
            app.update();
        }
        // Three paced frames at 30 fps: at least two whole periods.
        assert!(
            t.elapsed() >= Duration::from_millis(60),
            "{:?}",
            t.elapsed()
        );
    }
}
