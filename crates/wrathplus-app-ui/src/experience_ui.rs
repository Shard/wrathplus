//! Plus UI policy publication, after feeds and before this frame's Lua tick/layout.
use wrathplus_ui::script::UiScript;
use bevy::prelude::*;

use crate::experience::{Experience, Feature};

pub fn sync_text(experience: Res<Experience>, script: Option<NonSend<UiScript>>) {
    let Some(script) = script else { return };
    script.sync_chat_text_minimum(experience.enabled(Feature::ReadableText).then_some(16.0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experience::Profile;

    #[test]
    fn classic_masks_ui_and_plus_switch_restores_engine_size() {
        let mut app = App::new();
        let s = UiScript::new().unwrap();
        s.run("ChatFrame1=CreateFrame('ScrollingMessageFrame','ChatFrame1'); ChatFrame1:SetFont('stock.ttf',12,'')").unwrap();
        app.insert_non_send_resource(s);
        let mut experience = Experience::default();
        experience.set_feature(Feature::ReadableText, true);
        app.insert_resource(experience)
            .add_systems(Update, sync_text);
        let height = |app: &App| {
            app.world()
                .non_send_resource::<UiScript>()
                .eval::<f32>("local _,h=ChatFrame1:GetFont(); return h")
                .unwrap()
        };
        app.update();
        assert_eq!(height(&app), 12.0);
        app.world_mut()
            .resource_mut::<Experience>()
            .set_profile(Profile::Plus);
        app.update();
        assert_eq!(height(&app), 16.0);
        app.world_mut()
            .resource_mut::<Experience>()
            .set_profile(Profile::Classic);
        app.update();
        assert_eq!(height(&app), 12.0);
    }
}
