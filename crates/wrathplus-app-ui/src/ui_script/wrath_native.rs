//! The Wrath route's shared UI resources and schedule. The stock interface (FrameXML) is the
//! in-world UI; this module only installs what the shared input and paint passes expect.

use bevy::prelude::*;

use crate::ui_script::{CapturePointerPinned, EguiPointerOver, PointerOverUiPanel, SyntheticPointer, UiCostWanted, UiFrameCost, UiPaint, UiPassState};
use crate::app_core::state::CursorPayloadHeld;
use crate::app_core::ui_flags::InspectMode;
use crate::app_core::ui_flags::PlayerUiClickConsumed;
use crate::app_core::ui_flags::PlayerUiHover;
use crate::app_core::state::PointerOverUi;
use crate::app_core::ui_flags::UiClock;
use crate::app_core::schedule::UiFeed;
use crate::app_core::schedule::UiInput;
use crate::app_core::ui_flags::UiKeyboardCapture;

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
        .init_resource::<super::ReloadUiPending>()
        .configure_sets(
            Update,
            (
                UiFeed.after(wrathplus_world::schedule::WorldStage::Net),
                UiInput.before(wrathplus_world::schedule::WorldStage::Input),
                UiPaint,
            )
                .chain(),
        );
}
