//! The stock `Minimap.xml` driven engine-only. Indoor and outdoor zoom are independent levels, so
//! the +/- buttons must re-sync when a WMO transition switches between them.

use wrathplus_ui::script::UiScript;

use super::test_ui::load_ui as load_xml;

fn enabled(s: &UiScript, button: &str) -> bool {
    s.eval::<bool>(&format!("return {button}:IsEnabled() ~= 0"))
        .unwrap()
}

#[test]
fn minimap_zoom_buttons_resync_when_switching_inside_and_outside() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // Host globals the cluster calls that a bare engine does not install.
    s.run("function GetMinimapZoneText() return '' end")
        .unwrap();
    s.run("function PlaySound() end").unwrap();
    load_xml(&mut s, "Interface\\FrameXML\\Fonts.xml");
    // `GameTooltip` before the cluster, as shipped: `Minimap_Update` touches it from `OnLoad` on.
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.lua");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTooltip.xml");
    load_xml(&mut s, "Interface\\FrameXML\\UIDropDownMenu.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.lua");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.xml");
    load_xml(&mut s, "Interface\\FrameXML\\BattlefieldFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\Minimap.xml");

    // The zoom CVar defaults to 3, a middle level, so both buttons start enabled.
    assert_eq!(s.eval::<u8>("return Minimap:GetZoom()").unwrap(), 3);
    assert!(enabled(&s, "MinimapZoomIn"));
    assert!(enabled(&s, "MinimapZoomOut"));

    // Outdoors at 5, the top level: `MINIMAP_UPDATE_ZOOM` greys ZoomIn (`Minimap.lua:56-64`).
    s.run("Minimap:SetZoom(5)").unwrap();
    s.fire_event("MINIMAP_UPDATE_ZOOM", vec![]);
    assert!(!enabled(&s, "MinimapZoomIn"), "at max zoom ZoomIn disables");
    assert!(enabled(&s, "MinimapZoomOut"));

    // Indoors reads the indoor level, still 3; the app fires `MINIMAP_UPDATE_ZOOM` on the switch.
    s.set_minimap_inside(true);
    s.fire_event("MINIMAP_UPDATE_ZOOM", vec![]);
    assert_eq!(
        s.eval::<u8>("return Minimap:GetZoom()").unwrap(),
        3,
        "indoors reads the separate indoor index"
    );
    assert!(
        enabled(&s, "MinimapZoomIn"),
        "the stale outdoor max-zoom greying must clear — this is the reported bug"
    );
    assert!(enabled(&s, "MinimapZoomOut"));

    s.run("Minimap:SetZoom(0)").unwrap();
    s.fire_event("MINIMAP_UPDATE_ZOOM", vec![]);
    assert!(
        !enabled(&s, "MinimapZoomOut"),
        "at min zoom ZoomOut disables"
    );
    assert!(enabled(&s, "MinimapZoomIn"));

    s.set_minimap_inside(false);
    s.fire_event("MINIMAP_UPDATE_ZOOM", vec![]);
    assert_eq!(s.eval::<u8>("return Minimap:GetZoom()").unwrap(), 5);
    assert!(!enabled(&s, "MinimapZoomIn"), "outdoor max-zoom survived");
    assert!(enabled(&s, "MinimapZoomOut"));
}

/// 3.3.5's tracking button (`MiniMapTracking`, Minimap.xml:479-482) shows `GetTrackingTexture()`
/// in `MiniMapTrackingIcon` on `MINIMAP_UPDATE_TRACKING` (`MiniMapTracking_Update`,
/// Minimap.lua:408-414); 1.12's `MiniMapTrackingFrame` and its `PLAYER_AURAS_CHANGED` are gone.
#[test]
fn the_tracking_icon_follows_get_tracking_texture_on_minimap_update_tracking() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    use wrathplus_ui::script::TrackingState;

    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\Minimap.xml");
    let icon = |s: &UiScript| {
        s.eval::<Option<String>>("return MiniMapTrackingIcon:GetTexture()")
            .unwrap()
            .unwrap_or_default()
    };
    assert!(s.eval::<bool>("return MiniMapTracking:IsShown()").unwrap());

    // A tracking aura: Find Minerals, spell 2580.
    s.set_tracking(Some(TrackingState {
        spell_id: 2580,
        name: Some("Find Minerals".into()),
        icon: Some("Interface\\Icons\\Spell_Nature_Earthquake".into()),
        cancelable: true,
    }));
    s.fire_event("MINIMAP_UPDATE_TRACKING", vec![]);
    assert!(
        icon(&s).contains("Spell_Nature_Earthquake"),
        "the icon is the tracking texture: {}",
        icon(&s)
    );

    s.set_tracking(None);
    s.fire_event("MINIMAP_UPDATE_TRACKING", vec![]);
    assert!(
        !icon(&s).contains("Spell_Nature_Earthquake"),
        "no tracking clears it: {}",
        icon(&s)
    );
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// The cluster and the stock `GameTime.xml`, the clock at `hour:minute` as the app pushes it.
fn game_time_session(hour: u32, minute: u32) -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    s.run("function GetMinimapZoneText() return '' end")
        .unwrap();
    s.run("function PlaySound() end").unwrap();
    s.set_game_time(hour, minute);
    // `GameTime.lua` reads `TwentyFourHourTime`, which `LocalizeFrames` sets; the reference calls
    // that on `VARIABLES_LOADED` (`UIParent.lua:231-232`), and this session has no UIParent.
    load_xml(&mut s, "Interface\\FrameXML\\GlobalStrings.lua");
    load_xml(&mut s, "Interface\\FrameXML\\Fonts.xml");
    load_xml(&mut s, "Interface\\FrameXML\\Localization.xml");
    s.run("LocalizeFrames()").unwrap();
    // `TEXT()`, which the tooltip formatting goes through.
    load_xml(&mut s, "Interface\\FrameXML\\BasicControls.xml");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.lua");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTooltip.xml");
    load_xml(&mut s, "Interface\\FrameXML\\UIDropDownMenu.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.lua");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.xml");
    load_xml(&mut s, "Interface\\FrameXML\\BattlefieldFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\Minimap.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTime.xml");
    s
}

/// `GameTimeTexture`'s texcoords as `(left, right, top, bottom)`: `GetTexCoord` answers eight
/// values, `ULx, ULy, LLx, LLy, URx, URy, LRx, LRy`.
fn tod_window(s: &UiScript) -> (f64, f64, f64, f64) {
    let (ulx, uly, _, lly, urx, ..): (f64, f64, f64, f64, f64, f64, f64, f64) =
        s.eval("return GameTimeTexture:GetTexCoord()").unwrap();
    (ulx, urx, uly, lly)
}

/// Stock `GameTimeFrame_Update` slides the 50px window +0.5 onto the moon when
/// `time < DAWN or time >= DUSK`, 5:30 and 21:00 (`GameTime.lua:2-18`).
#[test]
fn game_time_frame_slides_the_sun_moon_window_on_the_game_clock() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = game_time_session(10, 30);
    // `OnLoad` seeds `timeOfDay = 0`, so its own update already seats the window.
    let day = (0.0, 50.0 / 128.0, 0.0, 50.0 / 64.0);
    assert_eq!(tod_window(&s), day, "mid-morning shows the sun half");

    s.set_game_time(21, 0);
    s.tick(0.016);
    assert_eq!(
        tod_window(&s),
        (0.5, 0.5 + 50.0 / 128.0, 0.0, 50.0 / 64.0),
        "9:00 PM sharp is the moon half"
    );

    s.set_game_time(5, 29);
    s.tick(0.016);
    assert_eq!(tod_window(&s).0, 0.5, "5:29 AM is still the moon");
    s.set_game_time(5, 30);
    s.tick(0.016);
    assert_eq!(tod_window(&s), day, "5:30 AM sharp flips to the sun");
}

/// `GameTimeFrame` declares no `enableMouse`: its `<Scripts>` enable the mouse, and its
/// `<HitRectInsets>` trim the hit rect (`GameTime.xml:15-17`).
#[test]
fn hovering_the_indicator_shows_and_live_updates_the_game_time_tooltip() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = game_time_session(21, 7);
    s.resolve();

    // The middle of the hit rect, inset by l=6, r=0, t=5, b=10.
    let (l, r, t, b) = (
        s.eval::<f32>("return GameTimeFrame:GetLeft()").unwrap(),
        s.eval::<f32>("return GameTimeFrame:GetRight()").unwrap(),
        s.eval::<f32>("return GameTimeFrame:GetTop()").unwrap(),
        s.eval::<f32>("return GameTimeFrame:GetBottom()").unwrap(),
    );
    let (x, y) = ((l + 6.0 + r) * 0.5, (b + 10.0 + t - 5.0) * 0.5);
    s.mouse_move(x, y);
    s.tick(0.016); // OnEnter only takes the plate; OnUpdate shows it (GameTime.lua:151, :199)
    assert!(
        s.eval::<bool>("return GameTooltip:IsVisible()").unwrap(),
        "OnEnter owns the tooltip (the Scripts-walker auto-enable capturing at {x},{y})"
    );
    let text = |s: &UiScript| {
        s.eval::<String>("return GameTooltipTextLeft1:GetText()")
            .unwrap()
    };
    assert_eq!(text(&s), "9:07 PM"); // enUS: twelve-hour (`GameTime.lua:22`)

    // While owned, `GameTimeFrame_OnUpdate` refreshes the tooltip (`GameTime.lua:199`).
    s.set_game_time(21, 8);
    s.tick(0.016);
    assert_eq!(text(&s), "9:08 PM", "the owned tooltip follows the clock");

    // Inside the raw 50x50 rect but in the left inset: not hoverable.
    s.mouse_move(l + 2.0, y);
    assert!(
        !s.eval::<bool>("return GameTooltip:IsVisible()").unwrap(),
        "the 6-px left inset band is not hoverable"
    );
}

/// A real click reaches `Minimap_OnClick` via `OnMouseUp` and parks centre-relative UI units.
#[test]
fn a_click_on_the_minimap_parks_a_centre_relative_ping_request() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    s.run("function GetMinimapZoneText() return '' end")
        .unwrap();
    s.run("function PlaySound() end").unwrap();
    load_xml(&mut s, "Interface\\FrameXML\\Fonts.xml");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.lua");
    load_xml(&mut s, r"Interface\FrameXML\MoneyFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\GameTooltip.xml");
    load_xml(&mut s, "Interface\\FrameXML\\UIDropDownMenu.xml");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.lua");
    load_xml(&mut s, r"Interface\FrameXML\UIPanelTemplates.xml");
    load_xml(&mut s, "Interface\\FrameXML\\BattlefieldFrame.xml");
    load_xml(&mut s, "Interface\\FrameXML\\Minimap.xml");
    s.resolve();

    let cx = s
        .eval::<f32>("local x = Minimap:GetCenter(); return x")
        .unwrap();
    let cy = s
        .eval::<f32>("local _, y = Minimap:GetCenter(); return y")
        .unwrap();
    assert!(cx > 0.0 && cy > 0.0, "the widget resolved: ({cx}, {cy})");

    assert_eq!(s.take_minimap_ping_request(), None);

    // 20 right and 12 up, inside the 70-unit disc; 3.3.5's `Minimap_OnClick` pings the cursor's
    // offset from the centre as is (Minimap.lua:177-188), with no 1.12 `CURSOR_OFFSET` shift.
    s.mouse_button(cx + 20.0, cy + 12.0, "LeftButton", true);
    s.mouse_button(cx + 20.0, cy + 12.0, "LeftButton", false);
    let (dx, dy) = s
        .take_minimap_ping_request()
        .expect("OnMouseUp → Minimap_OnClick → PingLocation");
    assert!((dx - 20.0).abs() < 0.01, "x right of centre: {dx}");
    assert!((dy - 12.0).abs() < 0.01, "y UP from centre: {dy}");
    assert_eq!(s.take_minimap_ping_request(), None);

    s.mouse_button(10.0, 10.0, "LeftButton", true);
    s.mouse_button(10.0, 10.0, "LeftButton", false);
    assert_eq!(s.take_minimap_ping_request(), None, "off-widget is no ping");
}

/// `Minimap:GetPingPosition` answers two numbers, `(0, 0)` before any publish. `MINIMAP_PING`
/// seats 3.3.5's texture ping (`MinimapPing`) at the normalized offset times the minimap's size
/// (Minimap.lua:83-90, :129-142).
#[test]
fn get_ping_position_answers_two_numbers_and_a_ping_seats_the_frame() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\Minimap.xml");

    assert_eq!(s.arity("Minimap:GetPingPosition()").unwrap(), 2);
    assert_eq!(
        s.eval::<f32>("return (Minimap:GetPingPosition())").unwrap(),
        0.0,
        "before any ping: a number, not nil"
    );
    s.set_minimap_ping((0.25, -0.125));
    let (x, y) = s
        .eval::<(f32, f32)>("return Minimap:GetPingPosition()")
        .unwrap();
    assert!(
        (x - 0.25).abs() < 1e-6 && (y + 0.125).abs() < 1e-6,
        "{x} {y}"
    );

    let shown = |s: &UiScript| s.eval::<bool>("return MinimapPing:IsVisible()").unwrap();
    assert!(!shown(&s), "hidden until a ping");
    s.fire_event(
        "MINIMAP_PING",
        vec![
            wrathplus_ui::script::ScriptValue::Str("player".into()),
            wrathplus_ui::script::ScriptValue::Number(0.25),
            wrathplus_ui::script::ScriptValue::Number(-0.125),
        ],
    );
    assert!(shown(&s));
    s.resolve();
    let (dx, w) = s
        .eval::<(f32, f32)>(
            "local x = MinimapPing:GetCenter() local mx = Minimap:GetCenter() \
             return x - mx, Minimap:GetWidth()",
        )
        .unwrap();
    assert!(
        (dx - 0.25 * w).abs() < 0.5,
        "seated at x times the width: {dx} of {w}"
    );
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// `MinimapPing_OnUpdate` holds the ping `MINIMAPPING_TIMER` (5.5 s), fading the last 0.5
/// (Minimap.lua:100-125), turning the spinner with `Texture:SetRotation`.
#[test]
fn the_ping_holds_then_fades_on_its_timer() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\Minimap.xml");
    let shown = |s: &UiScript| s.eval::<bool>("return MinimapPing:IsVisible()").unwrap();
    s.fire_event(
        "MINIMAP_PING",
        vec![
            wrathplus_ui::script::ScriptValue::Str("player".into()),
            wrathplus_ui::script::ScriptValue::Number(0.25),
            wrathplus_ui::script::ScriptValue::Number(-0.125),
        ],
    );
    assert!(shown(&s));

    // 0.1 s frames: still up through the hold, gone once the fade ends.
    for _ in 0..49 {
        s.tick(0.1);
    }
    assert!(shown(&s), "held at 4.9 s");
    for _ in 0..8 {
        s.tick(0.1);
    }
    assert!(!shown(&s), "5.5 s hold, the last 0.5 fading, then hidden");
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// The stock 3.3.5 tracking menu (`MiniMapTrackingDropDown_Initialize`, Minimap.lua:424-466) over
/// installed `Spell.dbc` data: the book's tracking spells, classified as the app's feed does, come
/// first, then the town types a mage may use and None. A spell row casts; a type row selects and
/// repaints the icon; None leaves the folder's `None` icon (12340 `0x57f1b0`, `0x57f380`,
/// `0x57f4f0`).
#[test]
fn the_stock_tracking_menu_lists_the_books_tracking_spells_then_the_town_types() {
    let data = wrathplus_formats::wow_data_or_skip!();
    use wrathplus_ui::script::{SpellBookState, SpellSlotView, UnitState};

    let mut chain = wrathplus_formats::Chain::open_wrath(&data, "enUS").expect("read-only chain");
    let catalog = wrathplus_formats::load_spell_catalog(&mut chain).expect("Spell.dbc");
    // Find Herbs, Find Minerals and Fireball, which tracks nothing.
    let book = SpellBookState {
        slots: [2383u32, 2580, 133]
            .map(|id| {
                let d = catalog.get(id).expect("an installed spell");
                SpellSlotView {
                    spell_id: id,
                    name: d.name.clone(),
                    texture: d.icon.clone(),
                    ..Default::default()
                }
            })
            .to_vec(),
        ..Default::default()
    };

    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_xml(&mut s, "Interface\\FrameXML\\Minimap.xml");
    s.set_unit(
        "player",
        Some(UnitState {
            exists: true,
            class_file: Some("MAGE".into()),
            ..Default::default()
        }),
    );
    s.set_spellbook(book.clone());
    s.set_tracking_spells(crate::ui_aura::tracking_spells(&book, &catalog));

    let open = |s: &mut UiScript| {
        s.run("HideDropDownMenu(1) MiniMapTrackingButton:Click()")
            .unwrap();
        s.eval::<Vec<String>>(
            "local t = {} for i = 1, DropDownList1.numButtons do \
                 t[#t + 1] = _G['DropDownList1Button' .. i]:GetText() or '' end return t",
        )
        .unwrap()
    };
    let icon = |s: &UiScript| {
        s.eval::<String>("return MiniMapTrackingIcon:GetTexture()")
            .unwrap()
    };
    assert_eq!(icon(&s), "Interface\\Minimap\\Tracking\\None");

    let rows = open(&mut s);
    assert_eq!(
        rows.len(),
        2 + 12 + 1,
        "two spells, a mage's twelve types, None: {rows:?}"
    );
    assert_eq!(&rows[..3], ["Find Herbs", "Find Minerals", "Repair"]);
    assert_eq!(rows.last().map(String::as_str), Some("None"));

    s.run("DropDownList1Button1:Click()").unwrap();
    assert_eq!(
        s.take_spell_casts(),
        vec![(2383, false)],
        "a spell row casts it"
    );

    open(&mut s);
    s.run("DropDownList1Button3:Click()").unwrap();
    s.tick(0.0);
    assert_eq!(icon(&s), "Interface\\Minimap\\Tracking\\Repair");
    assert_eq!(s.tracked_town_type(), Some((1, 0x1000)));

    open(&mut s);
    s.run("DropDownList1Button15:Click()").unwrap();
    s.tick(0.0);
    assert_eq!(icon(&s), "Interface\\Minimap\\Tracking\\None");
    assert_eq!(s.tracked_town_type(), None);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}
