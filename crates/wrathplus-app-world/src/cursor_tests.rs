use super::*;
use crate::target::{CursorKind, WorldCursor};
use bevy::ecs::system::RunSystemOnce;

const CAST_GREY: WorldCursor = WorldCursor {
    kind: CursorKind::Cast,
    unable: true,
};
const SWORD: WorldCursor = WorldCursor {
    kind: CursorKind::Attack,
    unable: false,
};

/// One frame of [`drive_displayed_cursor`] from `standing`; `lua` is what FrameXML did.
fn frame(standing: WorldCursor, world: WorldCursor, over_ui: bool, lua: &str) -> WorldCursor {
    frame_armed(standing, world, over_ui, lua, false)
}

/// One frame; `armed` parks the base at Cast.
fn frame_armed(
    standing: WorldCursor,
    world: WorldCursor,
    over_ui: bool,
    lua: &str,
    armed: bool,
) -> WorldCursor {
    frame_covered(standing, world, over_ui, lua, armed, false)
}

/// One frame; `covered` raises the loading cover.
fn frame_covered(
    standing: WorldCursor,
    world: WorldCursor,
    over_ui: bool,
    lua: &str,
    armed: bool,
    covered: bool,
) -> WorldCursor {
    let mut app = App::new();
    let script = wrathplus_ui::script::UiScript::new().unwrap();
    script.run(lua).unwrap();
    app.insert_non_send_resource(script);
    app.insert_resource(if covered {
        crate::loading_screen::LoadingScreen::test_covering()
    } else {
        crate::loading_screen::LoadingScreen::default()
    });
    app.insert_resource(world);
    app.insert_resource(crate::app_core::state::PointerOverUi(over_ui));
    app.insert_resource(DisplayedCursor(standing));
    let mut targeting = crate::spell::SpellTargeting::default();
    if armed {
        // Feed Pet's own bare item word.
        targeting.enter(6991, crate::spell::CastCommit::Spell, 0x0010);
    }
    app.insert_resource(targeting);
    app.world_mut()
        .run_system_once(drive_displayed_cursor)
        .expect("the displayed cursor drives");
    app.world().resource::<DisplayedCursor>().0
}

/// The reference parks cursor index 1 before the loading screen goes up (`0x6e49f5`).
#[test]
fn the_loading_cover_parks_the_plain_arrow_over_every_other_rule() {
    for (over_ui, lua, armed) in [
        (false, "", false),
        (true, "", true),
        (true, "SetCursor(\"CAST_CURSOR\")", true),
        (false, "", true),
    ] {
        assert_eq!(
            frame_covered(SWORD, SWORD, over_ui, lua, armed, true),
            WorldCursor::default(),
            "covered: the arrow wins (over_ui={over_ui}, armed={armed}, lua={lua:?})"
        );
    }
    // Uncovered, the same frame keeps the world's verdict.
    assert_eq!(
        frame_covered(SWORD, SWORD, false, "", false, false),
        SWORD,
        "no cover, no park"
    );
}

/// `ResetCursor` restores the base cell (`0xbe2c4c`): blue over the UI while armed, grey only
/// in the world, and no item validity verdict.
#[test]
fn an_armed_spell_parks_the_base_at_cast_so_the_ui_reads_blue() {
    const GREY: WorldCursor = CAST_GREY;
    const BLUE: WorldCursor = WorldCursor {
        kind: CursorKind::Cast,
        unable: false,
    };
    // In the world an item-only word greys.
    assert_eq!(frame_armed(BLUE, GREY, false, "", true), GREY);
    // Crossing into the UI restores the base.
    assert_eq!(frame_armed(GREY, GREY, true, "", true), BLUE);
    // A bag slot's `ResetCursor()` resolves to the base, whatever the slot holds:
    assert_eq!(frame_armed(GREY, GREY, true, "ResetCursor()", true), BLUE);
    // the blue means a spell is armed, not that the item is a legal target.
    assert_eq!(frame_armed(GREY, GREY, true, "ResetCursor()", true), BLUE);
    // Nothing armed: the base is Point.
    assert_eq!(
        frame_armed(GREY, WorldCursor::default(), true, "ResetCursor()", false),
        WorldCursor::default()
    );
}

/// A UI element that writes no cursor does not invent one.
#[test]
fn only_the_world_and_framexml_write_the_cursor() {
    // Over the world the classifier's verdict applies.
    assert_eq!(frame(CAST_GREY, SWORD, false, ""), SWORD);
    // Over the UI with nothing armed the base is Point.
    assert_eq!(frame(SWORD, SWORD, true, ""), WorldCursor::default());
    assert_eq!(
        frame(SWORD, SWORD, true, "ResetCursor()"),
        WorldCursor::default()
    );
    // A FrameXML write wins and stays, or the unit-frame lit/grey split would be stamped over.
    let lit = WorldCursor {
        kind: CursorKind::Cast,
        unable: false,
    };
    assert_eq!(
        frame(
            WorldCursor::default(),
            SWORD,
            true,
            "SetCursor(\"CAST_CURSOR\")"
        ),
        lit
    );
    assert_eq!(
        frame(lit, SWORD, true, "SetCursor(\"CAST_ERROR_CURSOR\")"),
        CAST_GREY
    );
}

/// `ShowContainerSellCursor` bails on `IsTargeting` first, so an armed spell keeps its cursor.
#[test]
fn the_sell_cursor_does_not_paint_over_an_armed_spell() {
    // The gate reads the app-fed targeting flag.
    let mut app = App::new();
    let mut script = wrathplus_ui::script::UiScript::new().unwrap();
    script.set_spell_targeting(true);
    script.set_container(
        0,
        Some(wrathplus_ui::script::ContainerState {
            name: None,
            num_slots: 16,
            slots: std::collections::HashMap::from([(
                1,
                wrathplus_ui::script::ContainerSlot::default(),
            )]),
        }),
    );
    script.run("ShowContainerSellCursor(0, 1)").unwrap();
    app.insert_non_send_resource(script);
    app.insert_resource(crate::loading_screen::LoadingScreen::default());
    app.insert_resource(CAST_GREY);
    app.insert_resource(crate::app_core::state::PointerOverUi(true));
    app.insert_resource(DisplayedCursor(CAST_GREY));
    let mut targeting = crate::spell::SpellTargeting::default();
    targeting.enter(6991, crate::spell::CastCommit::Spell, 0x0010);
    app.insert_resource(targeting);
    app.world_mut()
        .run_system_once(drive_displayed_cursor)
        .expect("the displayed cursor drives");
    assert_eq!(
        app.world().resource::<DisplayedCursor>().0,
        WorldCursor {
            kind: CursorKind::Cast,
            unable: false
        },
        "an armed spell suppresses the sell cursor entirely — the armed base shows instead"
    );
}
