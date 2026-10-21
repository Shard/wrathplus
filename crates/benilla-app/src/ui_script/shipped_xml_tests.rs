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

/// The stock pet bar with the hunter fixture (Claw autocasting), settled past its slide.
fn pet_bar_vm() -> benilla_ui::script::UiScript {
    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    super::pet_bar_tests::load_pet_bar(&mut s);
    super::pet_bar_tests::declare_token_strings(&s);
    s.set_pet_actions(true, true, true, super::pet_bar_tests::hunter_slots());
    s.fire_event("PET_BAR_UPDATE", vec![]);
    for _ in 0..3 {
        s.tick(0.05);
    }
    s.resolve();
    assert!(s.errors().is_empty(), "pet bar errors: {:?}", s.errors());
    s
}

/// The autocast brackets land on each button's corners: the art fills the middle 33/64 of
/// `UI-AutoCastableOverlay`, which puts the stock pet button's on its corners (58 × 33/64 = 29.9
/// on 30). Deviation: the spell book takes that ratio (`SpellBookAdapters.xml`), because its stock
/// brackets (60 × 33/64 = 30.9 on 37) sit three units inside each edge.
#[test]
fn the_autocast_brackets_reach_each_buttons_corners() {
    benilla_formats::wow_data_or_skip!();
    use benilla_ui::framexml::{Element, TopLevel};

    /// The bracket art's share of `UI-AutoCastableOverlay.blp`, texels 15..47 of 64, measured with
    /// `benilla-extract blp`.
    const ART: f32 = 33.0 / 64.0;

    fn dim(el: &Element, tag: &str, axis: &str) -> Option<f32> {
        el.children
            .iter()
            .find(|c| c.tag.eq_ignore_ascii_case(tag))
            .and_then(|n| n.children.first())
            .and_then(|d| {
                d.attrs()
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(axis))
                    .map(|(_, v)| v.clone())
            })
            .and_then(|v| v.parse().ok())
    }

    /// Walk a template, carrying the nearest enclosing button size down to the overlay.
    fn walk(el: &Element, button: Option<f32>, out: &mut Vec<(String, f32, f32)>) {
        let button = dim(el, "Size", "x")
            .filter(|_| el.tag.ends_with("Button"))
            .or(button);
        let name = el
            .attrs()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("name"))
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        if name.ends_with("AutoCastable") {
            if let (Some(size), Some(btn)) = (dim(el, "Size", "x"), button) {
                out.push((name.clone(), size * ART, btn));
            }
        }
        for child in &el.children {
            walk(child, button, out);
        }
    }

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/ui");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("assets/ui") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "xml") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read");
        for item in benilla_ui::framexml::parse(&src).expect("parse").items {
            match item {
                TopLevel::Font(el) | TopLevel::Template(el) | TopLevel::Instance(el) => {
                    walk(&el, None, &mut found)
                }
                TopLevel::Include(_) | TopLevel::Script(_) => {}
            }
        }
    }
    // The spell book's stock overlay is resized by a script, so it is measured off a VM.
    let s = super::spellbook_tests::spellbook_ui(1024.0, 768.0);
    let (overlay, button): (f32, f32) = s
        .eval("return SpellButton1AutoCastable:GetWidth(), SpellButton1:GetWidth()")
        .unwrap();
    found.push(("SpellButton1AutoCastable".into(), overlay * ART, button));
    // The pet button's is the stock template, measured the same way.
    let s = pet_bar_vm();
    let (overlay, button): (f32, f32) = s
        .eval("return PetActionButton1AutoCastable:GetWidth(), PetActionButton1:GetWidth()")
        .unwrap();
    found.push(("PetActionButton1AutoCastable".into(), overlay * ART, button));
    assert_eq!(found.len(), 2, "expected two autocast overlays: {found:?}");
    for (name, brackets, button) in &found {
        assert!(
            (brackets / button - 0.997).abs() < 0.01,
            "{name}: a {brackets:.1}-unit bracket square on a {button}-unit button is {:.3}x — the \
             pet button's is 0.997x, which is what puts brackets IN the corners",
            brackets / button
        );
    }
}

/// The autocast shine's rim runs on its viewport edge (1.024×), as the stock pet button's does
/// (`setAllPoints` on 30×30, `scale="1.2"`). Deviation: the spell book re-seats the stock model at
/// 1.48 on 37 units (`SpellBookAdapters.xml`), because its stock `scale="1.22"` in 36 units is a
/// 0.87× rim that floats clear of the edge and washes the icon.
#[test]
fn the_shine_panes_ask_for_the_rims_we_meant() {
    benilla_formats::wow_data_or_skip!();
    let mut found: Vec<(String, f32, f32)> = Vec::new();
    let mut s = super::spellbook_tests::spellbook_ui(1024.0, 768.0);
    s.run("ToggleSpellBook(BOOKTYPE_SPELL)").unwrap();
    s.tick(0.05);
    s.resolve();
    let (scale, view): (f32, f32) = s
        .eval("return SpellButton1AutoCast:GetModelScale(), SpellButton1AutoCast:GetWidth()")
        .unwrap();
    found.push(("SpellButton1AutoCast".into(), 0.02 * 1280.0 * scale, view));
    let s = pet_bar_vm();
    let (scale, view): (f32, f32) = s
        .eval(
            "return PetActionButton1AutoCast:GetModelScale(), PetActionButton1AutoCast:GetWidth()",
        )
        .unwrap();
    found.push((
        "PetActionButton1AutoCast".into(),
        0.02 * 1280.0 * scale,
        view,
    ));
    for (name, rim, view) in &found {
        assert!(
            (rim / view - 1.024).abs() < 1e-3,
            "{name}: a {rim}-unit rim in a {view}-unit viewport is {:.3}x, not the pet button's \
             1.024x — a rim that does not reach its own viewport edge is never clipped, and reads \
             as a wash rather than a rim",
            rim / view
        );
    }
}

/// Every shipped `text=` answers against the real `GlobalStrings.lua`: `text=` is a lookup
/// (`FrameScript_GetText`, `0x703bf0`) that shows the raw attribute only on a miss (`0x778c07`).
/// A key-shaped value must resolve, or it shows its own key; a literal must not collide with a
/// global, or the lookup swaps in that string.
#[test]
fn every_shipped_text_attribute_answers_against_the_real_global_strings() {
    let data = benilla_formats::wow_data_or_skip!();
    let mut chain = benilla_formats::open_chain(&data).expect("open chain");
    let src = chain
        .read_file("Interface\\FrameXML\\GlobalStrings.lua")
        .expect("GlobalStrings.lua in the chain");
    let s = benilla_ui::script::UiScript::new().expect("VM");
    s.run(&String::from_utf8_lossy(&src)).expect("runs clean");

    // The loader's key-shape predicate, restated because it is private.
    let key_shaped = |v: &str| {
        v.len() >= 2
            && v.chars().any(|c| c.is_ascii_uppercase())
            && v.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    };

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/ui");
    let mut swept = 0;
    for entry in std::fs::read_dir(&dir).expect("assets/ui").flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "xml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read");
        let file = path.file_name().unwrap_or_default().to_string_lossy();
        for value in text
            .match_indices("text=\"")
            .filter_map(|(i, m)| text[i + m.len()..].split('"').next())
        {
            let resolved = s.lua().globals().get::<String>(value).ok();
            if key_shaped(value) {
                assert!(
                    resolved.is_some_and(|t| !t.is_empty()),
                    "{file}: text=\"{value}\" is shaped like a GlobalStrings key but the real \
                     GlobalStrings.lua has no such string — it would render as its own key name"
                );
            } else {
                assert!(
                    resolved.is_none(),
                    "{file}: the literal text=\"{value}\" collides with a real GlobalStrings key — \
                     the loader would silently show that string's value instead of these words"
                );
            }
            swept += 1;
        }
    }
    // A floor, so the sweep cannot pass by matching nothing.
    assert!(swept >= 1, "only {swept} text= values swept");
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
                ..benilla_ui::script::UnitState::default()
            }),
        );
    }
    s.fire_event("PLAYER_ENTERING_WORLD", vec![]);
    s.fire_event("PARTY_MEMBERS_CHANGED", vec![]);
    // Target-of-target ships off (`SHOW_TARGET_OF_TARGET = "0"`, `UIOptionsFrame.lua:116`); it
    // holds the second of the reference's two named TextureFrames.
    s.run(r#"SHOW_TARGET_OF_TARGET = "1""#).unwrap();
    s.run("this = TargetofTargetFrame TargetofTarget_Update() this = nil")
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
    // The reference names two TextureFrames, `TargetFrameTextureFrame` and
    // `TargetofTargetTextureFrame` (TargetFrame.xml); the player, pet and party frames nest their
    // art in anonymous frames instead: two families with two bars each.
    assert!(
        checked >= 4,
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

/// `TargetBuffButtonTemplate` (`TargetFrame.xml`, 21×21) and `MainMenuBarMicroButton`
/// (`MainMenuBarMicroButtons.xml`, 29×58) confer their size, regions and scripts, read back as a
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
        BuffProbe = CreateFrame("Button", "BuffProbe", UIParent, "TargetBuffButtonTemplate")
        MicroProbe = CreateFrame("Button", "MicroProbe", UIParent, "MainMenuBarMicroButton")
        "#,
    )
    .unwrap();
    s.resolve();

    assert_eq!(
        s.eval::<(f64, f64)>("return BuffProbe:GetWidth(), BuffProbe:GetHeight()")
            .unwrap(),
        (21.0, 21.0),
        "the buff button carries the reference's 21x21"
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
        (29.0, 58.0)
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
        this = CursorProbe
        CursorUpdate()
        this = nil
        "#,
    )
    .unwrap();
    assert!(
        s.errors().is_empty(),
        "the ResetCursor arm runs clean: {:?}",
        s.errors()
    );

    // `CursorOnUpdate` does nothing unless the frame owns the tooltip.
    s.run("this = CursorProbe CursorOnUpdate() this = nil")
        .unwrap();
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

/// Every `parent=` the shipped tree declares attaches, read off the loaded tree: a parent name that
/// names nothing only warns, and the loader falls back to the enclosing element.
#[test]
fn every_declared_parent_really_attaches() {
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

    let declared = shipped_frame_parents();
    assert!(
        // A sanity floor for the scan, not a census.
        !declared.is_empty(),
        "only {} parent declarations found — the scan broke",
        declared.len()
    );
    let mut wrong: Vec<String> = Vec::new();
    for (child, parent) in &declared {
        let got = s
            .eval::<String>(&format!(
                "local f = getglobal(\"{child}\") \
                 if not f or not f.GetParent then return \"<not a frame>\" end \
                 local p = f:GetParent() \
                 return p and (p:GetName() or \"<anonymous>\") or \"<none>\""
            ))
            .unwrap_or_else(|_| "<error>".to_string());
        if got != *parent {
            wrong.push(format!(
                "  {child}: declared parent={parent}, attached to {got}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} frame(s) did not attach to the parent they declare:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// Every named, non-virtual frame in the shipped tree that declares a `parent=`, with that parent.
fn shipped_frame_parents() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/ui");
    let mut out: Vec<(String, String)> = Vec::new();
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
            let attr = |key: &str| -> Option<String> {
                let i = head.find(key)?;
                let rest = &head[i + key.len()..];
                let j = rest.find('"')?;
                Some(rest[..j].to_string())
            };
            let (Some(name), Some(parent)) = (attr("name=\""), attr("parent=\"")) else {
                continue;
            };
            if !name.contains('$') && !parent.contains('$') {
                out.push((name, parent));
            }
        }
    }
    out.sort();
    out.dedup();
    out
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

/// The stock Sound window's boxes write `this:GetChecked()` as is (`SoundOptionsFrame.lua:82`),
/// which is nil once unticked, and `SetCVar` stores that as "0" (`0x488c98`); its sliders write
/// `GetValue()`, a single-precision number stored as Lua's `%.14g` text (`0x6f7c80`).
#[test]
fn the_stock_sound_window_turns_a_box_off_and_stores_a_slider_as_lua_text() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
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
    s.set_cvar_host("EnableMusic", "1");
    s.run("this = SoundOptionsFrameOkay SoundOptionsFrame_Load()")
        .unwrap();
    let _ = s.take_cvar_changes();
    let checked = "return SoundOptionsFrameCheckButton5:GetChecked() and true or false";
    assert!(s.eval::<bool>(checked).unwrap(), "Enable Music starts on");

    s.run("SoundOptionsFrameCheckButton5:Click()").unwrap();
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert!(!s.eval::<bool>(checked).unwrap());
    assert_eq!(
        s.take_cvar_changes(),
        vec![("EnableMusic".to_string(), "0".to_string())],
        "the unticked box turns music off"
    );
    s.run("SoundOptionsFrameCheckButton5:Click()").unwrap();
    assert_eq!(
        s.take_cvar_changes(),
        vec![("EnableMusic".to_string(), "1".to_string())]
    );

    s.run("SoundOptionsFrameSlider3:SetValue(0.4)").unwrap();
    assert_eq!(
        s.eval::<String>("return GetCVar(\"MusicVolume\")").unwrap(),
        "0.40000000596046"
    );
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
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

/// `UIOptionsFrame_Load()` and `_Save()` run clean, as pfUI's `modules/gui.lua:146-148` calls them
/// around a checkbox write: every slider CVar `_Load` reads is registered, and `Slider:SetValue`
/// (`0x790980`) raises on nil in the reference too.
#[test]
fn the_stock_options_windows_load_and_save_are_reachable_for_addons() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = benilla_ui::script::UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    s.set_unit(
        "player",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Probefive".into()),
            level: 60,
            // `_Load`'s tail upper-cases `UnitClass("player")` (`UIOptionsFrame.lua:759-763`); a
            // warrior takes the branch that disables the combo-point box.
            class: Some("Warrior".into()),
            class_file: Some("WARRIOR".into()),
            ..Default::default()
        }),
    );
    assert!(super::load_default_ui(&mut s).is_empty());

    // `_Load` first, the window's own order: the reference loads on show and saves on Okay.
    s.run("this = UIOptionsFrameOkay UIOptionsFrame_Load()")
        .expect("_Load runs to completion — every slider CVar it reads is registered");
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());

    // Every slider CVar in the stock table must answer, or `_Load` raises.
    let unbacked: Vec<String> = s
        .eval(
            "local out = {} \
             for _, v in ipairs(UIOptionsFrameSliders) do \
                 if GetCVar(v.cvar) == nil then table.insert(out, v.cvar) end \
             end \
             return out",
        )
        .expect("read UIOptionsFrameSliders");
    assert!(
        unbacked.is_empty(),
        "these slider CVars answer nil, and `_Load` raises on the first of them: {unbacked:?}"
    );

    // The walk reached slider 3, Mouse Look Speed, on `cameraYawMoveSpeed`'s registered 180
    // (`UIOptionsFrame.lua:89`).
    let slider3: f64 = s
        .eval("return UIOptionsFrameSlider3:GetValue()")
        .expect("the Mouse Look Speed slider's value");
    assert!(
        (slider3 - 180.0).abs() < 0.001,
        "slider 3 should carry cameraYawMoveSpeed's registered 180, got {slider3}"
    );

    // `_SetDefaults` is the same walk through `GetCVarDefault`.
    s.run("this = UIOptionsFrameDefaults UIOptionsFrame_SetDefaults()")
        .expect("_SetDefaults runs to completion too");
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());

    // Through the Okay button's own `<OnClick>` (`UIOptionsFrame.xml:1205-1209`), which sets
    // `this`: the `SHOW_PARTY_PETS` arm reaches `RefreshBuffs`, whose first act is
    // `this.hasDispellable = nil` (`BuffFrame.lua:266`).
    s.run("UIOptionsFrameOkay:Click()")
        .expect("the stock Okay button's own handler");
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert!(
        !s.eval::<bool>("return UIOptionsFrame:IsShown()").unwrap(),
        "…and the window it hides on the way out was hidden to begin with"
    );

    // `_Save` writes the yaw slider's value and `cameraPitchMoveSpeed = value / 2`
    // (`UIOptionsFrame.lua:355-356`), as the registered defaults, 180 and 90, agree.
    let (yaw, pitch): (f64, f64) = (
        s.eval(r#"return tonumber(GetCVar("cameraYawMoveSpeed"))"#)
            .expect("yaw move speed"),
        s.eval(r#"return tonumber(GetCVar("cameraPitchMoveSpeed"))"#)
            .expect("pitch move speed"),
    );
    assert!(
        (yaw - 180.0).abs() < 0.001 && (pitch - 90.0).abs() < 0.001,
        "_Save should write the slider's 180 and its half; got yaw={yaw} pitch={pitch}"
    );
}
