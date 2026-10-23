use super::*;

// Exact branches from installed build 12340 Wow.exe 0x57f9e1..0x57fa23.
// The UV constructor 0x583718..0x5837a7 maps cell index directly onto the 8x2 atlas.
fn check(status: u32, normal: Option<u32>, trivial: Option<u32>) {
    let uv = |i: u32| {
        Some([
            (i % 8) as f32 / 8.0,
            (i % 8 + 1) as f32 / 8.0,
            (i / 8) as f32 / 2.0,
            (i / 8 + 1) as f32 / 2.0,
        ])
    };
    assert_eq!(
        ObjectIconAtlas::Wrath.quest_cell(status, false),
        normal.and_then(uv)
    );
    assert_eq!(
        ObjectIconAtlas::Wrath.quest_cell(status, true),
        trivial.and_then(uv)
    );
}
#[test]
fn none() {
    check(0, None, None);
}
#[test]
fn unavailable() {
    check(1, None, None);
}
#[test]
fn low_available() {
    check(2, None, Some(9));
}
#[test]
fn low_reward_repeatable() {
    check(3, None, None);
}
#[test]
fn low_available_repeatable() {
    check(4, None, Some(11));
}
#[test]
fn incomplete() {
    check(5, None, None);
}
#[test]
fn reward_repeatable() {
    check(6, None, None);
}
#[test]
fn available_repeatable() {
    check(7, Some(11), Some(11));
}
#[test]
fn available() {
    check(8, Some(9), Some(9));
}
#[test]
fn reward_no_dot() {
    check(9, None, None);
}
#[test]
fn reward() {
    check(10, Some(10), Some(10));
}
#[test]
fn unknown() {
    check(255, None, None);
}

#[test]
fn raw_status_survives_single_and_multiple_packets() {
    use wrathplus_protocol::{wrath_npc as n, SessionEvent};
    for status in 0..=10 {
        for opcode in [
            n::SMSG_QUESTGIVER_STATUS,
            n::SMSG_QUESTGIVER_STATUS_MULTIPLE,
        ] {
            let mut body = Vec::new();
            if opcode == n::SMSG_QUESTGIVER_STATUS_MULTIPLE {
                body.extend(1u32.to_le_bytes());
            }
            body.extend(7u64.to_le_bytes());
            body.push(status);
            let events = n::decode(opcode, &body).unwrap();
            assert!(
                matches!(events[1], SessionEvent::WrathQuestGiverStatus { npc: 7, status: s } if s == status)
            );
        }
    }
}

#[test]
fn native_status_lifetime_matches_object_and_session_lifetime() {
    let mut q = crate::app_core::state::QuestGiver::default();
    q.set_wrath_status(7, 8);
    q.set_wrath_status(9, 10);
    q.retain_statuses(|id| id == 7);
    assert_eq!(q.wrath_statuses().get(&7), Some(&8));
    assert!(!q.wrath_statuses().contains_key(&9));
    q.clear_status(7);
    assert!(q.wrath_statuses().is_empty());
    q.set_wrath_status(7, 2);
    q.clear_session();
    assert!(q.wrath_statuses().is_empty());
}

#[test]
fn native_status_packets_emit_a_wrath_question_mark_quad() {
    use wrathplus_protocol::types::ObjectType;
    use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
    use wrathplus_protocol::{wrath_npc as n, ObjectFields};
    use bevy::ecs::system::SystemState;
    let npc = 0xf130_0001_2300_4567u64;
    for opcode in [
        n::SMSG_QUESTGIVER_STATUS,
        n::SMSG_QUESTGIVER_STATUS_MULTIPLE,
    ] {
        for (native, trivial, cell_index) in [
            (0, false, None),
            (1, false, None),
            (2, false, None),
            (2, true, Some(9)),
            (3, true, None),
            (4, false, None),
            (4, true, Some(11)),
            (5, false, None),
            (6, false, None),
            (7, false, Some(11)),
            (8, false, Some(9)),
            (9, false, None),
            (10, false, Some(10)),
        ] {
            let mut body = Vec::new();
            if opcode == n::SMSG_QUESTGIVER_STATUS_MULTIPLE {
                body.extend(1u32.to_le_bytes());
            }
            body.extend(npc.to_le_bytes());
            body.push(native);
            let WrathDispatch::Handled { session_events, .. } =
                WrathEventAdapter::new().dispatch(opcode, &body, 0).unwrap()
            else {
                panic!()
            };
            let mut app = App::new();
            // What `ui_quest::net`'s handler does with each decoded status.
            let mut giver = crate::app_core::state::QuestGiver::default();
            for ev in session_events {
                if let wrathplus_protocol::SessionEvent::WrathQuestGiverStatus { npc, status } = ev {
                    giver.set_wrath_status(npc, status);
                }
            }
            app.insert_resource(giver);
            // Native UNIT_FIELD_HEALTH=24, no Vanilla flags or projected descriptor.
            app.world_mut().spawn((
                crate::net::Guid(npc),
                crate::net::NetEntity {
                    kind: EntityKind::Unit,
                    display_id: None,
                    scale: 1.0,
                },
                GlobalTransform::IDENTITY,
                ObjectStore(
                    ObjectFields::from_fields(&[(wrathplus_protocol::types::Field::UnitHealth, 100)])
                        .into_created(ObjectType::Unit),
                ),
            ));
            let ctx = BlipCtx {
                center: Vec2::splat(100.0),
                side: 140.8,
                px_per_yd: 1.0,
                radius_yd: 70.4,
                z: 9,
                alpha: 1.0,
                wx: 0.0,
                wy: 0.0,
                wz: 0.0,
                cursor: None,
                cursor_ui: None,
                seam: 1.0,
            };
            let statuses = app
                .world()
                .resource::<crate::app_core::state::QuestGiver>()
                .wrath_statuses()
                .clone();
            let mut state = SystemState::<TrackedCandidates>::new(app.world_mut());
            let candidates = state.get(app.world());
            let mut quads = UiQuads::default();
            let mut hover = MinimapBlipHover::default();
            let icons = Handle::default();
            emit_quest_dots(
                &ctx,
                &statuses,
                &candidates,
                None,
                &icons,
                ObjectIconAtlas::from_size(256, 64),
                trivial,
                false,
                |_| false,
                &mut quads,
                &mut hover,
            );
            assert_eq!(
                quads.overlays.len(),
                usize::from(cell_index.is_some()),
                "status {native}, trivial {trivial}"
            );
            let Some(i) = cell_index else {
                continue;
            };
            let quad = &quads.overlays[0];
            assert_eq!(quad.rect.center(), ctx.center);
            assert!(
                quad.uv
                    == UvRect::from_tex_coords([
                        (i % 8) as f32 / 8.0,
                        (i % 8 + 1) as f32 / 8.0,
                        (i / 8) as f32 / 2.0,
                        (i / 8 + 1) as f32 / 2.0
                    ])
            );
            assert_eq!(quad.texture, Some(icons));
            assert_eq!(quad.color, [1.0; 4]);
        }
    }
    assert_eq!(
        ObjectIconAtlas::from_size(128, 128).cell(7),
        quest_dot_cell(7)
    );
}

#[test]
#[ignore = "requires installed Wrath data"]
fn installed_wrath_question_mark_cell_is_nonempty() {
    let data = std::env::var("WOW_DATA").expect("WOW_DATA");
    let mut chain = wrathplus_formats::Chain::open_profile(
        std::path::Path::new(&data),
        &wrathplus_formats::DataProfile::Wrath {
            locale: "enUS".into(),
        },
    )
    .unwrap();
    let (width, height, rgba) = wrathplus_formats::blp_to_rgba(
        &chain
            .read_file("Interface\\Minimap\\ObjectIcons.blp")
            .unwrap(),
    )
    .unwrap();
    assert_eq!((width, height), (256, 64));
    let atlas = ObjectIconAtlas::from_size(width, height);
    let mut crops = Vec::new();
    let mut cells = Vec::new();
    for icon in [
        ObjectIcon::TrackedResource,
        ObjectIcon::TrackedUnit,
        ObjectIcon::Party,
        ObjectIcon::CompleteQuest,
        ObjectIcon::AvailableQuest,
        ObjectIcon::RepeatableQuest,
    ] {
        let cell = atlas.icon(icon).unwrap();
        assert!(
            !cells.contains(&cell),
            "every semantic cell must be distinct"
        );
        cells.push(cell);
        let mut crop = Vec::new();
        for y in (cell[2] * height as f32) as u32..(cell[3] * height as f32) as u32 {
            for x in (cell[0] * width as f32) as u32..(cell[1] * width as f32) as u32 {
                crop.extend_from_slice(&rgba[((y * width + x) * 4) as usize..][..4]);
            }
        }
        assert!(crop.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count() > 100);
        assert!(
            !crops.contains(&crop),
            "each cell must contain distinct installed art"
        );
        // Identify top-row cells by their installed colors, not just nonempty pixels.
        let rgb =
            crop.as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 0)
                .fold([0u64; 3], |mut sum, p| {
                    for i in 0..3 {
                        sum[i] += u64::from(p[i]);
                    }
                    sum
                });
        match icon {
            ObjectIcon::Party => assert!(rgb[2] > rgb[0] && rgb[2] > rgb[1], "blue party art"),
            ObjectIcon::TrackedUnit => {
                assert!(rgb[0] > rgb[1] && rgb[0] > rgb[2], "red unit art")
            }
            ObjectIcon::RepeatableQuest => assert!(rgb[2] > rgb[0], "blue repeatable ! art"),
            ObjectIcon::TrackedResource
            | ObjectIcon::CompleteQuest
            | ObjectIcon::AvailableQuest => {
                assert!(
                    rgb[0] > rgb[2] * 3 && rgb[1] > rgb[2] * 3,
                    "gold resource/quest art"
                );
            }
        }
        if std::env::var_os("AZB_QUEST_ART_CROP").is_some() {
            eprintln!("cell {:?}", cell);
            for y in 0..32usize {
                let line: String = (0..32usize)
                    .map(|x| {
                        let p = &crop[(y * 32 + x) * 4..][..4];
                        if p[3] < 128 {
                            ' '
                        } else if p[0].max(p[1]).max(p[2]) < 100 {
                            '.'
                        } else {
                            '#'
                        }
                    })
                    .collect();
                eprintln!("{line}");
            }
        }
        crops.push(crop);
    }
    assert_eq!(atlas.cell(10), Some([0.25, 0.375, 0.5, 1.0]));
    assert_eq!(atlas.cell(5), None);
    // Corpse is POIIcons cell 7 on an 8x8 32px grid, not an ObjectIcons cell.
    let (width, height, rgba) =
        wrathplus_formats::blp_to_rgba(&chain.read_file("Interface\\Minimap\\POIIcons.blp").unwrap())
            .unwrap();
    assert_eq!((width, height), (256, 256));
    let visible = (0..32)
        .flat_map(|y| (224..256).map(move |x| (y, x)))
        .filter(|&(y, x)| rgba[((y * width + x) * 4 + 3) as usize] > 0)
        .count();
    assert!(
        visible > 100,
        "existing corpse skull UV [0.875,1,0,0.125] is nonempty"
    );
}
