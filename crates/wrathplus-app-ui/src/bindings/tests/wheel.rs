//! The wheel's binding pass: per-notch dispatch and the UI's claim on a notch.

use super::*;

/// A trackpad's fractional lines carry to whole notches, each one `CameraZoomIn(1.0)`, and a
/// reversal drops the carried fraction.
#[test]
fn fractional_wheel_lines_dispatch_per_whole_notch() {
    let mut app = harness();
    for want in [0.0, 0.0, 1.0] {
        wheel(&mut app, 0.4);
        app.update();
        assert_eq!(
            state(&app).amount(Input::CameraZoomIn),
            want,
            "0.4 lines a frame: the third completes the notch"
        );
    }
    wheel(&mut app, -1.0);
    app.update();
    assert_eq!(state(&app).amount(Input::CameraZoomOut), 1.0);
    assert_eq!(state(&app).amount(Input::CameraZoomIn), 0.0);
}

/// A notch a UI `OnMouseWheel` handler took is the frame's, with no mouse-enabled frame under
/// the cursor: a chat frame takes the wheel without the mouse (`FloatingChatFrame.xml:505`).
#[test]
fn a_notch_a_ui_handler_took_does_not_zoom() {
    let mut app = harness();
    app.insert_resource(UiWheelEaten(true));
    wheel(&mut app, 1.0);
    app.update();
    assert!(
        !state(&app).fired(Input::CameraZoomIn),
        "the handler took the notch"
    );
    app.insert_resource(UiWheelEaten(false));
    wheel(&mut app, 1.0);
    app.update();
    assert_eq!(
        state(&app).amount(Input::CameraZoomIn),
        1.0,
        "a free notch zooms"
    );
}
