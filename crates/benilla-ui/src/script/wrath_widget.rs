//! Regression tests for native Wrath widget edges and diagnostics.
use crate::script::{ScriptDialect, UiScript};

#[test]
fn wrath_script_slots_and_button_edges() {
    let s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    s.run(r#"
        local b = CreateFrame('Button')
        local calls = {}
        b:SetScript('OnDisable', function(self) assert(self == b); table.insert(calls, 'disable') end)
        b:SetScript('OnEnable', function(self) table.insert(calls, 'enable') end)
        b:Disable(); b:Disable(); b:Enable(); b:Enable()
        b:SetScript('PreClick', function(self, button, down)
            assert(self == b and button == 'LeftButton' and down == false)
            table.insert(calls, 'pre')
        end)
        b:SetScript('OnClick', function() table.insert(calls, 'click') end)
        b:SetScript('PostClick', function() table.insert(calls, 'post') end)
        b:Click()
        assert(table.concat(calls, ',') == 'disable,enable,pre,click,post')
        local tip = CreateFrame('GameTooltip', 'SlotProbeTip')
        local movie = CreateFrame('MovieFrame')
        for owner, kinds in pairs({[tip] = {'OnTooltipSetUnit','OnTooltipSetItem',
            'OnTooltipSetSpell','OnTooltipSetQuest','OnTooltipSetAchievement',
            'OnTooltipSetEquipmentSet','OnTooltipSetFrameStack'},
            [movie] = {'OnMovieFinished','OnMovieShowSubtitle','OnMovieHideSubtitle'}}) do
            for _, kind in ipairs(kinds) do
                owner:SetScript(kind, function() end)
                assert(type(owner:GetScript(kind)) == 'function')
                assert(not b:HasScript(kind))
            end
        end
        local edit = CreateFrame('EditBox')
        assert(edit:HasScript('OnCharComposition') and edit:HasScript('OnInputLanguageChanged'))
        local bar = CreateFrame('StatusBar')
        local count = 0
        bar:SetScript('OnMinMaxChanged', function(self, min, max)
            assert(self == bar and min == 1 and max == 10); count = count + 1
        end)
        bar:SetMinMaxValues(1, 10); bar:SetMinMaxValues(1, 10)
        assert(count == 1)
    "#).unwrap();
}

#[test]
fn wrath_debuglocals_is_bounded_and_does_not_expose_debug_library() {
    let s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
    s.run(
        r#"
        assert(debug == nil)
        local marker = 12345
        local result = debuglocals()
        assert(type(result) == 'string' and string.find(result, 'marker = 12345', 1, true))
        assert(debuglocals(999999) == '')
        local hostile = setmetatable({}, {__tostring=function() error('must not execute') end})
        assert(type(debuglocals()) == 'string')
        assert(type(GetCurrentResolution()) == 'number')
        assert(type(RestoreVideoResolutionDefaults) == 'function')
        RestoreVideoResolutionDefaults()
    "#,
    )
    .unwrap();
}
