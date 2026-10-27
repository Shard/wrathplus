//! Whether the window was covered or unfocused during each frame. macOS throttles a covered
//! window to about one frame a second, so without this flag a Cmd-Tab or a screenshot overlay
//! reads as a one-second hitch in every frame-time instrument (the hitch log, the FPS journal,
//! the stream trace).
//!
//! A frame counts as occluded (or unfocused) if the window was so at any point in it: at the
//! frame's start, or by an event that arrived during it. So the long frame that ends at an
//! un-occlusion is still tagged, since the window was covered when it began.

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowFocused, WindowOccluded};

/// The primary window's visibility, current and over the frame just sampled.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowState {
    /// Covered right now, as the last `WindowOccluded` said; a window starts uncovered.
    pub occluded: bool,
    /// Key right now, as the last `WindowFocused` said; a window starts focused.
    pub focused: bool,
    /// Covered at any point in this frame.
    pub frame_occluded: bool,
    /// Not key at any point in this frame.
    pub frame_unfocused: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            occluded: false,
            focused: true,
            frame_occluded: false,
            frame_unfocused: false,
        }
    }
}

impl WindowState {
    /// Fold one frame's events: `occlusions` and `focuses` in arrival order.
    pub fn fold(
        &mut self,
        occlusions: impl IntoIterator<Item = bool>,
        focuses: impl IntoIterator<Item = bool>,
    ) {
        let mut any_occluded = self.occluded;
        for occluded in occlusions {
            any_occluded |= occluded;
            self.occluded = occluded;
        }
        let mut any_unfocused = !self.focused;
        for focused in focuses {
            any_unfocused |= !focused;
            self.focused = focused;
        }
        self.frame_occluded = any_occluded;
        self.frame_unfocused = any_unfocused;
    }

    /// The frame's tag for a log line: empty for a visible, key window.
    pub fn tag(&self) -> &'static str {
        match (self.frame_occluded, self.frame_unfocused) {
            (true, _) => " (window occluded: macOS throttles it to ~1 fps)",
            (false, true) => " (window not focused)",
            (false, false) => "",
        }
    }
}

/// Read this frame's events for the primary window. Runs in `First`, before every meter.
pub(super) fn track_window_state(
    mut state: ResMut<WindowState>,
    mut occlusions: MessageReader<WindowOccluded>,
    mut focuses: MessageReader<WindowFocused>,
    primary: Query<Entity, With<PrimaryWindow>>,
) {
    let primary = primary.single().ok();
    let ours = |w: Entity| primary.is_none_or(|p| p == w);
    let occluded: Vec<bool> = occlusions
        .read()
        .filter(|e| ours(e.window))
        .map(|e| e.occluded)
        .collect();
    let focused: Vec<bool> = focuses
        .read()
        .filter(|e| ours(e.window))
        .map(|e| e.focused)
        .collect();
    state.fold(occluded, focused);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_tagged_if_the_window_was_covered_at_any_point_in_it() {
        let mut s = WindowState::default();
        s.fold([], []);
        assert!(
            !s.frame_occluded && !s.frame_unfocused,
            "a quiet visible frame"
        );
        // Covered mid-frame: this frame and every one after it, until uncovered.
        s.fold([true], []);
        assert!(s.frame_occluded && s.occluded);
        s.fold([], []);
        assert!(s.frame_occluded, "still covered");
        // The long frame that ends at the uncover is still tagged: it began covered.
        s.fold([false], []);
        assert!(s.frame_occluded && !s.occluded);
        s.fold([], []);
        assert!(!s.frame_occluded, "visible again");
        // A covered-and-uncovered blip inside one frame tags it too.
        s.fold([true, false], []);
        assert!(s.frame_occluded && !s.occluded);
    }

    #[test]
    fn focus_is_tracked_apart_and_tags_a_frame_that_lost_it() {
        let mut s = WindowState::default();
        s.fold([], [false]);
        assert!(s.frame_unfocused && !s.focused && !s.frame_occluded);
        assert_eq!(s.tag(), " (window not focused)");
        s.fold([], [true]);
        assert!(
            s.frame_unfocused,
            "the frame focus came back in began unfocused"
        );
        s.fold([], []);
        assert_eq!(s.tag(), "");
        s.fold([true], [false]);
        assert!(
            s.tag().contains("occluded"),
            "occlusion outranks focus in the tag"
        );
    }

    #[test]
    fn the_system_reads_only_the_primary_windows_events() {
        let mut app = App::new();
        app.add_message::<WindowOccluded>()
            .add_message::<WindowFocused>()
            .init_resource::<WindowState>()
            .add_systems(Update, track_window_state);
        let primary = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let other = app.world_mut().spawn(Window::default()).id();
        app.world_mut().write_message(WindowOccluded {
            window: other,
            occluded: true,
        });
        app.update();
        assert!(
            !app.world().resource::<WindowState>().frame_occluded,
            "another window's event"
        );
        app.world_mut().write_message(WindowOccluded {
            window: primary,
            occluded: true,
        });
        app.update();
        assert!(app.world().resource::<WindowState>().frame_occluded);
    }
}
