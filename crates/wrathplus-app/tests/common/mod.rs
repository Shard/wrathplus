//! The integration tests' interface loader: integration tests cannot reach
//! `ui_script::test_ui::load_ui`, which is `#[cfg(test)]`. A bare filename is a file we ship
//! under `assets/ui`, a path is the reference's own off the installed chain, and `<Script file>`
//! includes resolve through the same provider. A test that names a chain entry opens with
//! `wrathplus_formats::wow_data_or_skip!()`. A kit's first stock `FrameXML` file seats the whole
//! session interface, as `test_ui` does ([`seat_session_ui`]); the rest are already there.

use wrathplus_ui::script::UiScript;

/// Load one manifest entry into `script`, panicking on any loader error. A `.lua` entry runs as a
/// chunk of raw bytes, as `GlobalStrings.lua` does in the real manifest; only XML is decoded.
pub fn load_ui(script: &mut UiScript, entry: &str) {
    if production_rank(entry).is_some() {
        seat_session_ui(script);
        return;
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/ui");
    let chain = |req: &str| -> Option<Vec<u8>> {
        let data = wrathplus_formats::wow_data()?;
        wrathplus_formats::open_chain(&data).ok()?.read(req).ok()
    };
    let read = |req: &str| -> Option<Vec<u8>> {
        if req.contains('\\') || req.contains('/') {
            return chain(req);
        }
        std::fs::read(dir.join(req)).ok()
    };

    let bytes = read(entry).unwrap_or_else(|| panic!("{entry}: not found"));
    if entry.to_ascii_lowercase().ends_with(".lua") {
        script
            .run_chunk_named(&bytes, &format!("@{entry}"))
            .unwrap_or_else(|e| panic!("{entry}: {e}"));
        return;
    }
    let doc = wrathplus_ui::framexml::parse(&wrathplus_ui::source::decode(&bytes))
        .unwrap_or_else(|e| panic!("{entry}: {e}"));
    let report = wrathplus_ui::loader::load_in(script, &doc, &entry.replace('\\', "/"), &read);
    assert!(
        report.errors.is_empty(),
        "{entry}: loader errors: {:#?}",
        report.errors
    );
}

thread_local! {
    /// The VM sessions whose whole stock interface [`seat_session_ui`] has loaded.
    static SEATED: std::cell::RefCell<std::collections::HashSet<u64>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// The stock interface as a session loads it (`wrathplus_app::load_default_ui`: the installed
/// `FrameXML.toc` whole, the archive's startup addon and the bootstrap, on a VM seeded with its
/// display mode and player), once per VM; the copy of `ui_script::test_ui::seat_session_ui`.
fn seat_session_ui(script: &mut UiScript) {
    if SEATED.with(|seated| seated.borrow().contains(&script.session())) {
        return;
    }
    let failures = wrathplus_app::load_default_ui(script);
    assert!(
        failures.is_empty(),
        "the session interface failed to load: {failures:#?}"
    );
    SEATED.with(|seated| seated.borrow_mut().insert(script.session()));
}

/// Where `entry` falls in the production load: a `FrameXML.toc` row at its row, a file a row pulls
/// in (`<Include>` or `<Script file=>`, at any depth) just above the first row that does, and
/// `None` for anything else (a fixture, an on-demand addon's file); `Some` marks a stock file for
/// [`seat_session_ui`]. The copy of `ui_script::test_ui::production_rank`.
fn production_rank(entry: &str) -> Option<usize> {
    static RANKS: std::sync::OnceLock<std::collections::HashMap<String, usize>> =
        std::sync::OnceLock::new();
    let ranks = RANKS.get_or_init(|| {
        let key = |path: &str| path.replace('/', "\\").to_ascii_lowercase();
        let mut ranks = std::collections::HashMap::new();
        let Some(chain) = wrathplus_formats::wow_data()
            .and_then(|data| wrathplus_formats::open_chain(&data).ok())
        else {
            return ranks;
        };
        let read = |path: &str| chain.read(path).ok();
        let toc = read(r"Interface\FrameXML\FrameXML.toc").unwrap_or_default();
        let rows: Vec<String> = wrathplus_ui::toc::Toc::parse(&wrathplus_ui::source::decode(&toc))
            .files
            .iter()
            .map(|f| format!(r"Interface\FrameXML\{f}"))
            .collect();
        for (i, row) in rows.iter().enumerate() {
            ranks.insert(key(row), 2 * i + 1);
        }
        for (i, row) in rows.iter().enumerate() {
            let mut queue = vec![row.clone()];
            while let Some(file) = queue.pop() {
                let Some(bytes) = read(&file) else { continue };
                let text = String::from_utf8_lossy(&bytes);
                for chunk in text.split('<').skip(1) {
                    let tag = chunk.split('>').next().unwrap_or("").trim_start();
                    if !tag.starts_with("Include") && !tag.starts_with("Script") {
                        continue;
                    }
                    let Some(name) = tag
                        .split("file=\"")
                        .nth(1)
                        .and_then(|r| r.split('"').next())
                    else {
                        continue;
                    };
                    let path = format!(r"Interface\FrameXML\{name}");
                    if let std::collections::hash_map::Entry::Vacant(slot) = ranks.entry(key(&path))
                    {
                        slot.insert(2 * i);
                        queue.push(path);
                    }
                }
            }
        }
        ranks
    });
    ranks
        .get(&entry.replace('/', "\\").to_ascii_lowercase())
        .copied()
}
