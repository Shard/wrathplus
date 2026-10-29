//! The create screen's layout, the reference's `CharacterCreate.xml` in Bevy UI. The glue engine
//! scales a 1024×768 virtual screen to the window, so every authored offset and size below is the
//! reference's number times `height / 768`.

use bevy::prelude::*;
use bevy::ui_render::ui_material::MaterialNode;
use bevy::window::PrimaryWindow;

use crate::char_select::race_name;
use crate::entities::CharCreate;
use crate::glue_strings::GlueStrings;
use crate::portrait::{GlueLook, GluePreview, PortraitImages, PortraitSource, GLUE_SLOT};
use crate::ui_text::wow_font;
use wrathplus_assets::WorldAssets;

use super::parts::{CharCreateUi, DialRow, DynIcon, DynText, DynTint, StatusLine};
use super::{CreateAction, CreateSelection, ALLIANCE, HORDE, INITIAL_FACING};
use crate::glue::art::{
    tc_rect, GlueArt, ALLIANCE_BORDER, ALLIANCE_FILL, BACKDROP, BTN_BG, DIM, GOLD, INFO_TEXT,
};
use crate::glue::widgets::{
    abs, dial_arrow, glue_button, icon_button, outlined_text, ArtSwap, FallbackFace, GlueBtnKind,
    GlueText, Hilight,
};

const SCREEN_Z: i32 = 1100;

// ── Spawn ────────────────────────────────────────────────────────────────────────────────────────

pub(super) fn enter_create(
    mut commands: Commands,
    assets: Res<AssetServer>,
    catalog: Option<Res<CharCreate>>,
    portraits: Res<PortraitImages>,
    mut sel: ResMut<CreateSelection>,
    mut preview: ResMut<GluePreview>,
    mut art: ResMut<GlueArt>,
    world_assets: Option<ResMut<WorldAssets>>,
    mut images: ResMut<Assets<Image>>,
    mut add_mats: ResMut<Assets<crate::glue::add_material::AddUiMaterial>>,
    strings: Option<Res<GlueStrings>>,
    window: Query<&Window, With<PrimaryWindow>>,
    roster: Res<crate::app_core::state::Roster>,
    paid: Res<super::paid::PaidChange>,
) {
    sel.death_knight = roster.chars.iter().any(|c| c.level >= 55);
    sel.reset(catalog.as_deref());
    // `CustomizeExistingCharacter` (`0x4e2330`): a paid service starts from the character's look
    // and name (`PaidChange_GetName`).
    if let Some(t) = &paid.0 {
        sel.race = t.race;
        sel.sex = t.sex;
        sel.class = t.class;
        sel.dials = t.dials;
        sel.name.set_text(&t.name);
        sel.clamp(catalog.as_deref());
    }
    preview.scene = Some(crate::portrait::GlueScene::Race {
        race: sel.race,
        class: sel.class,
    });
    preview.look = Some(GlueLook::Create(sel.look()));
    preview.yaw = INITIAL_FACING;
    if let Some(mut wa) = world_assets {
        art.ensure_loaded(&mut wa, &mut images, &mut add_mats);
    }
    spawn_screen(
        &mut commands,
        &assets,
        &portraits,
        &art,
        strings.as_deref(),
        &window,
    );
}

/// Rebuild the tree when a resize changes the glue scale; the selection and name live in
/// resources, so the rebuild loses nothing.
pub(super) fn rescale_screen(
    mut commands: Commands,
    existing: Query<(Entity, &CharCreateUi)>,
    assets: Res<AssetServer>,
    portraits: Res<PortraitImages>,
    art: Res<GlueArt>,
    strings: Option<Res<GlueStrings>>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    let s = crate::glue::screen_scale(window.single().ok());
    for (root, ui) in &existing {
        if ui.s != s {
            commands.entity(root).despawn();
            spawn_screen(
                &mut commands,
                &assets,
                &portraits,
                &art,
                strings.as_deref(),
                &window,
            );
        }
    }
}

fn spawn_screen(
    commands: &mut Commands,
    assets: &AssetServer,
    portraits: &PortraitImages,
    art: &GlueArt,
    strings: Option<&GlueStrings>,
    window: &Query<&Window, With<PrimaryWindow>>,
) {
    let font = wow_font(assets);
    // The edit box types in `GlueEditBoxFont`, ARIALN, not FRIZQT (`GlueFonts.xml`).
    let edit_font: Handle<Font> = assets.load("mpq://Fonts/ARIALN.ttf");
    // The pane labels' `FactionName_Shadow_Huge`.
    let title_font: Handle<Font> = assets.load("mpq://Fonts/MORPHEUS.ttf");
    let model_image = match portraits.0.get(GLUE_SLOT) {
        Some(PortraitSource::Live(h)) => Some(h.clone()),
        _ => None,
    };
    let s = crate::glue::screen_scale(window.single().ok());
    let px = |v: f32| Val::Px(v * s);
    let empty = GlueStrings::default();
    let strings = strings.unwrap_or(&empty);

    let root = commands
        .spawn((
            CharCreateUi { s },
            DynTint::Backdrop,
            GlobalZIndex(SCREEN_Z),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(BACKDROP),
        ))
        .with_children(|ui| {
            // The 3D scene first, full window: the reference's screen is a fullscreen ModelFFX,
            // the race background with the character in it, and the whole frame drags to rotate.
            // A pillarbox's bars are the booth camera's own clear inside this target.
            let mut pane = ui.spawn((
                CreateAction::Model,
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
            ));
            if let Some(image) = model_image {
                pane.insert(ImageNode::new(image));
            }
        })
        .id();

    // The chrome hangs off the canvas, the boxed scene's rect, never the window.
    let mut canvas = commands.spawn((crate::glue::glue_canvas(), ChildOf(root)));
    canvas.with_children(|ui| {
        left_tower(ui, art, &font, s, strings);

        super::panels::logo(ui, art, s);
        super::panels::right_stack(ui, art, &font, &title_font, s);
        name_cluster(ui, art, &font, &edit_font, s, strings);
        rotate_cluster(ui, art, &font, s);

        // `CharCreateOkayButton` 160×35, TOP at the class pane's BOTTOM (4,−3), over
        // `CharCreateBackButton` 120×30, TOP at its BOTTOM (4,3): centers 143 and 139 from the
        // canvas's right edge.
        for (action, key, fallback, (w, h), kind, (right, top)) in [
            (
                CreateAction::Create,
                "CHARACTER_CREATE_ACCEPT",
                "Accept",
                (160.0, 35.0),
                GlueBtnKind::Normal,
                (63.0, 705.0),
            ),
            (
                CreateAction::Back,
                "BACK",
                "Back",
                (120.0, 30.0),
                GlueBtnKind::Small,
                (79.0, 737.0),
            ),
        ] {
            ui.spawn((Node {
                position_type: PositionType::Absolute,
                right: px(right),
                top: px(top),
                ..default()
            },))
                .with_children(|slot| {
                    glue_button(
                        slot,
                        art,
                        &font,
                        action,
                        strings.text(key, fallback),
                        w,
                        h,
                        kind,
                        s,
                    );
                });
        }
    });
}

/// Build 12340's configuration frame, `CharacterCreateConfigurationFrame` (256×758 at TOPLEFT
/// (9,-5)), every child at its authored offset: three `OuterBorder` pieces over the tooltip
/// background, the banners, the two faction labels, two columns of five race buttons, the gender
/// pair, two rows of five class buttons, then the five dials stacked up from Randomize.
fn left_tower(
    ui: &mut ChildSpawnerCommands,
    art: &GlueArt,
    font: &Handle<Font>,
    s: f32,
    strings: &GlueStrings,
) {
    let px = |v: f32| Val::Px(v * s);
    // A 38² button's left edge for a TOP anchor at `x` from the frame's 128 center.
    let centered = |x: f32| 128.0 + x - 19.0;
    ui.spawn((Node {
        position_type: PositionType::Absolute,
        left: px(9.0),
        top: px(5.0),
        width: px(256.0),
        height: px(758.0),
        ..default()
    },))
        .with_children(|tower| {
            // `CharacterCreateConfigurationBackground`: the tooltip background inset (10,8) from
            // the border's corners.
            if let Some(bg) = &art.tooltip_bg {
                tower.spawn((ImageNode::new(bg.clone()), abs(s, 10.0, 8.0, 236.0, 742.0)));
            }
            // The three `OuterBorder` pieces: the first flipped (top 1, bottom 0), the second
            // cut at 0.9609375, the third whole.
            if let Some((border, size)) = &art.tower_border {
                for (top, height, tc, flip) in [
                    (0.0, 256.0, [0.0, 1.0, 0.0, 1.0], true),
                    (256.0, 246.0, [0.0, 1.0, 0.0, 0.9609375], false),
                    (502.0, 256.0, [0.0, 1.0, 0.0, 1.0], false),
                ] {
                    tower.spawn((
                        ImageNode {
                            image: border.clone(),
                            rect: Some(tc_rect(*size, tc)),
                            flip_y: flip,
                            ..default()
                        },
                        abs(s, 0.0, top, 256.0, height),
                    ));
                }
            }
            // `CharacterCreateBanners`, 256×512 at TOP (0,-5).
            if let Some(banners) = &art.banners {
                tower.spawn((
                    ImageNode::new(banners.clone()),
                    abs(s, 0.0, 5.0, 256.0, 512.0),
                ));
            }
            // `FactionName_Shadow_Huge` at TOP (∓50,-20); the XML's `text="ALLIANCE"` is a
            // GlueStrings key. Labelled: its size is taken as 20.
            for (key, fallback, x) in [("ALLIANCE", "Alliance", -50.0), ("HORDE", "Horde", 50.0)] {
                outlined_text(
                    tower,
                    Node {
                        justify_content: JustifyContent::Center,
                        ..abs(s, 128.0 + x - 60.0, 20.0, 120.0, 22.0)
                    },
                    (),
                    (),
                    GlueText {
                        text: strings.text(key, fallback),
                        size: 20.0,
                        color: GOLD,
                        wrap: false,
                    },
                    font,
                    s,
                );
            }

            // Race buttons 1 and 6 at TOP (∓50,-61), each next 21 below the last.
            for (faction, x) in [(ALLIANCE, -50.0), (HORDE, 50.0)] {
                tower
                    .spawn((Node {
                        position_type: PositionType::Absolute,
                        left: px(centered(x)),
                        top: px(61.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(21.0),
                        ..default()
                    },))
                    .with_children(|col| {
                        for race in faction {
                            icon_button(
                                col,
                                font,
                                CreateAction::Race(race),
                                Some(DynIcon::Race(race)),
                                None,
                                None::<DynText>,
                                race_name(race),
                                art,
                                s,
                            );
                        }
                    });
            }

            // The gender pair at TOP (∓20,-392).
            for (sex, key, fallback, x) in [
                (0u8, "MALE", "Male", -20.0),
                (1u8, "FEMALE", "Female", 20.0),
            ] {
                tower
                    .spawn((Node {
                        position_type: PositionType::Absolute,
                        left: px(centered(x)),
                        top: px(392.0),
                        ..default()
                    },))
                    .with_children(|g| {
                        let icon = art.gender.as_ref().map(|(h, size)| {
                            let half = if sex == 0 {
                                [0.0, 0.5, 0.0, 1.0]
                            } else {
                                [0.5, 1.0, 0.0, 1.0]
                            };
                            (h.clone(), tc_rect(*size, half))
                        });
                        icon_button(
                            g,
                            font,
                            CreateAction::Gender(sex),
                            None::<DynIcon>,
                            icon,
                            None::<DynText>,
                            strings.text(key, fallback),
                            art,
                            s,
                        );
                    });
            }

            // Class button 1 at TOP (-90,-463), the next four 6 right of each other; the second
            // row 6 below the first.
            tower
                .spawn((Node {
                    position_type: PositionType::Absolute,
                    left: px(centered(-90.0)),
                    top: px(463.0),
                    width: px(5.0 * 38.0 + 4.0 * 6.0),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: px(6.0),
                    row_gap: px(6.0),
                    ..default()
                },))
                .with_children(|grid| {
                    for slot in 0..10u8 {
                        icon_button(
                            grid,
                            font,
                            CreateAction::ClassSlot(slot),
                            Some(DynIcon::ClassSlot(slot)),
                            None,
                            Some(DynText::ClassSlotLabel(slot)),
                            "",
                            art,
                            s,
                        );
                    }
                });

            // The five 230×32 dials, `ButtonFrame5`'s BOTTOM at Randomize's TOP (-6,3), each
            // earlier one on top of the next: frame 1's top is 683 - 4·32.
            tower
                .spawn((Node {
                    position_type: PositionType::Absolute,
                    left: px(128.0 - 6.0 - 115.0),
                    top: px(683.0 - 4.0 * 32.0),
                    width: px(230.0),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },))
                .with_children(|dials| {
                    for dial in 0..5u8 {
                        dial_row(dials, art, font, dial, s);
                    }
                });

            // `CharCreateRandomizeButton`, 146×30 at BOTTOM (0,10).
            tower
                .spawn((Node {
                    position_type: PositionType::Absolute,
                    left: px(128.0 - 73.0),
                    top: px(758.0 - 10.0 - 30.0),
                    ..default()
                },))
                .with_children(|r| {
                    glue_button(
                        r,
                        art,
                        font,
                        CreateAction::Randomize,
                        strings.text("RANDOMIZE", "Randomize"),
                        146.0,
                        30.0,
                        GlueBtnKind::Small,
                        s,
                    );
                });
        });
}

/// One dial row: the `CharacterCreate-LabelFrame` 3-slice (64-tall art over the 32-tall row),
/// the per-race label and the arrow pair.
fn dial_row(
    dials: &mut ChildSpawnerCommands,
    art: &GlueArt,
    font: &Handle<Font>,
    dial: u8,
    s: f32,
) {
    let px = |v: f32| Val::Px(v * s);
    dials
        .spawn((
            DialRow(dial),
            Node {
                width: px(230.0),
                height: px(32.0),
                ..default()
            },
        ))
        .with_children(|row| {
            // The 128×64 art's 25|mid|25 slices: Left at x -5, Right ending at x 186 (RIGHT -44).
            if let Some((frame, size)) = &art.label_frame {
                for (left, width, tc) in [
                    (-5.0, 25.0, [0.0, 0.1953125, 0.0, 1.0]),
                    (20.0, 141.0, [0.1953125, 0.8046875, 0.0, 1.0]),
                    (161.0, 25.0, [0.8046875, 1.0, 0.0, 1.0]),
                ] {
                    row.spawn((
                        ImageNode {
                            image: frame.clone(),
                            rect: Some(tc_rect(*size, tc)),
                            ..default()
                        },
                        abs(s, left, -16.0, width, 64.0),
                    ));
                }
            }
            // `GlueFontHighlightSmall`, centered on the frame middle.
            outlined_text(
                row,
                Node {
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    // Centered on the middle slice, 2 up.
                    ..abs(s, 20.0, -2.0, 141.0, 32.0)
                },
                (),
                DynText::DialLabel(dial),
                GlueText {
                    text: "",
                    size: 12.0,
                    color: INFO_TEXT,
                    wrap: false,
                },
                font,
                s,
            );
            dial_arrow(
                row,
                &art.arrow_left,
                font,
                // The left button's RIGHT at the right one's LEFT +3.
                CreateAction::Dial(dial, -1),
                230.0 - 32.0 - 29.0,
                "<",
                s,
            );
            dial_arrow(
                row,
                &art.arrow_right,
                font,
                CreateAction::Dial(dial, 1),
                230.0 - 32.0,
                ">",
                s,
            );
        });
}

/// NAME over `CharacterCreateNameEdit` (156×40 at BOTTOM (8,50)), with the status line beneath.
fn name_cluster(
    ui: &mut ChildSpawnerCommands,
    art: &GlueArt,
    font: &Handle<Font>,
    edit_font: &Handle<Font>,
    s: f32,
    strings: &GlueStrings,
) {
    let px = |v: f32| Val::Px(v * s);
    ui.spawn((Node {
        position_type: PositionType::Absolute,
        bottom: px(50.0),
        left: px(8.0),
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: px(2.0),
        ..default()
    },))
        .with_children(|cluster| {
            outlined_text(
                cluster,
                Node::default(),
                (),
                (),
                GlueText {
                    text: strings.text("NAME", "Name"),
                    size: 18.0, // GlueFontNormalLarge
                    color: GOLD,
                    wrap: false,
                },
                font,
                s,
            );
            // Always Alliance-tinted, as the reference's `OnLoad` sets it.
            crate::glue::widgets::glue_edit_box(
                cluster,
                art,
                edit_font,
                (),
                DynText::Name,
                (156.0, 40.0),
                (ALLIANCE_BORDER, ALLIANCE_FILL),
                (15.0, 0.0, 0.0, 0.0), // CharacterCreate.xml: TextInsets left 15 only
                s,
            );
            // Empty until a create fails; the reference shows the error in a dialog, not built.
            outlined_text(
                cluster,
                Node::default(),
                (),
                StatusLine,
                GlueText {
                    text: "",
                    size: 13.0,
                    color: DIM,
                    wrap: true,
                },
                font,
                s,
            );
        });
}

/// `CharacterCreateRotateLeft/Right`: 50² at BOTTOMLEFT (237,0), overlapping -19, the left one
/// the mirrored `UI-RotationRight-Big`.
fn rotate_cluster(ui: &mut ChildSpawnerCommands, art: &GlueArt, font: &Handle<Font>, s: f32) {
    let px = |v: f32| Val::Px(v * s);
    ui.spawn((Node {
        position_type: PositionType::Absolute,
        left: px(237.0),
        bottom: px(0.0),
        flex_direction: FlexDirection::Row,
        ..default()
    },))
        .with_children(|rot| {
            for (action, flip, overlap) in [
                (CreateAction::RotateLeft, true, 0.0),
                (CreateAction::RotateRight, false, -19.0),
            ] {
                let mut b = rot.spawn((
                    action,
                    Button,
                    Node {
                        width: px(50.0),
                        height: px(50.0),
                        margin: UiRect::left(px(overlap)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                ));
                match (&art.rotate_up, &art.rotate_down) {
                    (Some(up), down) => {
                        b.insert(ImageNode {
                            image: up.clone(),
                            flip_x: flip,
                            ..default()
                        });
                        if let Some(down) = down {
                            b.insert(ArtSwap {
                                up: up.clone(),
                                down: down.clone(),
                            });
                        }
                        if let Some(hi) = &art.mouse_hilight {
                            b.with_children(|inner| {
                                inner.spawn((
                                    Hilight,
                                    Visibility::Hidden,
                                    MaterialNode(hi.clone()),
                                    abs(s, 10.0, 10.0, 30.0, 30.0),
                                ));
                            });
                        }
                    }
                    (None, _) => {
                        b.insert((FallbackFace, BackgroundColor(BTN_BG)));
                        b.with_children(|inner| {
                            inner.spawn((
                                Text::new(if flip { "<" } else { ">" }),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 16.0 * s,
                                    ..default()
                                },
                                TextColor(GOLD),
                            ));
                        });
                    }
                }
            }
        });
}

pub(super) fn exit_create(
    mut commands: Commands,
    roots: Query<Entity, With<CharCreateUi>>,
    mut preview: ResMut<GluePreview>,
    // A paid service ends with the screen, and its dialog does not follow (the tree is despawned
    // here, as select's exit does).
    (mut paid, mut dialog): (
        ResMut<super::paid::PaidChange>,
        ResMut<crate::glue::dialog::GlueDialog>,
    ),
) {
    for e in &roots {
        commands.entity(e).despawn();
    }
    paid.0 = None;
    dialog.dismiss(&mut commands);
    // CharSelect's `OnEnter` runs after this and re-establishes both the same frame.
    preview.look = None;
    preview.scene = None;
}
