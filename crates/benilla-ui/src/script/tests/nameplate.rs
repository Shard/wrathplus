//! The nameplate ABI as four vanilla addons read it, one idiom per test:
//! `pfUI/modules/nameplates.lua`, `ShaguTweaks/libs/libnameplate.lua`,
//! `CustomNameplates/CustomNameplates.lua` and `_Nameplates/_Nameplates.lua`.

use crate::script::{PlateGeometry, PlateState, UiScript};

/// A VM with a `WorldFrame` to hang plates off, all [`UiScript::sync_nameplates`] requires.
pub(super) fn vm() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    s.run(r#"WorldFrame = CreateFrame("WorldFrame", "WorldFrame") WorldFrame:SetAllPoints()"#)
        .unwrap();
    s.resolve();
    s
}

/// A 1280 px window, where one gx unit is 1280 px, so the 0.1 × 0.025 plate is 128 × 32.
pub(super) fn geometry() -> PlateGeometry {
    PlateGeometry {
        width: 128.0,
        height: 32.0,
        bar_off_x: 4.0,
        bar_off_y: 4.0,
        bar_width: 103.0,
        bar_height: 9.0,
        level_off_x: 11.8,
        level_off_y: 9.1,
        skull_size: 12.8,
        raid_size: 25.6,
        name_height: 12.8,
        level_height: 11.0,
        shadow_offset: 1.0,
    }
}

pub(super) fn plate(name: &str, health: f32, max: f32) -> PlateState {
    PlateState {
        // A hash of the name stands in for the GUID: stable per unit, distinct between units.
        key: name.bytes().map(u64::from).sum(),
        top_centre: (500.0, 400.0),
        health,
        max_health: max,
        bar_colour: [1.0, 0.0, 0.0],
        name: name.to_string(),
        level: Some(12),
        skull: false,
        level_colour: [1.0, 1.0, 0.0],
        raid_icon: None,
        alpha: 1.0,
        lit: false,
        hovered: false,
        elite: false,
        cast: None,
        threat: None,
    }
}

/// Drive N plates and settle the layout, the way one app frame does.
pub(super) fn drive(s: &mut UiScript, states: &[PlateState]) {
    s.sync_nameplates(geometry(), states);
    s.resolve();
}

/// pfUI and ShaguTweaks walk only the children past a count they never reset, so the list must not
/// shrink or reorder: a retired plate keeps its slot.
#[test]
fn the_worldframe_child_list_only_grows_and_keeps_its_order() {
    let mut s = vm();
    assert_eq!(
        s.eval::<i64>("return WorldFrame:GetNumChildren()").unwrap(),
        0
    );

    drive(
        &mut s,
        &[plate("Wolf", 30.0, 40.0), plate("Bear", 10.0, 90.0)],
    );
    assert_eq!(
        s.eval::<i64>("return WorldFrame:GetNumChildren()").unwrap(),
        2
    );

    // One unit leaves: its plate hides, and the list keeps its length.
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    assert_eq!(
        s.eval::<i64>("return WorldFrame:GetNumChildren()").unwrap(),
        2
    );
    assert!(s
        .eval::<bool>(
            "local a, b = WorldFrame:GetChildren() return a:IsShown() == 1 and b:IsShown() == nil"
        )
        .unwrap());

    // A new unit takes the pooled plate back: the count stays 2.
    drive(
        &mut s,
        &[plate("Wolf", 30.0, 40.0), plate("Boar", 5.0, 50.0)],
    );
    assert_eq!(
        s.eval::<i64>("return WorldFrame:GetNumChildren()").unwrap(),
        2
    );
    assert_eq!(s.nameplate_pool_len(), 2);
}

/// pfUI's `IsNamePlate` (`nameplates.lua:70`) and ShaguTweaks' (`libnameplate.lua:12`): a `Button`
/// whose second region is a `Texture` reading `Interface\Tooltips\Nameplate-Border`, the test 3.3.5
/// plate addons make (`select(2, frame:GetRegions())`).
#[test]
fn a_plate_identifies_by_button_plus_the_border_texture() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    assert!(s
        .eval::<bool>(
            r#"
            local plate = WorldFrame:GetChildren()
            if plate:GetObjectType() ~= "Button" then return false end
            local _, region = plate:GetRegions()
            if not region or not region.GetTexture then return false end
            if region:GetObjectType() ~= "Texture" then return false end
            return region:GetTexture() == "Interface\\Tooltips\\Nameplate-Border"
        "#
        )
        .unwrap());
}

/// Build 12340's constructor order (`0x98f790`), which 3.3.5 plate addons destructure: eleven
/// regions (threat flash, health border, cast border, cast shield, spell icon, highlight, name,
/// level, boss skull, raid icon, elite icon) and two children, the health bar then the cast bar.
#[test]
fn the_region_and_child_tuples_are_eleven_and_two() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);

    assert_eq!(
        s.eval::<i64>("local p = WorldFrame:GetChildren() return p:GetNumRegions()")
            .unwrap(),
        11
    );
    assert_eq!(
        s.eval::<i64>("local p = WorldFrame:GetChildren() return p:GetNumChildren()")
            .unwrap(),
        2
    );
    assert!(s
        .eval::<bool>(
            r#"
            local p = WorldFrame:GetChildren()
            local flash, border, castborder, shield, icon, glow, name, level, skull, raid, elite =
                p:GetRegions()
            return flash:GetTexture()      == "Interface\\TargetingFrame\\UI-TargetingFrame-Flash"
               and border:GetTexture()     == "Interface\\Tooltips\\Nameplate-Border"
               and castborder:GetTexture() == "Interface\\Tooltips\\Nameplate-Border"
               and shield:GetTexture()     == "Interface\\Tooltips\\Nameplate-CastBar-Shield"
               and icon:GetObjectType()    == "Texture"
               and glow:GetTexture()       == "Interface\\Tooltips\\Nameplate-Glow"
               and name:GetObjectType()    == "FontString"
               and level:GetObjectType()   == "FontString"
               and skull:GetTexture()      == "Interface\\TargetingFrame\\UI-TargetingFrame-Skull"
               and raid:GetTexture()       == "Interface\\TargetingFrame\\UI-RaidTargetingIcons"
               and elite:GetTexture()      == "Interface\\Tooltips\\EliteNameplateIcon"
        "#
        )
        .unwrap());
    // Both children are StatusBars whose only region is the `BarFill`.
    assert!(s
        .eval::<bool>(
            r#"
            local p = WorldFrame:GetChildren()
            for _, bar in ipairs({ p:GetChildren() }) do
                if bar:GetObjectType() ~= "StatusBar" then return false end
                if bar:GetNumRegions() ~= 1 then return false end
                local fill = bar:GetStatusBarTexture()
                if fill:GetTexture() ~= "Interface\\TargetingFrame\\UI-TargetingFrame-BarFill" then
                    return false
                end
            end
            return true
        "#
        )
        .unwrap());
}

/// `_Nameplates.lua:163` rejects a plate unless its name is anchored `BOTTOM` to `CENTER` and its
/// level `CENTER` to `BOTTOMRIGHT`, the reference's own anchors (`0x7cb456`, `0x7cb7f5`).
#[test]
fn the_two_fontstring_anchors_are_what_nameplates_validates() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    assert!(s
        .eval::<bool>(
            r#"
            local plate = WorldFrame:GetChildren()
            local Name, Level
            for _, region in ipairs({ plate:GetRegions() }) do
                if region:GetObjectType() == "FontString" then
                    local point, _, relativePoint = region:GetPoint()
                    if point == "BOTTOM" and relativePoint == "CENTER" then Name = region end
                    if point == "CENTER" and relativePoint == "BOTTOMRIGHT" then Level = region end
                end
            end
            return Name ~= nil and Level ~= nil
        "#
        )
        .unwrap());
}

/// `GetValue` (`0x78f5d0`) is raw health, `[bar+0x320]`, not the fill fraction (`0x783380`), and
/// `GetMinMaxValues` is `(0, maxHealth)`; pfUI (`:595`) and CustomNameplates (`:471`) divide them.
#[test]
fn the_healthbar_reports_raw_health_and_its_max() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    assert!(s
        .eval::<bool>(
            r#"
            local healthbar = WorldFrame:GetChildren():GetChildren()
            local min, max = healthbar:GetMinMaxValues()
            return healthbar:GetValue() == 30 and min == 0 and max == 40
        "#
        )
        .unwrap());
}

/// `_Nameplates.lua:164` and pfUI's bubble filter skip any WorldFrame child with a name.
#[test]
fn a_plate_is_anonymous() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    assert!(s
        .eval::<bool>("return WorldFrame:GetChildren():GetName() == nil")
        .unwrap());
}

/// Addons key per-plate state on the plate object: pfUI's `registry[plate]` (`:342`) and
/// `_Nameplates.Frames[Nameplate]` (`:205`).
#[test]
fn a_retired_plate_returns_as_the_same_lua_object() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    s.run("FirstPlate = WorldFrame:GetChildren()").unwrap();
    drive(&mut s, &[]);
    drive(&mut s, &[plate("Bear", 10.0, 90.0)]);
    assert!(s
        .eval::<bool>("return WorldFrame:GetChildren() == FirstPlate")
        .unwrap());
}

/// The pool is keyed by the unit's GUID, not by the driver's distance order, which changes when two
/// units cross; addons cache per-unit state on the plate.
#[test]
fn a_plate_belongs_to_its_unit_across_a_sort_order_swap() {
    let mut s = vm();
    drive(
        &mut s,
        &[plate("Wolf", 30.0, 40.0), plate("Bear", 10.0, 90.0)],
    );
    s.run(
        r#"
        WolfPlate = nil
        for _, p in ipairs({ WorldFrame:GetChildren() }) do
            local name = select(7, p:GetRegions())
            if name:GetText() == "Wolf" then WolfPlate = p end
        end
    "#,
    )
    .unwrap();
    assert!(s.eval::<bool>("return WolfPlate ~= nil").unwrap());

    // The units cross: the driver now sorts Bear first.
    drive(
        &mut s,
        &[plate("Bear", 10.0, 90.0), plate("Wolf", 30.0, 40.0)],
    );
    assert!(s
        .eval::<bool>(
            r#"
            local name = select(7, WolfPlate:GetRegions())
            return name:GetText() == "Wolf"
        "#
        )
        .unwrap());
}

/// pfUI (`:879`) and CustomNameplates (`:287`) find the target plate as the one at alpha 1; with a
/// target up, every other plate is dimmed.
#[test]
fn the_target_plate_is_the_opaque_one() {
    let mut s = vm();
    let mut target = plate("Wolf", 30.0, 40.0);
    let mut other = plate("Bear", 10.0, 90.0);
    target.alpha = 1.0;
    other.alpha = 0.5;
    drive(&mut s, &[target, other]);
    assert!(s
        .eval::<bool>(
            "local a, b = WorldFrame:GetChildren() return a:GetAlpha() == 1 and b:GetAlpha() < 1"
        )
        .unwrap());
}

/// pfUI's bar `OnValueChanged` (`:393`) finds the plate by `this:GetParent()`; the reference sets
/// the bar from a GUID-watch callback that fires the handler, as the driver's write does here.
#[test]
fn an_engine_health_write_fires_onvaluechanged_with_the_plate_as_parent() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    s.run(
        r#"
        Fired = 0
        ParentIsPlate = false
        local plate = WorldFrame:GetChildren()
        local healthbar = plate:GetChildren()
        healthbar:SetScript("OnValueChanged", function(this, arg1, arg2, arg3, arg4)
            Fired = Fired + 1
            ParentIsPlate = (this:GetParent() == plate)
        end)
    "#,
    )
    .unwrap();
    drive(&mut s, &[plate("Wolf", 12.0, 40.0)]);
    assert_eq!(s.eval::<i64>("return Fired").unwrap(), 1);
    assert!(s.eval::<bool>("return ParentIsPlate").unwrap());
    // Unmoved health fires nothing: the driver writes only what changed.
    drive(&mut s, &[plate("Wolf", 12.0, 40.0)]);
    assert_eq!(s.eval::<i64>("return Fired").unwrap(), 1);
}

/// The driver writes a plate's static properties once, at creation, so an addon's takeover (pfUI
/// blanks all its regions, ShaguTweaks reparents the raid icon) survives.
#[test]
fn the_driver_does_not_overwrite_an_addons_takeover() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    s.run(
        r#"
        local plate = WorldFrame:GetChildren()
        for _, region in ipairs({ plate:GetRegions() }) do
            if region.SetTexture then region:SetTexture("") end
        end
    "#,
    )
    .unwrap();
    // Several frames of driving: health moves and the plate re-seats.
    for hp in [29.0, 28.0, 27.0] {
        let mut state = plate("Wolf", hp, 40.0);
        state.top_centre = (500.0 + hp, 400.0);
        drive(&mut s, &[state]);
    }
    assert!(s
        .eval::<bool>(
            r#"
            local _, border = WorldFrame:GetChildren():GetRegions()
            return border:GetTexture() == "" or border:GetTexture() == nil
        "#
        )
        .unwrap());
}

#[test]
fn a_plate_created_after_the_first_frame_is_laid_out() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    // A second unit on a later frame, with no free slot and no geometry change.
    drive(
        &mut s,
        &[plate("Wolf", 30.0, 40.0), plate("Bear", 10.0, 90.0)],
    );
    let (w, h): (f32, f32) = s
        .eval("local _, b = WorldFrame:GetChildren() return b:GetWidth(), b:GetHeight()")
        .unwrap();
    assert_eq!((w, h), (128.0, 32.0), "the second plate has the plate size");
    // Its regions are anchored to it, or `GetLeft` would not answer.
    assert!(s
        .eval::<bool>(
            r#"local _, b = WorldFrame:GetChildren()
               local _, border = b:GetRegions()
               return border:GetLeft() == b:GetLeft() and border:GetRight() == b:GetRight()"#
        )
        .unwrap());
}

/// An addon's frame under `WorldFrame` (`pfUICombatScreen`, `_NameplatesFrame`) shares the list
/// with the plates, whose `GetName` is always nil (`0x7a1390`).
#[test]
fn an_addon_frame_under_the_worldframe_is_not_a_plate() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    s.run(r#"CreateFrame("Frame", "pfUICombatScreen", WorldFrame)"#)
        .unwrap();
    assert_eq!(
        s.eval::<i64>("return WorldFrame:GetNumChildren()").unwrap(),
        2
    );
    assert_eq!(
        s.eval::<i64>(
            r#"
            local found = 0
            for _, frame in ipairs({ WorldFrame:GetChildren() }) do
                local _, region = frame:GetRegions()
                if frame:GetObjectType() == "Button" and region and region.GetTexture
                   and region:GetTexture() == "Interface\\Tooltips\\Nameplate-Border" then
                    found = found + 1
                end
            end
            return found
        "#
        )
        .unwrap(),
        1
    );
}
