//! Whole-tree guards over the shipped interface: `assets/ui` and the manifest it loads.

/// Every shipped `assets/ui` `.xml`, the Wrath root's included, parses (XML forbids `--` inside a
/// comment). Parse-only: a file loaded out of manifest order reports templates its predecessors
/// define.
#[test]
fn every_shipped_ui_xml_parses() {
    let mut checked = 0;
    for path in super::content::shipped_tree_files() {
        if path.ends_with(".xml") {
            let text = super::content::read(path).expect("read");
            if let Err(e) = benilla_ui::framexml::parse(&text) {
                panic!("{path}: {e}");
            }
            checked += 1;
        }
    }
    // A floor, not a census: a moved directory must not pass by sweeping nothing.
    assert!(checked >= 1, "only {checked} xml files swept — sweep broke");
}

/// The whole manifest loads in its real order with zero loader errors, which the app only logs.
#[test]
fn the_whole_shipped_manifest_loads_without_errors() {
    benilla_formats::wow_data_or_skip!();
    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // The in-game UI loads on world entry, so a player always exists by then.
    s.set_unit(
        "player",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Probefour".into()),
            level: 60,
            ..Default::default()
        }),
    );
    let failures = super::load_default_ui(&mut s);
    assert!(failures.is_empty(), "manifest load errors: {failures:#?}");
    s.resolve();
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// Loading the UI makes no sound: a dropdown's OnLoad runs `UIDropDownMenu_Initialize`, which calls
/// its init at once (`UIDropDownMenu.lua:50`), and a unit popup's init reaches `UnitPopup_ShowMenu`
/// and its closing `PlaySound("igMainMenuOpen")` unless `UnitPopup_HideButtons` leaves only CANCEL
/// for a unit that does not exist (`UnitPopup.lua:115`).
#[test]
fn loading_the_shipped_ui_queues_no_sounds() {
    benilla_formats::wow_data_or_skip!();
    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // The in-game UI loads on world entry, so a player always exists by then.
    s.set_unit(
        "player",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Probefour".into()),
            level: 60,
            ..Default::default()
        }),
    );
    let failures = super::load_default_ui(&mut s);
    assert!(failures.is_empty(), "manifest load errors: {failures:#?}");
    s.resolve();
    assert_eq!(
        s.take_sounds(),
        vec![],
        "loading the UI played a sound — a load-time handler is ringing; see \
         UnitPopup_HideButtons / UIDropDownMenu_Initialize"
    );
}

/// Every `$parentTextureFrame` outranks its unit frame's status bars by frame level: level is the
/// only draw-key term above the layer, which would otherwise lift an ARTWORK bar fill over
/// BACKGROUND art. The stock frames spend a level on it (`TargetFrame.lua:32-34`) or nest the art
/// in anonymous frames (`PlayerFrame.xml:50-52`).
#[test]
fn every_texture_frame_outranks_its_status_bars() {
    benilla_formats::wow_data_or_skip!();
    use benilla_ui::order::unpack;

    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // The in-game UI loads on world entry, so a player always exists by then.
    s.set_unit(
        "player",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Probefour".into()),
            level: 60,
            ..Default::default()
        }),
    );
    let failures = super::load_default_ui(&mut s);
    assert!(failures.is_empty(), "manifest load errors: {failures:#?}");
    // Every unit frame shown: a hidden frame emits no quads to read a level from.
    for unit in [
        "player",
        "target",
        "targettarget",
        "party1",
        "party2",
        "party3",
        "party4",
    ] {
        s.set_unit(
            unit,
            Some(benilla_ui::script::UnitState {
                exists: true,
                name: Some("Someone".into()),
                health: 60,
                max_health: 100,
                level: 60,
                power_type: 0,
                power: 60,
                max_power: 100,
                // `PVPBattleground_UpdateQueueStatus` concatenates the player's faction.
                faction_group: Some("Horde".into()),
                faction_group_localized: Some("Horde".into()),
                ..benilla_ui::script::UnitState::default()
            }),
        );
    }
    s.fire_event("PLAYER_ENTERING_WORLD", vec![]);
    s.fire_event("PARTY_MEMBERS_CHANGED", vec![]);
    // Target-of-target is the target frame's `totFrame` (TargetFrame.lua:104); its uvar ships
    // off (InterfaceOptionsFrame.lua's uvarInfo), so the test turns it on.
    s.run(r#"SHOW_TARGET_OF_TARGET = "1""#).unwrap();
    s.run("TargetofTarget_Update(TargetFrame.totFrame)")
        .unwrap();
    s.resolve();

    // owner frame name → its (strata, level), read off the packed draw key the renderer sorts by.
    let mut level_of: std::collections::BTreeMap<String, (u8, u16)> =
        std::collections::BTreeMap::new();
    for q in s.extract() {
        if let Some(name) = s.quad_owner_name(q.target) {
            let p = unpack(q.z);
            level_of.insert(name, (p.strata, p.level));
        }
    }

    let mut checked = 0;
    for (texture_frame, &(tf_strata, tf_level)) in &level_of {
        let Some(base) = texture_frame.strip_suffix("TextureFrame") else {
            continue;
        };
        for suffix in ["HealthBar", "ManaBar"] {
            let bar = format!("{base}{suffix}");
            let Some(&(bar_strata, bar_level)) = level_of.get(&bar) else {
                continue;
            };
            assert_eq!(
                bar_strata, tf_strata,
                "{bar} and {texture_frame} must share a strata for the level to decide"
            );
            assert!(
                tf_level > bar_level,
                "{texture_frame} (level {tf_level}) must outrank {bar} (level {bar_level}): \
                 tied, the draw layer lifts the bar's ARTWORK fill over the frame's BACKGROUND art"
            );
            checked += 1;
        }
    }
    // 3.3.5's TargetFrameTemplate and its target-of-target name their TextureFrames
    // (TargetFrame.xml); the player, pet and party frames nest their art in anonymous frames. The
    // floor is the target frame's own two bars, so the sweep cannot pass by finding nothing.
    assert!(
        checked >= 2,
        "only {checked} texture-frame/bar pairs checked — the name sweep found nothing"
    );
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// The shipped UI takes `VARIABLES_LOADED`, which the saved-variables load fires at every launch
/// before any window has shown, without a script error.
#[test]
fn the_shipped_ui_takes_variables_loaded_without_a_script_error() {
    benilla_formats::wow_data_or_skip!();
    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // The in-game UI loads on world entry, so a player always exists by then.
    s.set_unit(
        "player",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Probefour".into()),
            level: 60,
            ..Default::default()
        }),
    );
    let failures = super::load_default_ui(&mut s);
    assert!(failures.is_empty(), "loader errors: {failures:?}");
    let _ = s.errors(); // drain the load's own errors
    s.fire_event("VARIABLES_LOADED", vec![]);
    assert!(
        s.errors().is_empty(),
        "VARIABLES_LOADED script errors: {:?}",
        s.errors()
    );
}

/// Every `<Font name=…>` the manifest declares is a `Font` global answering the FontInstance
/// getters, in the production order: `publish_global` never overwrites, so a same-named frame
/// loaded first would keep a font unpublished.
#[test]
fn every_shipped_font_object_is_published_as_a_lua_global() {
    benilla_formats::wow_data_or_skip!();
    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // The in-game UI loads on world entry, so a player always exists by then.
    s.set_unit(
        "player",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Probefour".into()),
            level: 60,
            ..Default::default()
        }),
    );
    let failures = super::load_default_ui(&mut s);
    assert!(failures.is_empty(), "loader errors: {failures:?}");

    // The names come from the manifest's own entries, chain files included.
    let mut names: Vec<String> = Vec::new();
    for entry in &super::manifest::manifest_files() {
        if !entry.to_ascii_lowercase().ends_with(".xml") {
            continue;
        }
        let Some(bytes) = super::test_ui::read(&entry.replace('\\', "/")) else {
            continue;
        };
        let text = benilla_ui::source::decode(&bytes);
        for chunk in text.split("<Font ").skip(1) {
            if let Some(rest) = chunk.split_once("name=\"") {
                if let Some((name, _)) = rest.1.split_once('"') {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    assert!(
        names.len() >= 50,
        "only {} <Font name=> declarations found — the sweep broke",
        names.len()
    );

    let mut unpublished: Vec<&str> = Vec::new();
    for name in &names {
        match s.eval::<String>(&format!("return {name}:GetObjectType()")) {
            Ok(t) if t == "Font" => {}
            _ => unpublished.push(name),
        }
    }
    assert!(
        unpublished.is_empty(),
        "declared <Font name=> that is not a Font global: {unpublished:?}"
    );

    // The four font objects addons use most carry a face and a size, not just a name.
    for name in [
        "GameFontNormal",
        "GameTooltipText",
        "GameFontHighlightSmall",
        "GameTooltipHeaderText",
    ] {
        let (face, height) = s
            .eval::<(String, f32)>(&format!("return {name}:GetFont()"))
            .unwrap_or_else(|e| panic!("{name}:GetFont() — {e}"));
        assert!(
            face.to_ascii_uppercase().ends_with(".TTF"),
            "{name}: {face}"
        );
        assert!(height > 0.0, "{name}: height {height}");
    }
}

/// `TargetBuffFrameTemplate` (`TargetFrame.xml:118`, 21×21) and `MainMenuBarMicroButton`
/// (`MainMenuBarMicroButtons.xml`, 28×58) confer their size, regions and scripts, read back as a
/// consumer gets them: an unresolved `inherits=` fails silently.
#[test]
fn the_inheritable_reference_templates_confer_their_shape() {
    benilla_formats::wow_data_or_skip!();
    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // The in-game UI loads on world entry, so a player always exists by then.
    s.set_unit(
        "player",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Probefour".into()),
            level: 60,
            ..Default::default()
        }),
    );
    assert!(super::load_default_ui(&mut s).is_empty());

    s.run(
        r#"
        BuffProbe = CreateFrame("Frame", "BuffProbe", UIParent, "TargetBuffFrameTemplate")
        MicroProbe = CreateFrame("Button", "MicroProbe", UIParent, "MainMenuBarMicroButton")
        "#,
    )
    .unwrap();
    s.resolve();

    assert_eq!(
        s.eval::<(f64, f64)>("return BuffProbe:GetWidth(), BuffProbe:GetHeight()")
            .unwrap(),
        (21.0, 21.0),
        "the buff frame carries the reference's 21x21"
    );
    assert!(
        s.eval::<bool>("return BuffProbeIcon ~= nil").unwrap(),
        "$parentIcon is the region a consumer getglobals to set the texture"
    );
    assert!(
        s.eval::<bool>(r#"return BuffProbe:GetScript("OnEnter") ~= nil"#)
            .unwrap(),
        "the tooltip script comes with the template"
    );

    assert_eq!(
        s.eval::<(f64, f64)>("return MicroProbe:GetWidth(), MicroProbe:GetHeight()")
            .unwrap(),
        (28.0, 58.0),
        "MainMenuBarMicroButtons.xml:4-7"
    );
    assert!(s
        .eval::<bool>(r#"return MicroProbe:GetScript("OnEnter") ~= nil"#)
        .unwrap());
    assert!(s.errors().is_empty(), "no script errors: {:?}", s.errors());
}

/// The stock inspect-cursor pair, `CursorUpdate` and `CursorOnUpdate` (`UIParent.lua:1805-1817`),
/// runs clean on the arms a frame without an item takes.
#[test]
fn the_inspect_cursor_pair_takes_both_arms() {
    benilla_formats::wow_data_or_skip!();
    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // The in-game UI loads on world entry, so a player always exists by then.
    s.set_unit(
        "player",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Probefour".into()),
            level: 60,
            ..Default::default()
        }),
    );
    assert!(super::load_default_ui(&mut s).is_empty());

    // Both exist as functions, the shape a caller checks before hooking.
    assert_eq!(
        s.eval::<(String, String)>("return type(CursorUpdate), type(CursorOnUpdate)")
            .unwrap(),
        ("function".into(), "function".into())
    );

    // A frame without `this.hasItem` takes the `ResetCursor` arm.
    s.run(
        r#"
        CursorProbe = CreateFrame("Frame", "CursorProbe", UIParent)
        CursorUpdate(CursorProbe)
        "#,
    )
    .unwrap();
    assert!(
        s.errors().is_empty(),
        "the ResetCursor arm runs clean: {:?}",
        s.errors()
    );

    // `CursorOnUpdate` does nothing unless the frame owns the tooltip.
    s.run("CursorOnUpdate(CursorProbe)").unwrap();
    assert!(
        s.errors().is_empty(),
        "the unowned-tooltip gate short-circuits: {:?}",
        s.errors()
    );
}

/// Nothing of the interface stays visible through a cinematic, swept over what is visible rather
/// than a list. The one survivor is `CinematicFrame`, which the reference declares with no parent
/// and `SetFullScreenFrame` shows as it hides `UIParent` (`UIParent.lua:852`).
#[test]
fn a_cinematic_leaves_nothing_of_the_interface_on_screen() {
    benilla_formats::wow_data_or_skip!();
    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // The in-game UI loads on world entry, so a player always exists by then.
    s.set_unit(
        "player",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Probefour".into()),
            level: 60,
            ..Default::default()
        }),
    );
    let failures = super::load_default_ui(&mut s);
    assert!(failures.is_empty(), "manifest load errors: {failures:#?}");
    s.resolve();

    // Our files' frames plus the stock HUD, which is most of what the cascade hides.
    let mut names = shipped_frame_names();
    names.extend(
        [
            "MainMenuBar",
            "ChatFrame1",
            "PlayerFrame",
            "MinimapCluster",
            "BuffFrame",
            "MainMenuBarBackpackButton",
            "CharacterMicroButton",
            "UIErrorsFrame",
            // The frame being shown, the one expected survivor.
            "CinematicFrame",
        ]
        .into_iter()
        .map(String::from),
    );
    let visible = |s: &benilla_ui::script::UiScript, n: &str| -> bool {
        s.eval::<i64>(&format!(
            "local f = getglobal(\"{n}\") \
             if not f or not f.IsVisible then return 0 end \
             return f:IsVisible() and 1 or 0"
        ))
        .unwrap_or(0)
            == 1
    };

    // The sweep is worth something only if there was something to hide.
    let before = names.iter().filter(|n| visible(&s, n)).count();
    assert!(
        // A floor over the named HUD, not over our files' census.
        before >= 6,
        "only {before} frames visible before the cinematic — the sweep found no interface to \
         hide, so it would pass no matter what the cascade did"
    );

    s.set_in_cinematic(true);
    s.fire_event("CINEMATIC_START", vec![]);
    s.resolve();

    let mut after: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|n| visible(&s, n))
        .collect();
    after.sort_unstable();
    assert_eq!(
        after,
        ["CinematicFrame"],
        "something is drawing over the fly-by (see this test's header for why exactly these \
         two are allowed to survive)"
    );

    // …and the player gets it all back.
    s.set_in_cinematic(false);
    s.fire_event("CINEMATIC_STOP", vec![]);
    s.resolve();
    let restored = names.iter().filter(|n| visible(&s, n)).count();
    assert_eq!(
        restored, before,
        "the interface comes back exactly as it was"
    );
}

/// Every named, non-virtual frame the shipped tree declares.
pub(super) fn shipped_frame_names() -> Vec<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/ui");
    let mut names: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("assets/ui").flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e == "xml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read");
        for chunk in text.split('<').skip(1) {
            let head = &chunk[..chunk.find('>').unwrap_or(chunk.len())];
            if head.contains("virtual=\"true\"") {
                continue;
            }
            let Some(i) = head.find("name=\"") else {
                continue;
            };
            let rest = &head[i + 6..];
            let Some(j) = rest.find('"') else { continue };
            let n = &rest[..j];
            // `$parent`-templated names are not globals; nothing can look them up by name.
            if !n.is_empty() && !n.contains('$') {
                names.push(n.to_string());
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// The macro icon picker opens off the shipped manifest, not a harness list: the on-demand macro
/// window's `MacroPopupScrollFrame` inherits a template only the manifest's
/// `ClassTrainerFrameTemplates.xml` row brings.
#[test]
fn the_shipped_manifest_opens_the_macro_icon_picker() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    s.set_unit(
        "player",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Probefive".into()),
            level: 60,
            ..Default::default()
        }),
    );
    // One saved macro, so "Change Name/Icon" has something selected.
    s.set_macros(benilla_ui::script::MacroState {
        account: vec![benilla_ui::script::MacroView {
            name: "die".into(),
            texture: Some(r"Interface\Icons\Ability_Ambush".into()),
            body: ".die".into(),
            ..Default::default()
        }],
        character: Vec::new(),
    });
    s.set_macro_icons(vec![
        r"Interface\Icons\Ability_Ambush".into(),
        r"Interface\Icons\Ability_Backstab".into(),
    ]);
    let failures = super::load_default_ui(&mut s);
    assert!(failures.is_empty(), "manifest load errors: {failures:#?}");
    s.resolve();
    let _ = s.errors();

    // A LoadOnDemand addon, which `ShowMacroFrame` loads through the stock `MacroFrame_LoadUI`.
    super::test_ui::seat_chain_addon(&mut s, "Blizzard_MacroUI");
    s.run("ShowMacroFrame()").unwrap();
    s.run("MacroButton1:Click()").unwrap();
    s.run("MacroEditButton:Click()").unwrap();
    assert!(
        s.errors().is_empty(),
        "opening the icon picker off the shipped manifest must not raise: {:?}",
        s.errors()
    );
    assert!(
        s.eval::<bool>("return MacroPopupFrame:IsShown() and true or false")
            .unwrap(),
        "the picker is up"
    );
}

/// The video window's slider walk finds none of these ten names: `OptionsFrame.lua:110` (`_Load`)
/// and `:208-209` (`_Save`) call `getglobal("Get"..value.func)` and its `Set` twin, falling back to
/// the CVar on a miss; eight of the eighteen composed names are real. The reference's `GetFarclip`
/// (`0x488f00`) and `SetFarclip` (`0x488f30`) miss the row's `"farclip"` only because `getglobal`
/// is case-sensitive.
#[test]
fn the_video_windows_ten_composed_names_stay_nil() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    let failures = super::load_default_ui(&mut s);
    assert!(failures.is_empty(), "manifest load errors: {failures:#?}");

    for name in [
        "Getuiscale",
        "Setuiscale",
        "Getfarclip",
        "Setfarclip",
        "Getanisotropic",
        "Setanisotropic",
        "GetspellEffectLevel",
        "SetspellEffectLevel",
        "GetweatherDensity",
        "SetweatherDensity",
    ] {
        assert!(
            s.eval::<bool>(&format!("return getglobal({name:?}) == nil"))
                .unwrap(),
            "{name} must not exist — `OptionsFrame_Load` branches on it and would stop using the \
             CVar path for that slider"
        );
    }
    // Case-sensitivity, against a name that is defined (Environment Detail's getter).
    assert!(
        s.eval::<bool>("return getglobal(\"GetWorldDetail\") ~= nil")
            .unwrap(),
        "the control: this one is real"
    );
    assert!(
        s.eval::<bool>("return getglobal(\"Getworlddetail\") == nil")
            .unwrap(),
        "`getglobal` must stay case-sensitive — case-folding it would resolve `Getfarclip` to the \
         reference's `GetFarclip` and silently change which path the far-clip slider takes"
    );
}
