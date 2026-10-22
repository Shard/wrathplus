//! The world right press ending repair mode, from `player/camera.rs`. It runs the merchant's
//! reset (`ui_merchant`) after the camera's latch, so it lives in the root layer.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// A right mouse-down in the world ends repair mode (`0x492c68`), from the latch through the
/// press edge to the reset; the same press on a UI frame, or one that drops a held payload
/// (`0x492b50`), leaves it standing.
#[test]
fn a_right_press_in_the_world_ends_repair_mode_and_one_on_the_ui_does_not() {
    use crate::player::camera::{latch_world_mouse, send_world_right_press, CameraControl, FlyCam};
    use wrathplus_ui::script::{MerchantState, UiScript};
    use wrathplus_world::interact::WorldRightPress;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::math::DVec2;

    // One right press with repair mode armed; returns whether repair mode survived it.
    let press = |over_ui: bool, payload_held: bool| {
        let mut world = World::new();
        let mut buttons = ButtonInput::<MouseButton>::default();
        buttons.press(MouseButton::Right);
        world.insert_resource(buttons);
        world.insert_resource(crate::ui_script::PointerOverUi(over_ui));
        world.insert_resource(crate::ui_script::CursorPayloadHeld(payload_held));
        world.init_resource::<CameraControl>();
        world.init_resource::<Messages<WorldRightPress>>();
        world.spawn((
            Camera::default(),
            FlyCam {
                yaw: 0.0,
                pitch: 0.0,
                speed: 0.0,
            },
        ));
        let mut window = Window::default();
        window.set_physical_cursor_position(Some(DVec2::new(100.0, 100.0)));
        world.spawn((window, PrimaryWindow));
        let mut script = UiScript::new().unwrap();
        script.set_merchant(Some(MerchantState {
            can_repair: true,
            ..MerchantState::default()
        }));
        script.run("ShowRepairCursor()").unwrap();
        assert!(script.repair_mode(), "the repair vendor arms repair mode");
        world.insert_non_send_resource(script);

        world.run_system_once(latch_world_mouse).unwrap();
        world.run_system_once(send_world_right_press).unwrap();
        world
            .run_system_once(crate::ui_merchant::end_repair_mode_on_right_press)
            .unwrap();
        world.non_send_resource::<UiScript>().repair_mode()
    };

    assert!(
        !press(false, false),
        "a right press in the world ends repair mode"
    );
    assert!(
        press(true, false),
        "a right press on a UI frame never reaches the world's hook"
    );
    assert!(
        press(false, true),
        "a press while a payload is held skips the repair reset"
    );
}
