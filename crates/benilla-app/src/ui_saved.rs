//! The saved-variables host side: the file and the load seam. The declaration set and
//! the serializer are [`benilla_ui::script`]'s.
//!
//! The file runs after the in-game UI's own files and before `VARIABLES_LOADED`, the order of the
//! reference's `AddOn_Load` (`0x51f240`), so a saved value overrides a file-scope default before
//! any consumer reads it. The 1.12 single-file writer went with the 1.12 UI load (Vanilla removal
//! step 4b); the Wrath shutdown writes per-addon files (`addons::save_addon_variables`).

use bevy::prelude::*;

use benilla_ui::script::UiScript;

pub(crate) struct UiSavedPlugin;

impl Plugin for UiSavedPlugin {
    fn build(&self, _app: &mut App) {
        // No systems: the load is one ordered step of the in-game UI load.
    }
}

/// Run the saved-variables file into the VM, then `host_settings`, then fire `VARIABLES_LOADED`.
/// A malformed or unreadable file warns and is held ([`UiScript::hold_saved_file`]), so the
/// shutdown write leaves it alone. Read as bytes: the writer keeps a Lua string's raw bytes.
///
/// `host_settings` seats what the host carries into a `RegisterForSave` global (a plate setting
/// left in `config.toml`): after the chunk, so it outranks the file, and before the event, where
/// the consumers run (`UIParent_OnEvent` → `UpdateNameplates`).
pub(crate) fn load_saved_variables(
    script: &mut UiScript,
    host_settings: impl FnOnce(&mut UiScript),
) {
    // `None` in a capture or with no install: nothing is read, and the event still fires.
    if let Some(path) = crate::local_state::saved_variables_path() {
        match std::fs::read(&path) {
            Ok(bytes) => {
                if let Err(e) = script.run_chunk(&bytes) {
                    warn!(
                        "saved variables: {} did not load ({e}) — running on defaults, and \
                         leaving the file as it is",
                        path.display()
                    );
                    script.hold_saved_file(&path);
                } else {
                    info!("saved variables: loaded {}", path.display());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                warn!("saved variables: cannot read {}: {e}", path.display());
                script.hold_saved_file(&path);
            }
        }
    }
    host_settings(script);
    // Fired whether or not there was a file, as a step of the reference's load sequence.
    script.fire_event("VARIABLES_LOADED", vec![]);
}
