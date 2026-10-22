//! Build 12340's right-hand column (`CharacterCreate.xml`): `CharacterCreateWoWLogo` (256×128 at
//! TOPRIGHT (-9,0)) with the race pane under it and the class pane under that, each 267×287
//! anchored TOP to the frame above's BOTTOM 5 left (race (−5,5), class (−5,−5)). A pane is the
//! faction-tinted `UI-Tooltip-Background` inset (12,3,20,6), the round header icon at (4,4) under
//! the `UI-CharacterCreate-InfoBox` frame, the `FactionName_Shadow_Huge` label at TOP (40,−10),
//! and a 220² `GlueScrollFrameTemplate` at (20,55) holding the gold lines over the flavor text.
//!
//! Labelled: `GlueFontCharacterCreate`'s `spacing="1.5"` line gap is not applied.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::ui_render::ui_material::MaterialNode;

use super::parts::{
    DynIcon, DynText, DynTint, InfoKind, InfoScroll, ScrollArrow, ScrollHides, ScrollThumb,
};
use crate::glue::art::{
    tc_rect, GlueArt, ScrollArt, ALLIANCE_FILL, FALLBACK_ALPHA, GOLD, INFO_TEXT, SCROLL_BTN_TC,
};
use crate::glue::widgets::{abs, outlined_text, overlay, GlueText, Hilight};

/// A pane's size and its canvas offsets from the top right (the anchors resolved).
const PANE: (f32, f32) = (267.0, 287.0);
const RACE_RIGHT_TOP: (f32, f32) = (8.5, 123.0);
const CLASS_RIGHT_TOP: (f32, f32) = (13.5, 415.0);
/// `UI-CharacterCreate-InfoBox`'s authored sub-rect.
const INFO_BOX_TC: [f32; 4] = [0.005859375, 0.52734375, 0.005859375, 0.56640625];
/// The scroll frame, in pane coordinates.
const SCROLL: (f32, f32, f32) = (20.0, 55.0, 220.0);

pub(super) fn logo(ui: &mut ChildSpawnerCommands, art: &GlueArt, s: f32) {
    if let Some(logo) = &art.logo {
        ui.spawn((
            ImageNode::new(logo.clone()),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(9.0 * s),
                top: Val::Px(0.0),
                width: Val::Px(256.0 * s),
                height: Val::Px(128.0 * s),
                ..default()
            },
        ));
    }
}

pub(super) fn right_stack(
    ui: &mut ChildSpawnerCommands,
    art: &GlueArt,
    font: &Handle<Font>,
    title_font: &Handle<Font>,
    s: f32,
) {
    let px = |v: f32| Val::Px(v * s);
    for (kind, (right, top)) in [
        (InfoKind::Race, RACE_RIGHT_TOP),
        (InfoKind::Class, CLASS_RIGHT_TOP),
    ] {
        let mut pane = ui.spawn((Node {
            position_type: PositionType::Absolute,
            right: px(right),
            top: px(top),
            width: px(PANE.0),
            height: px(PANE.1),
            ..default()
        },));
        if art.tooltip_bg.is_none() {
            pane.insert((
                DynTint::BoxFill,
                BackgroundColor(ALLIANCE_FILL.with_alpha(FALLBACK_ALPHA)),
            ));
        }
        pane.with_children(|pane| {
            // The backdrop's bg, stretched (no `tile`) over its insets; the refresh tints it.
            if let Some(bg) = &art.tooltip_bg {
                pane.spawn((
                    DynTint::BoxFill,
                    ImageNode {
                        image: bg.clone(),
                        color: ALLIANCE_FILL,
                        ..default()
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(12.0),
                        right: px(3.0),
                        top: px(20.0),
                        bottom: px(6.0),
                        ..default()
                    },
                ));
            }
            // BORDER under the ARTWORK frame: the round icon shows through its ring.
            pane.spawn((
                DynIcon::Info(kind),
                ImageNode {
                    color: Color::NONE, // transparent until the refresh assigns art
                    ..default()
                },
                abs(s, 4.0, 4.0, 48.0, 48.0),
            ));
            if let Some((frame, size)) = &art.info_box {
                pane.spawn((
                    ImageNode {
                        image: frame.clone(),
                        rect: Some(tc_rect(*size, INFO_BOX_TC)),
                        ..default()
                    },
                    overlay(),
                ));
            }
            // `FactionName_Shadow_Huge`: MORPHEUS 18, white, TOP (40,−10) at width 220.
            outlined_text(
                pane,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(PANE.0 / 2.0 + 40.0 - 110.0),
                    top: px(10.0),
                    width: px(220.0),
                    ..default()
                },
                (),
                DynText::InfoTitle(kind),
                GlueText {
                    text: "",
                    size: 18.0,
                    color: Color::WHITE,
                    wrap: false,
                },
                title_font,
                s,
            );
            let scroll = pane
                .spawn((
                    InfoScroll,
                    Interaction::default(),
                    FocusPolicy::Block,
                    ScrollPosition::default(),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(SCROLL.0),
                        top: px(SCROLL.1),
                        width: px(SCROLL.2),
                        height: px(SCROLL.2),
                        flex_direction: FlexDirection::Column,
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                ))
                .with_children(|inner| {
                    // `CharacterCreateRaceAbilityText`/`ClassRolesText`, `GlueFontNormalSmall`.
                    outlined_text(
                        inner,
                        Node::default(),
                        (),
                        DynText::InfoAbilities(kind),
                        GlueText {
                            text: "",
                            size: 12.0,
                            color: GOLD,
                            wrap: true,
                        },
                        font,
                        s,
                    );
                    // The flavor text 2 below, `GlueFontCharacterCreate`.
                    outlined_text(
                        inner,
                        Node {
                            margin: UiRect::top(px(2.0)),
                            ..default()
                        },
                        (),
                        DynText::InfoBody(kind),
                        GlueText {
                            text: "",
                            size: 12.0,
                            color: INFO_TEXT,
                            wrap: true,
                        },
                        font,
                        s,
                    );
                })
                .id();
            if let Some(sc) = &art.scroll {
                scrollbar(pane, sc, scroll, s);
            }
        });
    }
}

/// A pane's `GlueScrollBarTemplate`, hidden while the frame fits its text
/// (`GlueScrollFrame_OnScrollRangeChanged`): the 16-wide slider re-anchored at the frame's right
/// (7,4)..(7,12), the 18² arrows outside its ends, the 18×24 knob inside.
fn scrollbar(pane: &mut ChildSpawnerCommands, sc: &ScrollArt, scroll: Entity, s: f32) {
    let left = SCROLL.0 + SCROLL.2 + 7.0;
    let top = SCROLL.1 - 4.0;
    let bar_h = SCROLL.2 + 4.0 - 12.0;
    for (up, y, art) in [
        (true, top - 18.0, &sc.up_btn),
        (false, top + bar_h, &sc.down_btn),
    ] {
        pane.spawn((
            ScrollHides { scroll },
            Visibility::Hidden,
            ScrollArrow {
                scroll,
                up,
                step: bar_h * s / 2.0,
            },
            Button,
            // Disabled at the end of travel: `scroll_visuals` writes it and `glue_hilights`
            // reads it, so the sheen goes with the arrow.
            crate::glue::widgets::GlueDisabled(false),
            ImageNode {
                image: art.up.clone(),
                rect: Some(tc_rect(art.size, SCROLL_BTN_TC)),
                ..default()
            },
            abs(s, left - 1.0, y, 18.0, 18.0),
        ))
        .with_children(|b| {
            b.spawn((
                Hilight,
                Visibility::Hidden,
                MaterialNode(art.hi.clone()),
                overlay(),
            ));
        });
    }
    pane.spawn((
        ScrollHides { scroll },
        Visibility::Hidden,
        abs(s, left, top, 16.0, bar_h),
    ))
    .with_children(|track| {
        track.spawn((
            ScrollThumb {
                scroll,
                travel: (bar_h - 24.0) * s,
            },
            Button,
            ImageNode {
                image: sc.knob.0.clone(),
                rect: Some(tc_rect(sc.knob.1, [0.2, 0.8, 0.125, 0.875])),
                ..default()
            },
            abs(s, -1.0, 0.0, 18.0, 24.0),
        ));
    });
}
