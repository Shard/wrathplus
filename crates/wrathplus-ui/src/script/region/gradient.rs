//! FontString `SetAlphaGradient`, the quest text's write-on reveal, over the drawn glyphs.

use mlua::{Lua, Table};

use crate::script::Model;

use super::super::region_handle_of;

/// Install `SetAlphaGradient` into the FontString map `m`.
pub(super) fn install(lua: &Lua, m: &Table) -> mlua::Result<()> {
    // SetAlphaGradient(start, length) (12340 `0x48d0f0` → `0x6c78f0`), the quest text's write-on:
    // both truncate to integers, and the answer is whether the reveal is still inside the DRAWN
    // glyphs (`start * 4` against the laid-out vertex count): spaces, line breaks and markup have
    // no quad. A repeated pair, or a string with no layout yet, answers true; stock
    // `QuestInfo.lua:5-15` advances until false.
    m.set(
        "SetAlphaGradient",
        lua.create_function(|lua, (this, start, length): (Table, f64, f64)| {
            let rh = region_handle_of(lua, &this)?;
            let mut model = lua.app_data_mut::<Model>().expect("model");
            let data = model.region_data.entry(rh).or_default();
            let (start, length) = (start.trunc(), length.trunc());
            if start < 0.0 || length < 0.0 {
                return Ok(false);
            }
            let pair = (start as f32, length as f32);
            if data.alpha_gradient == Some(pair) {
                return Ok(true);
            }
            data.alpha_gradient = Some(pair);
            let glyphs = data.text.as_deref().map_or(0, drawn_glyphs);
            Ok(glyphs == 0 || start < glyphs as f64)
        })?,
    )?;
    Ok(())
}

/// The glyphs a FontString draws, one quad each in the reference's layout: a visible character,
/// an escaped `||` or a `|T` texture; spaces, line breaks and colour or link markup draw none.
pub(crate) fn drawn_glyphs(text: &str) -> usize {
    use crate::markup::TokenKind;
    crate::markup::tokens(text)
        .filter(|(_, t)| match t.kind {
            TokenKind::Char(c) => !c.is_whitespace(),
            TokenKind::EscapedPipe | TokenKind::Texture { .. } => true,
            _ => false,
        })
        .count()
}
