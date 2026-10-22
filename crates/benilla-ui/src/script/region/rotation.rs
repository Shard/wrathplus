//! Texture `SetRotation`, written as `SetTexCoord`'s corner form.

use mlua::{Lua, Table, Value};

use crate::script::{Model, TexCoords};

use super::region_handle_of;

/// Install `SetRotation` into the Texture map `m`.
pub(super) fn install(lua: &Lua, m: &Table) -> mlua::Result<()> {
    // SetRotation(angle [, cx, cy]) (12340 `0x48c860`, after `SetTexCoord` in the Texture table):
    // the texcoords of a unit square turned `angle` radians about (cx, cy), 0.5 each unless both
    // are numbers. With `a = angle - π/4` (the double `0x9eaf48`, stored as a float), the corners
    // are UL (cx+sin, cy-cos), LL (cx-cos, cy-sin), UR (cx+cos, cy+sin), LR (cx-sin, cy+cos), set
    // as `SetTexCoord`'s eight values (`0x481640`): at 0 they span -0.207..1.207.
    m.set(
        "SetRotation",
        lua.create_function(|lua, (this, args): (Table, mlua::Variadic<Value>)| {
            let num = |i: usize| {
                args.get(i)
                    .cloned()
                    .and_then(|v| lua.coerce_number(v).ok().flatten())
            };
            let rh = region_handle_of(lua, &this)?;
            let Some(angle) = num(0) else {
                let name = this
                    .get::<mlua::Function>("GetName")
                    .and_then(|f| f.call::<Option<String>>(this.clone()))
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "<unnamed>".to_string());
                return Err(mlua::Error::runtime(format!(
                    "Usage: {name}:SetRotation(angle, [cx, cy])"
                )));
            };
            let (cx, cy) = match (num(1), num(2)) {
                (Some(x), Some(y)) => (x as f32, y as f32),
                _ => (0.5, 0.5),
            };
            let a = f64::from((angle - 0.785_398_185_253_143) as f32);
            let (s, c) = (a.sin(), a.cos());
            let (cx, cy) = (f64::from(cx), f64::from(cy));
            let at = |x: f64, y: f64| [x as f32, y as f32];
            // Stored as TL, TR, BR, BL, as `SetTexCoord`'s corner form stores UL, UR, LR, LL.
            let coords = TexCoords::Corners([
                at(cx + s, cy - c),
                at(cx + c, cy + s),
                at(cx - s, cy + c),
                at(cx - c, cy - s),
            ]);
            lua.app_data_mut::<Model>()
                .expect("model")
                .region_data
                .entry(rh)
                .or_default()
                .tex_coords = Some(coords);
            Ok(())
        })?,
    )?;
    Ok(())
}
