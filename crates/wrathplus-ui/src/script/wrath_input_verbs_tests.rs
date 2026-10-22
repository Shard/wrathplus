use std::collections::HashMap;

use crate::script::{BindingInput, FiredInput, HeldInput, ScriptCall, UiScript, UnitGuids};

const ME: u64 = 0x1;
const MOB: u64 = 0xF130_0000_0000_0042;

/// `MoveAndSteerStart` raises the left button's channel then the right's; Stop clears them in the
/// same order (`0x5fc780`, `0x5fc830`), and only inside a binding body.
#[test]
fn move_and_steer_drives_both_mouse_channels_in_the_references_order() {
    let mut s = UiScript::new().unwrap();
    s.run("MoveAndSteerStart() MoveAndSteerStop()").unwrap();
    assert!(s.take_binding_input().is_empty(), "the movement gate");
    s.run_binding("MoveAndSteerStart() MoveAndSteerStop()")
        .unwrap();
    assert_eq!(
        s.take_binding_input(),
        vec![
            BindingInput::Start(HeldInput::CameraOrSelectOrMove),
            BindingInput::Start(HeldInput::TurnOrAction),
            BindingInput::Stop(HeldInput::CameraOrSelectOrMove),
            BindingInput::Stop(HeldInput::TurnOrAction),
        ]
    );
}

/// The vehicle zoom is the camera zoom (`0x6018a0` → `0x6017e0`), ungated, its amount 1.0 when
/// absent.
#[test]
fn the_vehicle_zoom_is_the_camera_zoom() {
    let mut s = UiScript::new().unwrap();
    s.run("VehicleCameraZoomIn(1.0) VehicleCameraZoomOut() VehicleCameraZoomIn('2')")
        .unwrap();
    assert_eq!(
        s.take_binding_input(),
        vec![
            BindingInput::Fire(FiredInput::CameraZoomIn(1.0)),
            BindingInput::Fire(FiredInput::CameraZoomOut(1.0)),
            BindingInput::Fire(FiredInput::CameraZoomIn(2.0)),
        ]
    );
}

/// The stock `INTERACTMOUSEOVER` body falls back to the target when nothing is under the mouse:
/// nil for a token naming nobody, 1 and one queued interact for the target.
#[test]
fn interact_unit_answers_and_queues_for_a_named_unit_only() {
    let mut s = UiScript::new().unwrap();
    s.set_unit_guids(&UnitGuids {
        player: ME,
        target: MOB,
        held: HashMap::from([(ME, MOB), (MOB, 0)]),
        ..Default::default()
    });
    s.take_script_calls();
    s.run_binding(
        r#"
        R1 = InteractUnit("mouseover")
        if ( not R1 ) then
            R2 = InteractUnit("target")
        end
    "#,
    )
    .unwrap();
    assert_eq!(s.eval::<Option<f64>>("return R1").unwrap(), None);
    assert_eq!(s.eval::<Option<f64>>("return R2").unwrap(), Some(1.0));
    assert_eq!(
        s.take_script_calls(),
        vec![ScriptCall::InteractUnit { guid: MOB }]
    );
    let err = s.run("InteractUnit()").unwrap_err().to_string();
    assert!(
        err.contains("Usage: InteractUnit(unitToken[, exactMatch])"),
        "{err}"
    );
}
