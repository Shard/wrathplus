//! The world map's `GetDeathReleasePosition`: the release location the wire layer captures
//! (`crate::net::wrath_death_capture`), published into the VM.

use bevy::prelude::*;

use crate::net::wrath_death_capture::DeathRelease;

pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        feed_death_release
            .in_set(crate::ui_script::UiFeed)
            .run_if(crate::ui_script::ingame_ui_up),
    );
}

#[allow(clippy::type_complexity)] // Bevy system parameters / a closure table, kept inline
fn feed_death_release(
    script: Option<NonSendMut<benilla_ui::script::UiScript>>,
    release: Res<DeathRelease>,
    mut last: Local<crate::ui_script::VmMemo<Option<Option<(u32, f32, f32)>>>>,
) {
    let Some(mut script) = script else {
        return;
    };
    let (last, reset) = last.get_reset(&script);
    if reset || *last != Some(release.0) {
        *last = Some(release.0);
        script.set_death_release_location(release.0);
    }
}
