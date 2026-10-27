//! While the window is covered, draw nothing to it, and pace the loop at the stock background rate.
//!
//! macOS throttles a covered window: the drawable request behind Bevy's `prepare_windows`
//! (`-[CAMetalLayer nextDrawable]`) waits up to about a second. So a Cmd-Tab or a screenshot
//! overlay held whole frames there, and every frame-time instrument read it as a hitch.
//!
//! Bevy presents a window only when an active camera draws to it (`present_frames`), and keeps an
//! unpresented drawable rather than ask for another. So with the window's cameras switched off
//! nothing waits on the window server. Offscreen cameras keep rendering.
//!
//! On uncover only the cameras this module switched off, and nobody wrote since, come back on: a
//! camera its owner switched while covered stays as the owner left it. A write shows in the
//! camera's change tick, sampled either side of Bevy's `CameraUpdateSystems`, which rewrites a
//! camera's computed values on a resize. A write inside that slot counts as Bevy's, so an
//! unordered owner landing there reads as no write and its camera comes back on: never stuck off.
//!
//! The game simulation, the network and the audio threads keep running; the loop is paced at 30
//! fps instead, 3.3.5's `maxFPSBk` default, since nothing else would bound it with no present.

use std::time::{Duration, Instant};

use bevy::camera::{CameraUpdateSystems, RenderTarget};
use bevy::ecs::change_detection::Tick;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowOccluded, WindowRef};

/// The covered window's frame rate: the stock client's background cap (`maxFPSBk` 30).
const BACKGROUND_FPS: f64 = 30.0;

/// A camera switched off for the cover: the change tick last accounted for (this module's write
/// or Bevy's), and whether an owner has written it since.
struct Parked {
    entity: Entity,
    seen: Tick,
    owned: bool,
}

/// The covered-window state: whether it is covered, the cameras switched off for it, and when
/// the last paced frame ended.
#[derive(Resource, Default)]
pub struct OccludedFrames {
    occluded: bool,
    parked: Vec<Parked>,
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
            .add_systems(
                PostUpdate,
                (
                    note_owner_writes.before(CameraUpdateSystems),
                    absorb_bevy_writes.after(CameraUpdateSystems),
                ),
            )
            .add_systems(Last, park_while_occluded);
    }
}

/// Mark every parked camera written since its last accounted tick as its owner's.
fn note_owner_writes(mut state: ResMut<OccludedFrames>, cameras: Query<Ref<Camera>>) {
    if state.parked.is_empty() {
        return;
    }
    for p in &mut state.parked {
        if cameras
            .get(p.entity)
            .is_ok_and(|c| c.last_changed() != p.seen)
        {
            p.owned = true;
        }
    }
}

/// Account for `CameraUpdateSystems`' writes: they are Bevy's, not an owner's.
fn absorb_bevy_writes(mut state: ResMut<OccludedFrames>, cameras: Query<Ref<Camera>>) {
    if state.parked.is_empty() {
        return;
    }
    for p in &mut state.parked {
        if let Ok(c) = cameras.get(p.entity) {
            p.seen = c.last_changed();
        }
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
    // Writes since `absorb_bevy_writes`, in the rest of `PostUpdate` and in `Last`.
    for p in &mut state.parked {
        if cameras
            .get_mut(p.entity)
            .is_ok_and(|(_, c, _)| c.last_changed() != p.seen)
        {
            p.owned = true;
        }
    }
    if state.occluded && state.parked.is_empty() {
        for (entity, mut camera, target) in &mut cameras {
            if camera.is_active && draws_to(target, primary) {
                camera.is_active = false;
                state.parked.push(Parked {
                    entity,
                    seen: camera.last_changed(),
                    owned: false,
                });
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
        let mut back_on = 0;
        for p in parked.iter().filter(|p| !p.owned) {
            if let Ok((_, mut camera, _)) = cameras.get_mut(p.entity) {
                camera.is_active = true;
                back_on += 1;
            }
        }
        let owned = parked.iter().filter(|p| p.owned).count();
        info!(
            "window uncovered: {back_on} camera(s) back on, {owned} left as their owner set them"
        );
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
    use bevy::ecs::schedule::ScheduleLabel;

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

    /// One write to the camera, on request: `Some(active)` sets `is_active`, `None` only marks it
    /// changed, as `camera_system`'s computed-value rewrite on a resize does.
    #[derive(Resource, Default)]
    struct WriteOnce(Option<Option<bool>>);

    fn write_once(mut ask: ResMut<WriteOnce>, mut cams: Query<&mut Camera>) {
        if let Some(write) = ask.0.take() {
            for mut cam in &mut cams {
                match write {
                    Some(active) => cam.is_active = active,
                    None => cam.set_changed(),
                }
            }
        }
    }

    /// A covered app with one window camera and [`write_once`] in `schedule`, asked for `write`
    /// mid-cover; whether the camera is on after the uncover.
    fn on_after_uncover(
        schedule: impl ScheduleLabel,
        in_bevys_slot: bool,
        write: Option<bool>,
    ) -> bool {
        let (mut app, window) = app();
        app.init_resource::<WriteOnce>();
        if in_bevys_slot {
            app.add_systems(schedule, write_once.in_set(CameraUpdateSystems));
        } else {
            app.add_systems(schedule, write_once);
        }
        let cam = app
            .world_mut()
            .spawn((Camera::default(), RenderTarget::Window(WindowRef::Primary)))
            .id();
        cover(&mut app, window, true);
        assert!(!active(&app, cam));
        app.world_mut().resource_mut::<WriteOnce>().0 = Some(write);
        app.update();
        cover(&mut app, window, false);
        active(&app, cam)
    }

    #[test]
    fn an_owners_write_while_covered_stands() {
        // A transition-only owner: it switches its camera off once, while covered.
        assert!(!on_after_uncover(Update, false, Some(false)));
        assert!(
            !on_after_uncover(Last, false, Some(false)),
            "a write in Last"
        );
    }

    #[test]
    fn bevys_camera_update_is_not_an_owners_write() {
        assert!(on_after_uncover(PostUpdate, true, None));
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
