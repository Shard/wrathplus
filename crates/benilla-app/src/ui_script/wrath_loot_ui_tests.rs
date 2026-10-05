use benilla_ui::script::{LootRow, LootState, ScriptDialect, UiScript};

use super::*;

fn solo_snapshot() -> LootState {
    LootState {
        rows: vec![
            Some(LootRow {
                name: Some("12 Copper".into()),
                texture: Some("Interface\\Icons\\INV_Misc_Coin_01".into()),
                quantity: 1,
                quality: Some(0),
                is_coin: true,
                ..LootRow::default()
            }),
            Some(LootRow {
                name: Some("Linen Cloth".into()),
                texture: Some("Interface\\Icons\\INV_Fabric_Linen_01".into()),
                quantity: 2,
                quality: Some(1),
                item_id: 2589,
                ..LootRow::default()
            }),
        ],
        ..LootState::default()
    }
}

fn loaded() -> UiScript {
    let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    script.set_screen_size(1024.0, 768.0);
    assert!(super::super::wrath_addons::load_bootstrap(&script).is_empty());
    assert!(super::super::wrath_stock::load(&script).is_empty());
    let failures = load(&script);
    assert!(failures.is_empty(), "{failures:#?}");
    script.set_loot(Some(solo_snapshot()));
    script.fire_event(
        "LOOT_OPENED",
        vec![benilla_ui::script::ScriptValue::Number(0.0)],
    );
    assert!(script.errors().is_empty(), "{:#?}", script.errors());
    script
}

#[test]
fn installed_stock_loot_frame_shows_server_snapshot_and_manual_click_queues_take() {
    benilla_formats::wow_data_or_skip!();
    let mut script = loaded();

    assert!(script.frame_visible("LootFrame"));
    assert!(script
        .eval::<bool>(
            "return LootButton1:IsShown() and LootButton2:IsShown() \
             and LootButton1Text:GetText()=='12 Copper' \
             and LootButton2Text:GetText()=='Linen Cloth'"
        )
        .unwrap());

    // Scripted Click is used only to invoke the actual installed callback deterministically; the
    // callback calls the Wrath LootSlot take verb, which queues the supported display-row intent.
    script.run("LootButton2:Click('LeftButton')").unwrap();
    assert_eq!(script.take_loot_picks(), [2]);
    assert!(script.take_loot_confirms().is_empty());
    assert!(script.errors().is_empty(), "{:#?}", script.errors());
}

#[test]
fn bounded_adapter_removes_only_unsupported_loot_controls_and_close_is_real() {
    benilla_formats::wow_data_or_skip!();
    let mut script = loaded();

    assert!(script
        .eval::<bool>(
            "return GroupLootDropDown==nil \
             and LootButton1:GetScript('OnEnter')==nil \
             and LootButton1:GetScript('OnUpdate')==nil \
             and LootFrame:GetScript('OnHide')~=nil"
        )
        .unwrap());
    script.run("LootCloseButton:Click()").unwrap();
    assert!(script.take_loot_close());
    assert!(!script.frame_visible("LootFrame"));
}
