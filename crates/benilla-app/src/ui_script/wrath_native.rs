//! Native diagnostics and input arbitration around the bounded stock Wrath interface.
//!
//! The pre-world login, realm and character screens are already native Bevy UI. Once a character
//! enters the world the stock GameMenu is loaded by `wrath_stock`; this small diagnostic remains
//! host-authored and does not imitate stock controls or gameplay services.

use bevy::prelude::*;

use crate::char_select::ClientState;
use crate::net::{NetStatus, SelfPlayer};
use crate::ui_script::{
    CapturePointerPinned, CursorPayloadHeld, EguiPointerOver, InspectMode, PlayerUiClickConsumed,
    PlayerUiHover, PointerOverUi, PointerOverUiPanel, SyntheticPointer, UiClock, UiCostWanted,
    UiFeed, UiFrameCost, UiInput, UiKeyboardCapture, UiPaint, UiPassState,
};

#[derive(Component)]
struct WrathOverlay;

#[derive(Component)]
struct SessionStatus;

/// Native chrome participates in the shared pointer arbiter alongside addon widgets.
#[derive(Resource, Default)]
pub(super) struct NativeChromeHover(pub(super) bool);

/// A mouse press begun on native chrome owns the gesture through its release frame.
#[derive(Resource, Default)]
pub(super) struct NativePointerOwnership(pub(super) bool);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TruthfulSessionStatus {
    Connected,
    AwaitingSelf,
    Disconnected,
}

fn truthful_status(connected: bool, self_streamed: bool) -> TruthfulSessionStatus {
    if !connected {
        TruthfulSessionStatus::Disconnected
    } else if !self_streamed {
        TruthfulSessionStatus::AwaitingSelf
    } else {
        TruthfulSessionStatus::Connected
    }
}

/// Install the shared resources, diagnostic overlay and the native Escape bridge into the stock
/// menu. Pointer capture remains shared with authored addon widgets and the stock frame tree.
pub(super) fn build(app: &mut App) {
    app.insert_resource(super::UiScaleCvar(super::default_ui_scale()))
        .init_resource::<UiFrameCost>()
        .init_resource::<crate::bindings::WheelNotches>()
        .init_resource::<UiCostWanted>()
        .init_resource::<PointerOverUi>()
        .init_resource::<SyntheticPointer>()
        .insert_resource(CapturePointerPinned(
            std::env::var_os("WOW_CAPTURE").is_some()
                || std::env::var_os("WOW_CAPTURE_UI").is_some(),
        ))
        .init_resource::<EguiPointerOver>()
        .init_resource::<InspectMode>()
        .init_resource::<PlayerUiHover>()
        .init_resource::<UiKeyboardCapture>()
        .init_resource::<PlayerUiClickConsumed>()
        .init_resource::<CursorPayloadHeld>()
        .init_resource::<UiClock>()
        .init_resource::<UiPassState>()
        .init_resource::<PointerOverUiPanel>()
        // Shared logout systems validate this seam even without a Lua VM to drain.
        .init_resource::<super::ReloadUiPending>()
        .init_resource::<NativeChromeHover>()
        .init_resource::<NativePointerOwnership>()
        .configure_sets(
            Update,
            (
                UiFeed.after(benilla_world::schedule::WorldStage::Net),
                UiInput.before(benilla_world::schedule::WorldStage::Input),
                UiPaint,
            )
                .chain(),
        )
        .add_systems(OnEnter(ClientState::InWorld), spawn_overlay)
        .add_systems(OnExit(ClientState::InWorld), despawn_overlay)
        .add_systems(
            Update,
            native_pointer_input
                .before(UiFeed)
                .run_if(in_state(ClientState::InWorld)),
        )
        .add_systems(
            Update,
            menu_escape
                .in_set(UiInput)
                .after(super::input::feed_ui_input)
                .run_if(in_state(ClientState::InWorld)),
        )
        .add_systems(
            Update,
            update_status
                .in_set(UiInput)
                .run_if(in_state(ClientState::InWorld)),
        )
        .add_systems(
            Update,
            release_native_pointer
                .after(UiInput)
                .run_if(in_state(ClientState::InWorld)),
        );
}

fn spawn_overlay(mut commands: Commands) {
    commands
        .spawn((
            WrathOverlay,
            Interaction::default(),
            bevy::ui::FocusPolicy::Pass,
            GlobalZIndex(1100),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(18.0),
                right: Val::Px(18.0),
                width: Val::Px(390.0),
                padding: UiRect::all(Val::Px(14.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.025, 0.03, 0.045, 0.92)),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("AzerothBench · Wrath native validation"),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.78, 0.2)),
            ));
            panel.spawn((
                SessionStatus,
                Text::new("Session: awaiting status"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            panel.spawn((
                Text::new(
                    "Stock milestone: build-12340 GameMenu Resume, Logout and Quit. Options, bindings, macros, ratings, voice and bag integration remain unavailable.",
                ),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.65, 0.35)),
            ));
            panel.spawn((
                Text::new("Escape: open/close stock Game Menu · movement and gameplay input remain host-owned"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.78, 0.82, 0.9)),
            ));
        });
}

fn despawn_overlay(
    mut commands: Commands,
    roots: Query<Entity, With<WrathOverlay>>,
    mut over: ResMut<PointerOverUi>,
    mut panel: ResMut<PointerOverUiPanel>,
    mut native_hover: ResMut<NativeChromeHover>,
    mut native_pointer: ResMut<NativePointerOwnership>,
) {
    native_hover.0 = false;
    native_pointer.0 = false;
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    over.0 = false;
    panel.0 = false;
}

fn update_status(
    script: Option<NonSend<benilla_ui::script::UiScript>>,
    net: Res<NetStatus>,
    self_player: Query<(), With<SelfPlayer>>,
    logout: Res<crate::ui_logout::LogoutState>,
    mut labels: Query<(&mut Text, &mut TextColor), With<SessionStatus>>,
) {
    let state = truthful_status(net.connected, !self_player.is_empty());
    let (label, color) = if logout.pending() && state != TruthfulSessionStatus::Disconnected {
        (
            "Session: connected · safe logout requested",
            Color::srgb(1.0, 0.8, 0.3),
        )
    } else {
        match state {
            TruthfulSessionStatus::Connected => (
                "Session: connected · self object streamed",
                Color::srgb(0.45, 0.95, 0.55),
            ),
            TruthfulSessionStatus::AwaitingSelf => (
                "Session: connected · awaiting self object",
                Color::srgb(1.0, 0.8, 0.3),
            ),
            TruthfulSessionStatus::Disconnected => {
                ("Session: disconnected", Color::srgb(1.0, 0.35, 0.3))
            }
        }
    };
    for (mut text, mut text_color) in &mut labels {
        **text = label.into();
        text_color.0 = color;
    }
    if let Some(script) = script {
        if let Err(error) =
            super::wrath_stock::set_session_actions_enabled(&script, !logout.pending())
        {
            warn!("wrath stock GameMenu: could not update pending state: {error}");
        }
    }
}

fn native_pointer_input(
    mouse: Res<ButtonInput<MouseButton>>,
    overlay: Query<&Interaction, With<WrathOverlay>>,
    mut over: ResMut<PointerOverUi>,
    mut panel: ResMut<PointerOverUiPanel>,
    mut native_hover: ResMut<NativeChromeHover>,
    mut native_pointer: ResMut<NativePointerOwnership>,
) {
    let hovered = overlay.iter().any(|i| *i != Interaction::None);
    native_hover.0 = hovered;
    over.0 = hovered;
    panel.0 = hovered;
    if hovered && mouse.get_just_pressed().next().is_some() {
        native_pointer.0 = true;
    }
}

fn menu_escape(
    keys: Res<ButtonInput<KeyCode>>,
    mut capture: ResMut<UiKeyboardCapture>,
    script: Option<NonSend<benilla_ui::script::UiScript>>,
) {
    if !keys.just_pressed(KeyCode::Escape) || capture.consumed.contains(&KeyCode::Escape) {
        return;
    }
    let Some(script) = script else {
        warn!("wrath stock GameMenu: Escape ignored because the interface VM is unavailable");
        return;
    };
    match super::wrath_stock::toggle(&script) {
        Ok(open) => {
            // The stock menu owns this Escape press; the later host binding pass must not also
            // execute the legacy native logout binding in the same frame.
            capture.consumed.push(KeyCode::Escape);
            info!(
                "wrath stock GameMenu: {}",
                if open { "opened" } else { "closed" }
            );
        }
        Err(error) => warn!("wrath stock GameMenu: Escape failed: {error}"),
    }
}

fn release_native_pointer(
    mouse: Res<ButtonInput<MouseButton>>,
    mut native_pointer: ResMut<NativePointerOwnership>,
) {
    if native_pointer.0 && mouse.get_pressed().next().is_none() {
        native_pointer.0 = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::NetCommands;

    #[test]
    fn native_route_supplies_the_shared_logout_resource_without_a_lua_vm() {
        use bevy::ecs::system::RunSystemOnce;

        let mut app = App::new();
        build(&mut app);
        let (tx, rx) = crossbeam_channel::unbounded();
        app.insert_resource(NetCommands(tx))
            .init_resource::<crate::net::SelfGuid>()
            .init_resource::<crate::ui_logout::LogoutState>()
            .init_resource::<crate::cinematic::Cinematic>()
            .add_message::<AppExit>();

        assert!(app
            .world()
            .contains_resource::<super::super::ReloadUiPending>());
        assert!(!app
            .world()
            .contains_non_send::<benilla_ui::script::UiScript>());
        app.world_mut()
            .run_system_once(crate::ui_logout::drain_logout)
            .expect("native boot validates the shared logout system without a Lua VM");
        assert!(rx.is_empty(), "a VM-less drain emits no logout command");
    }

    #[test]
    fn status_never_calls_a_socket_ready_before_self_streams() {
        assert_eq!(
            truthful_status(false, false),
            TruthfulSessionStatus::Disconnected
        );
        assert_eq!(
            truthful_status(true, false),
            TruthfulSessionStatus::AwaitingSelf
        );
        assert_eq!(
            truthful_status(true, true),
            TruthfulSessionStatus::Connected
        );
        assert_eq!(
            truthful_status(false, true),
            TruthfulSessionStatus::Disconnected
        );
    }

    #[test]
    fn escape_toggles_the_menu_and_captures_the_legacy_binding_press() {
        use benilla_ui::script::{ScriptDialect, UiScript};
        use bevy::ecs::system::RunSystemOnce;

        let mut app = App::new();
        let script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script
            .run("GameMenuFrame=CreateFrame('Frame','GameMenuFrame'); GameMenuFrame:Hide()")
            .unwrap();
        app.insert_non_send_resource(script)
            .init_resource::<UiKeyboardCapture>()
            .init_resource::<ButtonInput<KeyCode>>();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.world_mut().run_system_once(menu_escape).unwrap();

        let script = app
            .world()
            .non_send_resource::<benilla_ui::script::UiScript>();
        assert!(script
            .eval::<bool>("return GameMenuFrame:IsShown()")
            .unwrap());
        assert_eq!(
            app.world().resource::<UiKeyboardCapture>().consumed,
            [KeyCode::Escape]
        );
    }
}
