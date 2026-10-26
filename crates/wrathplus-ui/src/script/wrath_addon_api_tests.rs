use std::collections::HashMap;
use std::sync::Arc;

use crate::script::{ItemTemplateView, UiScript};

/// AceAddon's enable gate (`AceAddon-3.0.lua:626`): nil through the load, 1 from `PLAYER_LOGIN`.
#[test]
fn is_logged_in_turns_on_at_player_login() {
    let mut s = UiScript::new().unwrap();
    assert!(s
        .eval::<Option<i64>>("return IsLoggedIn()")
        .unwrap()
        .is_none());
    s.fire_event("PLAYER_LOGIN", vec![]);
    assert_eq!(
        s.eval::<Option<i64>>("return IsLoggedIn()").unwrap(),
        Some(1)
    );
}

/// Every argument form reaches the `Item.dbc` join; an iconless display is the question mark and
/// an entry with no row answers nothing.
#[test]
fn get_item_icon_reads_the_item_dbc_join_by_id_string_link_and_name() {
    let mut s = UiScript::new().unwrap();
    s.set_item_icons(Arc::new(HashMap::from([
        (6948, Some(Arc::from("Interface\\Icons\\INV_Misc_Rune_01"))),
        (25, None),
    ])));
    s.set_item_template(
        6948,
        ItemTemplateView {
            name: "Hearthstone".into(),
            ..Default::default()
        },
    );
    let rune = "Interface\\Icons\\INV_Misc_Rune_01";
    for call in [
        "GetItemIcon(6948)",
        "GetItemIcon(6948.4)",
        "GetItemIcon('6948')",
        "GetItemIcon('|cffffffff|Hitem:6948:0:0:0:0:0:0:0:80|h[Hearthstone]|h|r')",
        "GetItemIcon('item:6948')",
        "GetItemIcon('hearthstone')",
    ] {
        let got: Option<String> = s.eval(&format!("return {call}")).unwrap();
        assert_eq!(got.as_deref(), Some(rune), "{call}");
    }
    let unknown: Option<String> = s.eval("return GetItemIcon(25)").unwrap();
    assert_eq!(
        unknown.as_deref(),
        Some("Interface\\Icons\\INV_Misc_QuestionMark")
    );
    for call in ["GetItemIcon(1)", "GetItemIcon('nope')", "GetItemIcon()"] {
        let n: i64 = s.eval(&format!("return select('#', {call})")).unwrap();
        assert_eq!(n, 0, "{call}");
    }
}

/// Gold first, each non-zero part with the height twice, a space between; zero is copper.
#[test]
fn get_coin_texture_string_formats_each_part_with_the_font_height() {
    let s = UiScript::new().unwrap();
    s.run(
        "GOLD_AMOUNT_TEXTURE = '%dg:%d:%d' SILVER_AMOUNT_TEXTURE = '%ds:%d:%d' \
         COPPER_AMOUNT_TEXTURE = '%dc:%d:%d'",
    )
    .unwrap();
    for (call, want) in [
        ("GetCoinTextureString(10203)", "1g:14:14 2s:14:14 3c:14:14"),
        ("GetCoinTextureString(10003, 12)", "1g:12:12 3c:12:12"),
        ("GetCoinTextureString('500', 10.5)", "5s:10:10"),
        ("GetCoinTextureString(0)", "0c:14:14"),
    ] {
        let got: String = s.eval(&format!("return {call}")).unwrap();
        assert_eq!(got, want, "{call}");
    }
    let err = s.run("GetCoinTextureString()").unwrap_err().to_string();
    assert!(
        err.contains("Usage: GetCoinText(amount, fontHeight)"),
        "{err}"
    );
}
