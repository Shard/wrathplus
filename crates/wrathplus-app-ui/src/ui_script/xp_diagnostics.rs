//! Observation-only XP-bar diagnostics. Paint route is context, not a transition trigger:
//! normal full/splice/clean alternation must not produce a line every frame.

use wrathplus_ui::script::{ExtractedQuad, QuadContent, UiScript};

use crate::ui_pass::UiQuad;

// Read getters only; no globals, handlers, visibility or widget values are written.
const SNAPSHOT: &str = r#"
local out = {}
local function add(k, v) out[#out+1] = k .. '=' .. tostring(v) end
local f = MainMenuExpBar
add('exists', f ~= nil)
if f then
    add('shown', f:IsShown()); add('visible', f:IsVisible())
    local lo, hi = f:GetMinMaxValues()
    add('min', lo); add('max', hi); add('value', f:GetValue())
    add('alpha', f:GetAlpha())
    add('left', f:GetLeft()); add('bottom', f:GetBottom())
    add('width', f:GetWidth()); add('height', f:GetHeight())
    local p = f:GetParent()
    for i = 1, 8 do
        if not p then break end
        add('parent'..i, p:GetName() or '<anonymous>')
        add('parent'..i..'_shown', p:IsShown())
        add('parent'..i..'_visible', p:IsVisible())
        add('parent'..i..'_alpha', p:GetAlpha())
        p = p:GetParent()
    end
    add('ancestors_truncated', p ~= nil)
end
add('party', GetNumPartyMembers())
add('quest_visible', QuestFrame and QuestFrame:IsVisible() or false)
add('questlog_visible', QuestLogFrame and QuestLogFrame:IsVisible() or false)
add('xp', UnitXP('player')); add('xpmax', UnitXPMax('player'))
return table.concat(out, ' ')
"#;

#[derive(Default)]
pub(super) struct Diagnostics {
    last: Option<String>,
}

impl Diagnostics {
    fn changed(&mut self, snapshot: String) -> bool {
        if self.last.as_ref() == Some(&snapshot) {
            return false;
        }
        self.last = Some(snapshot);
        true
    }

    /// The caller's existing VmMemo resets this baseline on VM replacement. Bounds apply to
    /// ancestor depth and raw/paint samples; unrelated UI entries and cache routes never dedup-key.
    pub(super) fn observe(
        &mut self,
        script: &UiScript,
        extracted: &[ExtractedQuad],
        spans: &[u32],
        painted: &[UiQuad],
        route: &str,
    ) {
        let mut state = script
            .eval::<String>(SNAPSHOT)
            .unwrap_or_else(|e| format!("snapshot_error={e}"));
        state.push_str(&format!(
            " effective_alpha={:?}",
            script.frame_effective_alpha("MainMenuExpBar")
        ));
        let mut raw_count = 0;
        let mut painted_count = 0;
        for (i, q) in extracted.iter().enumerate() {
            if !matches!(q.content, QuadContent::Texture { .. })
                || script.quad_owner_name(q.target).as_deref() != Some("MainMenuExpBar")
            {
                continue;
            }
            raw_count += 1;
            if raw_count <= 8 {
                state.push_str(&format!(
                    " raw_texture{raw_count}=[rect={:?} alpha={} clip={:?} content={:?}]",
                    q.rect, q.alpha, q.clip, q.content
                ));
            }
            let start = if i == 0 {
                0
            } else {
                spans.get(i - 1).copied().unwrap_or(0)
            };
            if let Some(end) = spans.get(i) {
                if let Some(slice) = painted.get(start as usize..*end as usize) {
                    for quad in slice {
                        painted_count += 1;
                        if painted_count <= 8 {
                            state.push_str(&format!(
                                " painted_texture{painted_count}=[rect={:?} color={:?} uv={:?} texture={:?} clip={:?}]",
                                quad.rect, quad.color, quad.uv.corners, quad.texture, quad.clip
                            ));
                        }
                    }
                }
            }
        }
        state.push_str(&format!(
            " raw_textures={raw_count} painted_textures={painted_count}"
        ));
        let baseline = self.last.is_none();
        if self.changed(state) {
            bevy::log::info!(
                "[xp-diag] vm={} baseline={} paint={} {}",
                script.session(),
                baseline,
                route,
                self.last.as_deref().unwrap_or_default()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_core::vm_memo::VmMemo;

    fn fixture() -> UiScript {
        let mut s = UiScript::new().unwrap();
        s.run(
            r#"
            local p = CreateFrame('Frame', 'XpParent')
            local f = CreateFrame('StatusBar', 'MainMenuExpBar', p)
            f:SetSize(100, 10); f:SetPoint('BOTTOMLEFT', 0, 0)
            f:SetMinMaxValues(0, 100); f:SetValue(25)
            f:SetStatusBarTexture('diagnostic-test')
        "#,
        )
        .unwrap();
        s.resolve();
        s
    }

    #[test]
    fn baseline_dedup_and_vm_reset() {
        let first = fixture();
        let second = fixture();
        let mut memo = VmMemo::<Diagnostics>::default();
        let state = first.eval::<String>(SNAPSHOT).unwrap();
        assert!(memo.get(&first).changed(state.clone()));
        assert!(!memo.get(&first).changed(state.clone()));
        assert!(memo.get(&second).changed(state));
    }

    #[test]
    fn raw_fill_transitions_but_paint_route_alone_does_not() {
        let mut s = fixture();
        let mut d = Diagnostics::default();
        let raw = s.extract();
        d.observe(&s, &raw, &[], &[], "full");
        let baseline = d.last.clone();
        assert!(baseline.as_ref().unwrap().contains("raw_textures=1"));
        d.observe(&s, &raw, &[], &[], "splice");
        assert_eq!(d.last, baseline);
        d.observe(&s, &raw, &[], &[], "clean-skipped");
        assert_eq!(d.last, baseline);
        s.run("MainMenuExpBar:SetValue(75)").unwrap();
        s.resolve();
        d.observe(&s, &s.extract(), &[], &[], "splice");
        assert_ne!(d.last, baseline);
        s.run("XpParent:Hide()").unwrap();
        s.resolve();
        d.observe(&s, &s.extract(), &[], &[], "full");
        assert!(d.last.as_ref().unwrap().contains("raw_textures=0"));
    }

    #[test]
    fn visibility_range_value_and_context_transitions() {
        let s = fixture();
        let mut d = Diagnostics::default();
        assert!(d.changed(s.eval(SNAPSHOT).unwrap()));
        for change in [
            "XpParent:Hide()",
            "XpParent:Show()",
            "MainMenuExpBar:Hide()",
            "MainMenuExpBar:Show()",
            "MainMenuExpBar:SetMinMaxValues(0, 200)",
            "MainMenuExpBar:SetValue(50)",
            "MainMenuExpBar:SetAlpha(0)",
            "CreateFrame('Frame', 'QuestFrame')",
            "QuestFrame:Hide()",
        ] {
            s.run(change).unwrap();
            let now = s.eval::<String>(SNAPSHOT).unwrap();
            assert!(d.changed(now.clone()), "{change}");
            assert!(!d.changed(now), "settled after {change}");
        }
    }
}
