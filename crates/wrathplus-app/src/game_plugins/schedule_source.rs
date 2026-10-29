//! Source identities for the schedule census: use the owning crate, never a bare fn suffix.

use std::collections::HashMap;
use std::path::Path;

fn system_name(file: &Path, rel: &str, name: &str) -> String {
    let src = file
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == "src"))
        .expect("source lives under src/");
    let krate = src
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .expect("src/ has a crate directory")
        .replace('-', "_");
    let module = rel.trim_end_matches(".rs").trim_end_matches("/mod");
    if module == "lib" {
        format!("{krate}::{name}")
    } else {
        format!("{krate}::{}::{name}", module.replace('/', "::"))
    }
}

pub(super) fn lookup<'a, T>(
    by_name: &HashMap<&str, &'a T>,
    file: &Path,
    rel: &str,
    name: &str,
) -> Option<&'a T> {
    by_name.get(system_name(file, rel, name).as_str()).copied()
}

#[cfg(test)]
mod schedule_tests {
    use super::*;

    #[test]
    fn source_lookup_keeps_crate_and_module_even_for_repeated_function_names() {
        let cases = [
            ("wrathplus-app-world", "spell/unit_cast.rs", "feed"),
            ("wrathplus-app-ui", "ui_arena_team.rs", "feed_arena"),
            ("wrathplus-app-ui", "ui_gm_response.rs", "feed"),
            ("wrathplus-app-ui", "ui_lfg/l1.rs", "feed"),
            ("wrathplus-app-ui", "ui_loot_roll/wrath/feed.rs", "feed"),
            ("wrathplus-app-ui", "ui_native/wrath_arena.rs", "feed_arena"),
            ("wrathplus-app", "lib.rs", "feed"),
            ("wrathplus-app-core", "target/mod.rs", "feed"),
        ];
        // Expected names are independent of system_name, including two different feed_arena's.
        let names = [
            "wrathplus_app_world::spell::unit_cast::feed",
            "wrathplus_app_ui::ui_arena_team::feed_arena",
            "wrathplus_app_ui::ui_gm_response::feed",
            "wrathplus_app_ui::ui_lfg::l1::feed",
            "wrathplus_app_ui::ui_loot_roll::wrath::feed::feed",
            "wrathplus_app_ui::ui_native::wrath_arena::feed_arena",
            "wrathplus_app::feed",
            "wrathplus_app_core::target::feed",
        ];
        let values: Vec<_> = (0..names.len()).collect();
        let mut index: HashMap<_, _> = names.iter().copied().zip(values.iter()).collect();
        for (i, (krate, rel, name)) in cases.iter().enumerate() {
            let file = format!("/checkout/crates/{krate}/src/{rel}");
            assert_eq!(lookup(&index, Path::new(&file), rel, name), Some(&i));
        }
        index.remove(names[0]);
        assert_eq!(
            lookup(
                &index,
                Path::new("/checkout/crates/wrathplus-app-world/src/spell/unit_cast.rs"),
                "spell/unit_cast.rs",
                "feed"
            ),
            None,
            "a different module's feed is not evidence for this consumer"
        );
        let only_wrong_crate =
            HashMap::from([("wrathplus_app::spell::unit_cast::feed", &values[0])]);
        assert_eq!(
            lookup(
                &only_wrong_crate,
                Path::new("/checkout/crates/wrathplus-app-world/src/spell/unit_cast.rs"),
                "spell/unit_cast.rs",
                "feed"
            ),
            None,
            "even a unique bare name or matching module in another crate must not resolve"
        );
    }
}
