use super::*;
use benilla_formats::glyphs::{GlyphProperty, GlyphSlot};
use benilla_protocol::messages::{Field, FieldLayout};

const MAJOR: u32 = 0;
const MINOR: u32 = 1;

fn tables() -> GlyphCatalog {
    GlyphCatalog::from_rows(
        [
            GlyphProperty {
                id: 251,
                spell_id: 56_224,
                type_flags: MAJOR,
                icon_id: 1,
            },
            GlyphProperty {
                id: 252,
                spell_id: 56_225,
                type_flags: MINOR,
                icon_id: 1,
            },
        ],
        [GlyphSlot {
            id: 21,
            type_flags: MAJOR,
            order: 0,
        }],
    )
}

fn glyph_spell(glyph: i32) -> SpellDisplay {
    SpellDisplay {
        effects: [APPLY_GLYPH, 0, 0],
        effect_misc_value: [glyph, 0, 0],
        ..Default::default()
    }
}

/// Socket 0 offers major slot 21; `held` is socket 0's glyph.
fn player(enabled: u32, held: u32) -> ObjectFields {
    ObjectFields::from_fields(
        FieldLayout::Wrath12340,
        &[
            (Field::PlayerGlyphsEnabled, enabled),
            (Field::PlayerGlyphSlots1, 21),
            (Field::PlayerGlyphs1, held),
        ],
    )
}

#[test]
fn a_glyph_needs_an_unlocked_socket_of_its_type_and_no_twin() {
    let t = tables();
    assert_eq!(check(0, &glyph_spell(251), &player(1, 0), &t), Ok(()));
    assert_eq!(
        check(0, &glyph_spell(251), &player(0, 0), &t),
        Err(SOCKET_LOCKED)
    );
    assert_eq!(
        check(6, &glyph_spell(251), &player(!0, 0), &t),
        Err(SOCKET_LOCKED)
    );
    assert_eq!(
        check(0, &glyph_spell(252), &player(1, 0), &t),
        Err(INVALID_GLYPH),
        "a minor glyph in a major socket"
    );
    assert_eq!(
        check(0, &glyph_spell(251), &player(1, 251), &t),
        Err(UNIQUE_GLYPH)
    );
    assert_eq!(
        check(0, &SpellDisplay::default(), &player(1, 0), &t),
        Err(INVALID_GLYPH),
        "no glyph effect"
    );
}
