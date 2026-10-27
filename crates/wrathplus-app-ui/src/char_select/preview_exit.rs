//! The glue stage on the way out of character select ([`clear_preview`]).

use bevy::prelude::*;
use bevy::state::state::StateTransitionEvent;

use crate::app_core::state::ClientState;
use crate::portrait::GluePreview;

/// Leaving select empties the glue stage, except into the create screen. The stock create's
/// `CharacterCreate_OnShow` sets its stage inside the Create click (`SetCharacterRace` →
/// `SetCharCustomizeBackground`), a frame before this exit, and nothing sets it again until the
/// race or class changes: cleared here, the create scene stood black on its first open (Shannon's
/// 10-27 session). The native create's `OnEnter` sets its own, and both creates dress the look.
pub(crate) fn clear_preview(
    mut preview: ResMut<GluePreview>,
    mut transitions: MessageReader<StateTransitionEvent<ClientState>>,
) {
    let into_create = transitions
        .read()
        .last()
        .is_some_and(|t| t.entered == Some(ClientState::CharCreate));
    if !into_create {
        preview.look = None;
        preview.scene = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portrait::GlueScene;

    fn go(app: &mut App, state: ClientState) {
        app.world_mut()
            .resource_mut::<NextState<ClientState>>()
            .set(state);
        app.update();
    }

    /// The Create click: the stock OnShow's stage, set while select is up, stands on the create
    /// screen's first frame with no selection change; the world and the login still empty it.
    #[test]
    fn the_create_screen_opens_on_the_stage_its_on_show_set() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(ClientState::CharSelect)
            .init_resource::<GluePreview>()
            .add_systems(OnExit(ClientState::CharSelect), clear_preview);
        app.update();
        let stage = Some(GlueScene::Stage("DeathKnight"));
        app.world_mut().resource_mut::<GluePreview>().scene = stage;
        go(&mut app, ClientState::CharCreate);
        assert!(
            app.world().resource::<GluePreview>().scene == stage,
            "the create's first frame keeps the stage CharacterCreate_OnShow asked for"
        );

        for out in [ClientState::InWorld, ClientState::Login] {
            go(&mut app, ClientState::CharSelect);
            app.world_mut().resource_mut::<GluePreview>().scene = stage;
            go(&mut app, out);
            assert!(
                app.world().resource::<GluePreview>().scene.is_none(),
                "select → {out:?} empties the stage"
            );
        }
    }
}
