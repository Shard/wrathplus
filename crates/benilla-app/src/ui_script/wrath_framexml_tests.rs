//! The installed build-12340 FrameXML, loaded whole into a trusted Wrath VM.

use benilla_ui::script::{ScriptDialect, UiScript, UnitState};

/// Every distinct failure, error and warning, with a count, printed for triage.
fn census(script: &mut UiScript, failures: &[String]) -> usize {
    let mut seen: Vec<(String, usize)> = Vec::new();
    let mut note = |line: String| match seen.iter_mut().find(|(l, _)| *l == line) {
        Some((_, n)) => *n += 1,
        None => seen.push((line, 1)),
    };
    failures.iter().for_each(|f| note(format!("FAIL  {f}")));
    script.take_errors().into_iter().for_each(|e| note(format!("ERROR {e}")));
    script.warnings().into_iter().for_each(|w| note(format!("WARN  {w}")));
    for (line, n) in &seen {
        eprintln!("[{n:>3}] {line}");
    }
    seen.iter().filter(|(l, _)| !l.starts_with("WARN")).count()
}

#[test]
#[ignore = "reads the installed Wrath client; run with WOW_CLIENT_PROFILE=wrath WOW_DATA=…"]
fn installed_wrath_framexml_builds_the_stock_hud() {
    let _ = benilla_formats::wow_data_or_skip!();
    let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    // A live VM receives its display modes before FrameXML; UIParent uses their aspect ratio.
    script.set_screen_size(1600.0, 1000.0);
    script.set_screen_resolutions(vec![], Some(benilla_ui::script::ScreenResolution {
        width: 1600, height: 1000,
    }));
    script.set_unit(
        "player",
        Some(UnitState {
            exists: true,
            name: Some("Benchone".into()),
            level: 1,
            health: 60,
            max_health: 60,
            ..Default::default()
        }),
    );
    eprintln!(
        "default bindings: {} rows",
        super::manifest::default_bindings().len()
    );
    // As the game loads it: the server's addon reply (nothing hidden), the load budget armed and
    // the archive's Blizzard addons registered.
    script.note_addon_info_reply(&[]);
    let failures = super::manifest::load_wrath_ui(&mut script, None, &[], true);
    for event in ["VARIABLES_LOADED", "PLAYER_LOGIN", "PLAYER_ENTERING_WORLD"] {
        script.fire_event(event, vec![]);
    }
    script.set_screen_size(1600.0, 1000.0);
    let _ = script.run("LoadAddOn('Blizzard_TimeManager')");
    let started = std::time::Instant::now();
    script.tick(0.1);
    eprintln!("first tick: {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
    for w in script.warnings().iter().filter(|w| w.contains("converge")) {
        eprintln!("CYCLE {w}");
    }
    let hard = census(&mut script, &failures);
    let toc = super::reference_ui::read(r"Interface\AddOns\Blizzard_CombatLog\Blizzard_CombatLog.toc");
    eprintln!(
        "Blizzard_CombatLog toc: {:?}",
        toc.map(|b| String::from_utf8_lossy(&b).chars().take(300).collect::<String>())
    );
    let combat_log = script
        .eval::<String>("local ok, why = LoadAddOn('Blizzard_CombatLog') return tostring(ok) .. ' ' .. tostring(why) .. ' ' .. tostring(GetNumAddOns())")
        .unwrap_or_else(|e| e.to_string());
    eprintln!("Blizzard_CombatLog loads: {combat_log}");
    script.set_screen_size(1600.0, 1000.0);
    for i in 1..=24 {
        let started = std::time::Instant::now();
        let row = script
            .eval::<String>(&format!(
                "local name = GetAddOnInfo({i}) local ok, why = LoadAddOn({i}) \
                 return tostring(name) .. ' ' .. tostring(ok) .. ' ' .. tostring(why)"
            ))
            .unwrap_or_else(|e| e.to_string());
        eprintln!("LOAD {:>8.1} ms  {row}", started.elapsed().as_secs_f64() * 1000.0);
    }
    missing_surface(&script);
    let shown = script
        .eval::<bool>("return PlayerFrame ~= nil and PlayerFrame:IsShown() and TargetFrame ~= nil")
        .unwrap_or(false);
    eprintln!("hard failures: {hard}; PlayerFrame shown with TargetFrame built: {shown}");
    assert!(shown);
    assert!(!failures.iter().any(|f| f.contains("instruction budget")));
}

/// Every `:Method(` and capitalised global `Name(` the installed FrameXML source calls that this
/// VM lacks after the load: the remaining API surface, in one list.
fn missing_surface(script: &UiScript) {
    let core = super::reference_ui::core().expect("FrameXML.toc");
    let mut sources = Vec::new();
    let mut queue: Vec<String> = core.toc.files.clone();
    if let Some(dir) = std::env::var_os("AZB_FRAMEXML_DUMP") {
        for table in [
            "Languages", "ItemBagFamily", "TaxiNodes", "GMTicketCategory", "EmotesTextData",
            "GroundEffectDoodad", "GroundEffectTexture", "SpellVisualKit", "SpellVisual",
        ] {
            if let Some(bytes) = super::reference_ui::read(&format!(r"DBFilesClient\{table}.dbc")) {
                let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{table}.dbc")), bytes);
            }
        }
        for (path, file) in [
            (r"Creature\Kobold\Kobold.m2", "Kobold.m2"),
            (r"World\Generic\Orc\Passive Doodads\Braziers\SmallBrazierPurple01.m2", "Brazier.m2"),
            (r"World\Azeroth\Elwynn\PassiveDoodads\Campfire\ElwynnCampfire.m2", "Campfire.m2"),
        ] {
            if let Some(bytes) = super::reference_ui::read(path) {
                let _ = std::fs::write(std::path::Path::new(&dir).join(file), bytes);
            }
        }
        for name in ["Blizzard_TimeManager"] {
            let dir = format!(r"Interface\AddOns\{name}");
            if let Some(toc) = super::reference_ui::read(&format!(r"{dir}\{name}.toc")) {
                let toc = benilla_ui::toc::Toc::parse(&benilla_ui::source::decode(&toc));
                queue.extend(toc.files.iter().map(|f| format!(r"{dir}\{f}")));
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    while let Some(path) = queue.pop() {
        if !seen.insert(path.to_ascii_lowercase()) {
            continue;
        }
        let Some(bytes) = super::reference_ui::read(&path) else { continue };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if let Some(dir) = std::env::var_os("AZB_FRAMEXML_DUMP") {
            let leaf = path.rsplit('\\').next().unwrap_or(&path);
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(std::path::Path::new(&dir).join(leaf), &bytes);
        }
        if path.to_ascii_lowercase().ends_with(".xml") {
            let dir = path.rsplit_once('\\').map_or("", |(d, _)| d).to_string();
            for part in text.split("file=\"").skip(1) {
                if let Some(file) = part.split('"').next() {
                    queue.push(format!("{dir}\\{file}"));
                }
            }
        }
        sources.push(text);
    }
    script.clear_instruction_budget();
    let has: Vec<String> = script
        .lua()
        .load(
            r##"
            local kinds = { "Frame", "Button", "CheckButton", "EditBox", "StatusBar", "Slider",
                "ScrollFrame", "MessageFrame", "ScrollingMessageFrame", "GameTooltip", "Model",
                "PlayerModel", "DressUpModel", "Minimap", "ColorSelect", "SimpleHTML", "Cooldown" }
            local has, out = {}, {}
            for _, kind in ipairs(kinds) do
                local ok, frame = pcall(CreateFrame, kind)
                if ok and frame then
                    for _, object in ipairs({ frame, frame:CreateTexture(), frame:CreateFontString() }) do
                        local mt = getmetatable(object)
                        local index = mt and mt.__index
                        while type(index) == "table" do
                            for name in pairs(index) do has[name] = true end
                            local up = getmetatable(index)
                            index = up and up.__index
                        end
                    end
                else
                    out[#out + 1] = "KIND:" .. kind
                end
            end
            for name in pairs(has) do out[#out + 1] = name end
            return out
            "##,
        )
        .call(())
        .unwrap();
    let has: std::collections::HashSet<String> = has.into_iter().collect();
    for kind in has.iter().filter(|n| n.starts_with("KIND:")) {
        eprintln!("MISSING {kind}");
    }
    let mut defined = std::collections::HashSet::new();
    let mut methods = std::collections::BTreeMap::<String, usize>::new();
    let mut calls = std::collections::BTreeMap::<String, usize>::new();
    for src in &sources {
        let bytes = src.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i];
            if c.is_ascii_alphabetic() || c == b'_' {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let name = &src[start..i];
                let mut j = i;
                while j < bytes.len() && bytes[j] == b' ' {
                    j += 1;
                }
                let before = if start > 0 { bytes[start - 1] } else { b' ' };
                let preceded_by_function = src[..start].trim_end().ends_with("function");
                if j < bytes.len() && bytes[j] == b'(' && name.as_bytes()[0].is_ascii_uppercase() {
                    if preceded_by_function {
                        defined.insert(name.to_string());
                    } else if before == b':' {
                        *methods.entry(name.to_string()).or_default() += 1;
                    } else if before != b'.' && before != b'"' && before != b'\'' {
                        *calls.entry(name.to_string()).or_default() += 1;
                    }
                }
            } else {
                i += 1;
            }
        }
    }
    // ALL_CAPS globals read but never assigned in the source: engine-provided constants.
    let mut constants = std::collections::BTreeMap::<String, usize>::new();
    let mut assigned = std::collections::HashSet::new();
    for src in &sources {
        let bytes = src.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i].is_ascii_uppercase()
                && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || b"_.:\"'".contains(&bytes[i - 1])))
            {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_uppercase() || bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                    i += 1;
                }
                if i < bytes.len() && (bytes[i].is_ascii_lowercase() || bytes[i] == b'(') {
                    continue;
                }
                let name = &src[start..i];
                if name.len() < 4 || !name.contains('_') {
                    continue;
                }
                let rest = src[i..].trim_start();
                if rest.starts_with('=') && !rest.starts_with("==") {
                    assigned.insert(name.to_string());
                } else {
                    *constants.entry(name.to_string()).or_default() += 1;
                }
            } else {
                i += 1;
            }
        }
    }
    let unassigned: Vec<String> = constants.keys().filter(|n| !assigned.contains(*n)).cloned().collect();
    let absent_constants: Vec<String> = script
        .lua()
        .load("local out = {} for _, n in ipairs(...) do if _G[n] == nil then out[#out+1] = n end end return out")
        .call(unassigned)
        .unwrap();
    for name in absent_constants {
        eprintln!("CONST {name} {}", constants[&name]);
    }
    let names: Vec<String> = calls.keys().filter(|n| !defined.contains(*n)).cloned().collect();
    let absent: Vec<String> = script
        .lua()
        .load("local out = {} for _, n in ipairs(...) do if _G[n] == nil then out[#out+1] = n end end return out")
        .call(names)
        .unwrap();
    let mut lines = Vec::new();
    for (name, n) in &methods {
        if !has.contains(name) {
            lines.push(format!("METHOD {name} {n}"));
        }
    }
    for name in absent {
        lines.push(format!("GLOBAL {name} {}", calls[&name]));
    }
    eprintln!("missing surface: {} names", lines.len());
    for line in lines {
        eprintln!("{line}");
    }
}
