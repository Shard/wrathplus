//! Build-12340 FontString and Texture flags the 1.12 tables lack: word wrap, indented word wrap
//! and texture tiling, behind both the Lua setters and the XML attributes the loader forwards
//! (`wordwrap=`, `indented=`, `horizTile=`, `vertTile=`).

use mlua::{Lua, Table, Value};

use super::region::region_handle_of;
use super::{Model, REG_FONTSTRING_METHODS, REG_TEXTURE_METHODS};

fn truth(v: &Value) -> bool {
    !matches!(v, Value::Nil | Value::Boolean(false))
}

fn with<T>(
    lua: &Lua,
    this: &Table,
    f: impl FnOnce(&mut super::RegionData) -> T,
) -> mlua::Result<T> {
    let rh = region_handle_of(lua, this)?;
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    model.touch_measure(rh);
    Ok(f(model.region_data.entry(rh).or_default()))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let fs: Table = lua.named_registry_value(REG_FONTSTRING_METHODS)?;
    // SetWordWrap(flag) / CanWordWrap() → 1 or nil: off draws one line.
    fs.set(
        "SetWordWrap",
        lua.create_function(|lua, (this, v): (Table, Value)| {
            with(lua, &this, |d| d.word_wrap = Some(truth(&v)))
        })?,
    )?;
    fs.set(
        "CanWordWrap",
        lua.create_function(|lua, this: Table| {
            with(lua, &this, |d| d.word_wrap.unwrap_or(true)).map(|on| on.then_some(1i64))
        })?,
    )?;
    fs.set(
        "SetIndentedWordWrap",
        lua.create_function(|lua, (this, v): (Table, Value)| {
            with(lua, &this, |d| d.indented_word_wrap = truth(&v))
        })?,
    )?;
    fs.set(
        "GetIndentedWordWrap",
        lua.create_function(|lua, this: Table| {
            with(lua, &this, |d| d.indented_word_wrap).map(|on| on.then_some(1i64))
        })?,
    )?;
    let tex: Table = lua.named_registry_value(REG_TEXTURE_METHODS)?;
    for (set, get, vertical) in [("SetHorizTile", "GetHorizTile", false), ("SetVertTile", "GetVertTile", true)] {
        tex.set(
            set,
            lua.create_function(move |lua, (this, v): (Table, Value)| {
                with(lua, &this, |d| {
                    if vertical {
                        d.vert_tile = truth(&v)
                    } else {
                        d.horiz_tile = truth(&v)
                    }
                })
            })?,
        )?;
        tex.set(
            get,
            lua.create_function(move |lua, this: Table| {
                with(lua, &this, |d| if vertical { d.vert_tile } else { d.horiz_tile })
                    .map(|on| on.then_some(1i64))
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{ScriptDialect, UiScript};

    #[test]
    fn word_wrap_indent_and_tiling_flags() {
        let s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        s.run("F = CreateFrame('Frame') S = F:CreateFontString() T = F:CreateTexture()").unwrap();
        for check in [
            "S:CanWordWrap() == 1 and S:GetIndentedWordWrap() == nil and T:GetHorizTile() == nil",
        ] {
            assert!(s.eval::<bool>(&format!("return {check}")).unwrap(), "{check}");
        }
        s.run("S:SetWordWrap(false) S:SetIndentedWordWrap(true) T:SetHorizTile(true) T:SetVertTile(1)").unwrap();
        assert!(s
            .eval::<bool>("return S:CanWordWrap() == nil and S:GetIndentedWordWrap() == 1 and T:GetHorizTile() == 1 and T:GetVertTile() == 1")
            .unwrap());
    }

    #[test]
    fn a_font_string_template_applies_through_create_font_string() {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        let doc = crate::framexml::parse(
            r#"<Ui><FontString name="TestFsTemplate" justifyH="LEFT" wordwrap="false" virtual="true"/></Ui>"#,
        )
        .unwrap();
        crate::loader::load(&s, &doc, &|_: &str| None);
        s.run("F = CreateFrame('Frame') S = F:CreateFontString(nil, 'ARTWORK', 'TestFsTemplate')").unwrap();
        assert!(s.eval::<bool>("return S:GetJustifyH() == 'LEFT' and S:CanWordWrap() == nil").unwrap());
        assert!(!s.take_warnings().iter().any(|w| w.contains("TEMPLATE")));
    }
}
