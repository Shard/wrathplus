//! `config.toml`'s `[data]` table: what the archive chain mounts beyond the stock 3.3.5a client.
//!
//! The default is the stock client: Blizzard's archives only. A repack's `patch-4.MPQ`,
//! `patch-enUS-4.MPQ` and on (its GlueXML, art or DBCs) mount only on request, as
//! `nonstock_patches = true` here or `WOW_NONSTOCK_PATCHES=1`, which this table sets for the boot
//! when the environment does not already say. Loose replacement packs are the Experience
//! layer's (`[experience] packs`, `mods/<pack>/`), not this table's.

/// Read `[data] nonstock_patches` and, when it is `true` and the environment is silent, set
/// [`wrathplus_formats::NONSTOCK_PATCHES_ENV`] before any archive chain opens.
pub fn apply_nonstock_patches() {
    let env = wrathplus_formats::NONSTOCK_PATCHES_ENV;
    if std::env::var_os(env).is_some() {
        return;
    }
    let text = crate::local_state::config_path().and_then(|p| std::fs::read_to_string(p).ok());
    if text.as_deref().is_some_and(nonstock_patches) {
        eprintln!("wrathplus: [data] nonstock_patches = true — mounting non-stock patch archives");
        std::env::set_var(env, "1");
    }
}

/// `[data] nonstock_patches` in a `config.toml`'s text; absent or unreadable is `false`.
fn nonstock_patches(text: &str) -> bool {
    text.parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("data")?.get("nonstock_patches")?.as_bool())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::nonstock_patches;

    #[test]
    fn the_data_table_opts_in_and_nothing_else_does() {
        assert!(nonstock_patches("[data]\nnonstock_patches = true\n"));
        assert!(!nonstock_patches("[data]\nnonstock_patches = false\n"));
        assert!(!nonstock_patches("[cvars]\nrealmlist = \"x\"\n"));
        assert!(
            !nonstock_patches("nonstock_patches = true\n"),
            "top level is not [data]"
        );
        assert!(!nonstock_patches("not toml ["));
    }
}
