//! The glue VM is built only once text can be measured: GlueXML sizes buttons from their labels
//! in `OnLoad`, as the reference's synchronous measure lets it.

use super::*;
use crate::app_core::state::ClientState;
use bevy::window::PrimaryWindow;

/// A windowed client on the login screen, before the font atlas exists.
fn windowed_login() -> World {
    let mut world = World::new();
    world.spawn((
        Window {
            resolution: (1024, 768).into(),
            ..Default::default()
        },
        PrimaryWindow,
    ));
    world.insert_resource(State::new(ClientState::Login));
    world.init_resource::<GlueVm>();
    world.init_resource::<DeferredGlueEntry>();
    world
}

/// `CharSelectChangeRealmButton` (`CharacterSelect.xml:462-476`) is `GlueButtonSmallTemplate`
/// resized in `OnLoad` to `GetTextWidth() + 50`. Built before the atlas, that read 0 and left a
/// 50-wide stub of red art behind the label. Now the entry waits for the atlas, the label
/// measures, and the button's one texture (`GluePanelButtonUpTexture`, a single file cut by
/// `TexCoords`, set all points by the implicit anchor) spans the whole button.
#[test]
fn the_glue_vm_waits_for_the_text_measure_and_the_change_realm_button_spans_its_label() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut world = windowed_login();
    enter_stock_login(&mut world);
    assert!(
        world.get_non_send_resource::<UiScript>().is_none(),
        "no glue VM is built before the text measure exists"
    );
    assert_eq!(
        world.resource::<DeferredGlueEntry>().0,
        Some(ClientState::Login)
    );

    world.insert_resource(crate::ui_text::UiFontAtlas::for_test(1.0).expect("the client faces"));
    finish_deferred_entry(&mut world);
    assert_eq!(world.resource::<DeferredGlueEntry>().0, None);
    let mut script = world.non_send_resource_mut::<UiScript>();
    script.resolve();
    let (width, text, art_l, art_r, btn_l, btn_r): (f32, f32, f32, f32, f32, f32) = script
        .eval(
            "local b = CharSelectChangeRealmButton local t = b:GetNormalTexture() \
             return b:GetWidth(), b:GetTextWidth(), t:GetLeft(), t:GetRight(), \
                 b:GetLeft(), b:GetRight()",
        )
        .unwrap();
    assert!(text > 40.0, "the label \"Change Realm\" measured: {text}");
    assert!(
        (width - (text + 50.0)).abs() < 0.01,
        "the button is its label plus the template's 50: {width} vs {text}"
    );
    assert!(
        (art_l - btn_l).abs() < 0.01 && (art_r - btn_r).abs() < 0.01,
        "the red art spans the button: art {art_l}..{art_r}, button {btn_l}..{btn_r}"
    );
    // The other small glue buttons sized the same way (`CharacterSelect.xml:588`,
    // `CharacterCreate.xml:594`, `:677`).
    for name in [
        "CharSelectCreateCharacterButton",
        "CharCreateRandomizeButton",
        "CharacterCreateRandomName",
    ] {
        let (width, text): (f32, f32) = script
            .eval(&format!("return {name}:GetWidth(), {name}:GetTextWidth()"))
            .unwrap();
        assert!(
            text > 20.0 && (width - (text + 50.0)).abs() < 0.01,
            "{name}: {width} wide for a {text} label"
        );
    }
}

/// A world with no window (a headless test or tool) has no atlas coming: the VM is built at once.
#[test]
fn a_windowless_world_builds_the_glue_vm_at_once() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut world = windowed_login();
    let windows: Vec<Entity> = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .iter(&world)
        .collect();
    for w in windows {
        world.despawn(w);
    }
    enter_stock_login(&mut world);
    assert!(world.get_non_send_resource::<UiScript>().is_some());
    assert_eq!(world.resource::<DeferredGlueEntry>().0, None);
}
