//! Canonical script names and their owning frame types.

use crate::widget::FrameKind;

/// The frame base's script names (`0x48e680`), after `OnEvent` (`0x816830`), on every frame type.
const BASE_SCRIPT_KINDS: [&str; 20] = [
    "OnEvent",
    "OnLoad",
    "OnSizeChanged",
    "OnUpdate",
    "OnShow",
    "OnHide",
    "OnEnter",
    "OnLeave",
    "OnMouseDown",
    "OnMouseUp",
    "OnMouseWheel",
    "OnDragStart",
    "OnDragStop",
    "OnReceiveDrag",
    "OnChar",
    "OnKeyDown",
    "OnKeyUp",
    "OnAttributeChanged",
    "OnEnable",
    "OnDisable",
];

/// The hyperlink trio (`0x978610`): SimpleHTML and both message frames.
const HYPERLINK_SCRIPT_KINDS: [&str; 3] =
    ["OnHyperlinkEnter", "OnHyperlinkLeave", "OnHyperlinkClick"];

/// Each frame type's own script names, which build 12340's lookup for that type tests before
/// falling back to [`BASE_SCRIPT_KINDS`] (Button `0x96f200`, EditBox `0x9628f0`, GameTooltip
/// `0x61a280`, MovieFrame `0x95db80`, Model `0x95f420`, StatusBar `0x961630`, Slider `0x96bca0`,
/// ScrollFrame `0x96b0b0`, ColorSelect `0x96dc50`, ScrollingMessageFrame `0x969040` over the
/// hyperlink trio). Labelled: which class owns each lookup is read from the names it tests, not
/// from the vtables' RTTI.
///
/// Not every accepted kind fires: `OnCharComposition` and `OnInputLanguageChanged` need an IME the
/// host does not have (capture-only); `OnTooltipSetQuest` and `OnTooltipSetAchievement` need the
/// quest and achievement link tooltips, which `SetHyperlink` does not render yet; and
/// `OnTooltipSetUnit`, `OnTooltipSetItem`, `OnTooltipSetSpell` and `OnTooltipSetFrameStack` are
/// not fired by their setters yet.
fn own_script_kinds(kind: FrameKind) -> (&'static [&'static str], &'static [&'static str]) {
    const NONE: &[&str] = &[];
    match kind {
        FrameKind::Button | FrameKind::CheckButton | FrameKind::LootButton => {
            (&["PreClick", "OnClick", "PostClick", "OnDoubleClick"], NONE)
        }
        FrameKind::EditBox => (
            &[
                "OnEnterPressed",
                "OnEscapePressed",
                "OnSpacePressed",
                "OnTabPressed",
                "OnTextChanged",
                "OnTextSet",
                "OnCursorChanged",
                "OnInputLanguageChanged",
                "OnEditFocusGained",
                "OnEditFocusLost",
                "OnCharComposition",
            ],
            NONE,
        ),
        FrameKind::GameTooltip => (
            &[
                "OnTooltipSetDefaultAnchor",
                "OnTooltipCleared",
                "OnTooltipAddMoney",
                "OnTooltipSetUnit",
                "OnTooltipSetItem",
                "OnTooltipSetSpell",
                "OnTooltipSetQuest",
                "OnTooltipSetAchievement",
                "OnTooltipSetEquipmentSet",
                "OnTooltipSetFrameStack",
            ],
            NONE,
        ),
        FrameKind::MovieFrame => (
            &[
                "OnMovieFinished",
                "OnMovieShowSubtitle",
                "OnMovieHideSubtitle",
            ],
            NONE,
        ),
        FrameKind::Model
        | FrameKind::PlayerModel
        | FrameKind::DressUpModel
        | FrameKind::TabardModel => (&["OnUpdateModel", "OnAnimFinished"], NONE),
        FrameKind::StatusBar | FrameKind::Slider => (&["OnValueChanged", "OnMinMaxChanged"], NONE),
        FrameKind::ScrollFrame => (
            &[
                "OnHorizontalScroll",
                "OnVerticalScroll",
                "OnScrollRangeChanged",
            ],
            NONE,
        ),
        FrameKind::ColorSelect => (&["OnColorSelect"], NONE),
        FrameKind::SimpleHtml | FrameKind::MessageFrame => (&HYPERLINK_SCRIPT_KINDS, NONE),
        FrameKind::ScrollingMessageFrame => (&["OnMessageScrollChanged"], &HYPERLINK_SCRIPT_KINDS),
        FrameKind::Frame
        | FrameKind::WorldFrame
        | FrameKind::Minimap
        | FrameKind::Cooldown
        | FrameKind::QuestPoiFrame => (NONE, NONE),
    }
}

/// The canonical spelling of script kind `name` on a frame of type `kind`, or `None` if that type
/// has no such script, matched case-insensitively as the client's lookups do.
pub(crate) fn script_kind(kind: FrameKind, name: &str) -> Option<&'static str> {
    let (own, inherited) = own_script_kinds(kind);
    own.iter()
        .chain(inherited)
        .chain(BASE_SCRIPT_KINDS.iter())
        .copied()
        .find(|k| k.eq_ignore_ascii_case(name))
}
