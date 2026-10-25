//! A real click on the stock login screen, through the Bevy input path: the primary window's
//! cursor and `ButtonInput<MouseButton>` into [`super::super::input::feed_ui_input`], the
//! production feed, never `UiScript::mouse_button` directly.

use super::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::math::DVec2;
use bevy::window::PrimaryWindow;
use wrathplus_ui::script::glue::GlueRequest;

/// The world `feed_ui_input` reads, with the stock login screen up on a 1024x768 window.
fn login_world() -> World {
    let mut script = UiScript::new().unwrap();
    script.install_glue_api().unwrap();
    script.set_screen_size(1024.0, 768.0);
    let failures = load_glue_ui(&script);
    assert!(failures.is_empty(), "{failures:#?}");
    script.fire_event("FRAMES_LOADED", vec![]);
    script.fire_event("SET_GLUE_SCREEN", vec![ScriptValue::Str("login".into())]);
    script.tick(0.1);
    script.resolve();
    let _ = script.take_glue_requests();

    let mut world = World::new();
    world.insert_non_send_resource(script);
    world.insert_non_send_resource(crate::textinput::HostClipboard::default());
    world.init_resource::<ButtonInput<MouseButton>>();
    world.init_resource::<ButtonInput<KeyCode>>();
    world.init_resource::<Messages<KeyboardInput>>();
    world.init_resource::<AccumulatedMouseScroll>();
    world.init_resource::<crate::bindings::WheelNotches>();
    world.init_resource::<crate::ui_script::UiWheelEaten>();
    world.init_resource::<crate::bindings::LayoutNames>();
    world.insert_resource(crate::app_core::ui_flags::UiScaleCvar(1.0));
    world.init_resource::<crate::app_core::ui_flags::PlayerUiHover>();
    world.init_resource::<crate::app_core::ui_flags::PlayerUiClickConsumed>();
    world.init_resource::<crate::app_core::ui_flags::UiKeyboardCapture>();
    world.init_resource::<crate::app_core::state::CursorPayloadHeld>();
    world.init_resource::<crate::target::Hovered>();
    world.init_resource::<crate::target::HoveredObject>();
    world.init_resource::<crate::target::PickOcclusion>();
    world.init_resource::<super::super::SyntheticPointer>();
    world.insert_resource(super::super::CapturePointerPinned(false));
    world
}

/// `AccountLoginExitButton` (`AccountLogin.xml`, `OnClick` → `QuitGame()`) answers a left click
/// the window delivers at its centre: the press and release reach the glue VM through
/// `feed_ui_input`, and the stock handler queues the quit the host turns into `AppExit`.
#[test]
fn a_real_click_on_the_login_exit_button_reaches_quit_game() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut world = login_world();
    // The button's centre in UI units (y-up), then in window px (y-down) on a 1:1 seam.
    let (x, y): (f32, f32) = world
        .non_send_resource::<UiScript>()
        .eval(
            "local x, y = AccountLoginExitButton:GetCenter() \
             local s = AccountLoginExitButton:GetEffectiveScale() return x * s, y * s",
        )
        .unwrap();
    let mut window = Window {
        resolution: (1024, 768).into(),
        ..Default::default()
    };
    window.set_physical_cursor_position(Some(DVec2::new(f64::from(x), f64::from(768.0 - y))));
    world.spawn((window, PrimaryWindow));

    let frame = |world: &mut World| {
        world
            .run_system_once(super::super::input::feed_ui_input)
            .unwrap()
    };
    frame(&mut world); // hover
    world
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    frame(&mut world);
    {
        let mut buttons = world.resource_mut::<ButtonInput<MouseButton>>();
        buttons.clear();
        buttons.release(MouseButton::Left);
    }
    frame(&mut world);

    let mut script = world.non_send_resource_mut::<UiScript>();
    assert!(script.errors().is_empty(), "{:?}", script.errors());
    assert!(
        script.take_glue_requests().contains(&GlueRequest::Quit),
        "the window's click at ({x}, {y}) reached AccountLoginExitButton's QuitGame()"
    );
}

/// `ui_input_live`'s table: the world always; the glue screens exactly when the stock glue hosts
/// them (an install with `GlueXML.toc`), which is what the installed client runs.
#[test]
fn ui_input_live_is_the_world_and_the_stock_glue_screens() {
    use crate::app_core::state::ClientState;
    use bevy::ecs::system::RunSystemOnce;
    let live = |state: ClientState| {
        let mut world = World::new();
        world.insert_resource(State::new(state));
        world.run_system_once(super::ui_input_live).unwrap()
    };
    assert!(live(ClientState::InWorld));
    let stock = super::stock_glue_active();
    for state in [
        ClientState::Login,
        ClientState::CharSelect,
        ClientState::CharCreate,
    ] {
        assert_eq!(live(state), stock, "{state:?} follows the stock glue");
    }
    if wrathplus_formats::wow_data().is_some() && std::env::var("WOW_NATIVE_GLUE").is_err() {
        assert!(stock, "an install with GlueXML hosts the stock glue");
    }
}
