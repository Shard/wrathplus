//! Optional host policy for the ten stock chat windows, never shared Font objects.
//! This changes the engine value used by both Lua GetFont and measurement/painting.
//! Addon SetFont remains immediate; the next host sync adopts its size as the new baseline.

use std::collections::HashMap;

use crate::script::UiScript;
use crate::widget::{FrameKind, RegionHandle, RegionKind};

#[derive(Default)]
struct ChatTextState {
    sizes: HashMap<RegionHandle, ChatSize>,
}

struct ChatSize {
    authored: f32,
    applied: f32,
}

// A write equal to our applied size is still authored intent. Observe successful region
// setters without allocating policy state in Classic, rather than guessing from value changes.
pub(crate) fn authored_height(lua: &mlua::Lua, region: RegionHandle, height: f32) {
    if let Some(mut state) = lua.app_data_mut::<ChatTextState>() {
        if let Some(size) = state.sizes.get_mut(&region) {
            size.authored = height;
            size.applied = height;
        }
    }
}

impl UiScript {
    /// Apply a finite positive minimum to ChatFrame1..10, or restore authored heights.
    /// Returns the number of changed engine sizes. Disabled and never enabled is a true no-op:
    /// no arena walk, lazy region creation, state allocation, or measurement invalidation.
    /// A Lua setter can expose its raw height until the next frame; no render-only scale exists.
    pub fn sync_chat_text_minimum(&self, minimum: Option<f32>) -> usize {
        let minimum = minimum.filter(|v| v.is_finite() && *v > 0.0);
        if minimum.is_none() && self.lua().app_data_ref::<ChatTextState>().is_none() {
            return 0;
        }
        let mut state = self
            .lua()
            .remove_app_data::<ChatTextState>()
            .unwrap_or_default();
        let mut model = self.model_mut();
        let mut changed = 0;
        if let Some(minimum) = minimum {
            // Bounded named lookups only. Never scan the UI arena or touch quest/global fonts.
            let mut live = Vec::with_capacity(10);
            for index in 1..=10 {
                let Some(frame) = model
                    .arena
                    .lookup(&format!("ChatFrame{index}"))
                    .and_then(|h| model.arena.frame(h))
                else {
                    continue;
                };
                if frame.kind != FrameKind::ScrollingMessageFrame {
                    continue;
                }
                let Some(region) = frame.regions.iter().copied().find(|h| {
                    model
                        .arena
                        .region(*h)
                        .is_some_and(|r| r.kind == RegionKind::FontString)
                }) else {
                    continue;
                };
                let Some(data) = model.region_data.get_mut(&region) else {
                    continue;
                };
                let Some(height) = data.font_height.filter(|h| h.is_finite() && *h > 0.0) else {
                    continue;
                };
                live.push(region);
                let size = state.sizes.entry(region).or_insert(ChatSize {
                    authored: height,
                    applied: height,
                });
                if height.to_bits() != size.applied.to_bits() {
                    size.authored = height;
                }
                size.applied = size.authored.max(minimum);
                if height.to_bits() != size.applied.to_bits() {
                    data.font_height = Some(size.applied);
                    model.touch_measure(region);
                    changed += 1;
                }
            }
            state.sizes.retain(|region, _| live.contains(region));
        } else {
            for (region, size) in state.sizes.drain() {
                let Some(data) = model.region_data.get_mut(&region) else {
                    continue;
                };
                // A newer addon write wins, even when the policy is disabled before a sync.
                if data.font_height.map(f32::to_bits) == Some(size.applied.to_bits())
                    && size.authored.to_bits() != size.applied.to_bits()
                {
                    data.font_height = Some(size.authored);
                    model.touch_measure(region);
                    changed += 1;
                }
            }
        }
        drop(model);
        if minimum.is_some() {
            self.lua().set_app_data(state);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script() -> UiScript {
        let s = UiScript::new().unwrap();
        s.run(
            r#"
            ChatFrame1 = CreateFrame('ScrollingMessageFrame', 'ChatFrame1')
            ChatFrame1:SetFont('Fonts\\FRIZQT__.TTF', 12, 'OUTLINE')
            ChatFrame2 = CreateFrame('ScrollingMessageFrame', 'ChatFrame2')
            ChatFrame2:SetFont('Fonts\\FRIZQT__.TTF', 20, '')
            OtherChat = CreateFrame('ScrollingMessageFrame', 'OtherChat')
            OtherChat:SetFont('Fonts\\FRIZQT__.TTF', 10, '')
            ProbeFont = CreateFont('ProbeFont')
            ProbeFont:SetFont('Fonts\\FRIZQT__.TTF', 11, '')
            owner = CreateFrame('Frame', 'Owner')
            label = owner:CreateFontString('QuestBody')
            label:SetFontObject(ProbeFont)
            label:SetText("Classic metric snapshot")
        "#,
        )
        .unwrap();
        s
    }

    fn height(s: &UiScript, name: &str) -> f32 {
        s.eval(&format!("local _, h = {name}:GetFont(); return h"))
            .unwrap()
    }

    #[test]
    fn disabled_is_noop_for_every_engine_and_lua_visible_font() {
        let s = script();
        let before = s.model_ref().region_data.clone();
        // The 1.12 FontString ABI exposes measured height as GetHeight, not GetStringHeight.
        let metrics: (f64, f64) = s
            .eval("return label:GetHeight(), label:GetStringWidth()")
            .unwrap();
        let heights: Vec<_> = [
            "ChatFrame1",
            "ChatFrame2",
            "OtherChat",
            "ProbeFont",
            "label",
        ]
        .map(|name| height(&s, name))
        .into();
        for _ in 0..3 {
            assert_eq!(s.sync_chat_text_minimum(None), 0);
        }
        assert!(s.lua().app_data_ref::<ChatTextState>().is_none());
        let after_metrics: (f64, f64) = s
            .eval("return label:GetHeight(), label:GetStringWidth()")
            .unwrap();
        assert_eq!(metrics.0.to_bits(), after_metrics.0.to_bits());
        assert_eq!(metrics.1.to_bits(), after_metrics.1.to_bits());
        for (region, data) in &before {
            let model = s.model_ref();
            let after = &model.region_data[region];
            assert_eq!(
                data.font_height.map(f32::to_bits),
                after.font_height.map(f32::to_bits)
            );
            assert_eq!(data.font_explicit, after.font_explicit);
        }
        assert_eq!(
            heights,
            [
                "ChatFrame1",
                "ChatFrame2",
                "OtherChat",
                "ProbeFont",
                "label"
            ]
            .map(|name| height(&s, name))
        );
    }

    #[test]
    fn minimum_is_local_truthful_idempotent_and_restores_exactly() {
        let s = script();
        let before = s.model_ref().region_data.clone();
        assert_eq!(s.sync_chat_text_minimum(Some(16.0)), 1);
        assert_eq!(height(&s, "ChatFrame1"), 16.0);
        assert_eq!(height(&s, "ChatFrame2"), 20.0);
        assert_eq!(height(&s, "OtherChat"), 10.0);
        assert_eq!(height(&s, "ProbeFont"), 11.0);
        assert_eq!(height(&s, "label"), 11.0);
        for _ in 0..3 {
            assert_eq!(s.sync_chat_text_minimum(Some(16.0)), 0);
        }
        assert_eq!(s.sync_chat_text_minimum(None), 1);
        for (region, data) in &before {
            let model = s.model_ref();
            let after = &model.region_data[region];
            assert_eq!(
                data.font_height.map(f32::to_bits),
                after.font_height.map(f32::to_bits)
            );
            assert_eq!(data.font_path, after.font_path);
            assert_eq!(data.outline, after.outline);
            assert_eq!(data.font_explicit, after.font_explicit);
        }
    }

    #[test]
    fn addon_sizes_become_baseline_without_stacking_and_last_write_wins() {
        let s = script();
        s.sync_chat_text_minimum(Some(16.0));
        s.run("ChatFrame1:SetFont('new.ttf', 13, 'THICKOUTLINE')")
            .unwrap();
        assert_eq!(height(&s, "ChatFrame1"), 13.0); // accepted one-frame gap
        assert_eq!(s.sync_chat_text_minimum(Some(16.0)), 1);
        assert_eq!(s.sync_chat_text_minimum(Some(16.0)), 0);
        s.sync_chat_text_minimum(None);
        let font: (String, f32, String) = s.eval("return ChatFrame1:GetFont()").unwrap();
        assert_eq!(font, ("new.ttf".into(), 13.0, "THICKOUTLINE".into()));
        s.sync_chat_text_minimum(Some(16.0));
        s.run("ChatFrame1:SetFont('new.ttf', 16, '')").unwrap();
        assert_eq!(s.sync_chat_text_minimum(None), 0);
        assert_eq!(height(&s, "ChatFrame1"), 16.0);
        s.sync_chat_text_minimum(Some(18.0));
        s.run("ChatFrame1:SetFont('new.ttf', 14, '')").unwrap();
        assert_eq!(s.sync_chat_text_minimum(None), 0);
        assert_eq!(height(&s, "ChatFrame1"), 14.0);
    }

    #[test]
    fn late_chat_windows_and_minimum_changes_preserve_original() {
        let s = script();
        s.sync_chat_text_minimum(Some(16.0));
        s.run("ChatFrame10 = CreateFrame('ScrollingMessageFrame', 'ChatFrame10'); ChatFrame10:SetFont('late.ttf', 9.5, '')").unwrap();
        assert_eq!(s.sync_chat_text_minimum(Some(16.0)), 1);
        assert_eq!(height(&s, "ChatFrame10"), 16.0);
        assert_eq!(s.sync_chat_text_minimum(Some(18.0)), 2);
        s.sync_chat_text_minimum(None);
        assert_eq!(height(&s, "ChatFrame10"), 9.5);
        assert_eq!(height(&s, "ChatFrame1"), 12.0);
    }
}
