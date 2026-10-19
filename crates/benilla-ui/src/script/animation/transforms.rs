//! Render-only animation transforms: layout, anchors and hit rectangles remain unchanged.
use super::*;
use crate::order::ZTarget;

#[derive(Clone, Debug)]
pub(crate) struct Transform {
    pub origin: String,
    pub offset: (f32, f32),
    pub scale: (f32, f32),
    pub radians: f32,
}
fn target(lua: &Lua, owner: &Table) -> mlua::Result<ZTarget> {
    let id = super::super::object::decode_id(owner)?;
    let m = lua.app_data_ref::<Model>().expect("model");
    if let Some(&h) = m.id_to_frame.get(&id) {
        return Ok(ZTarget::Frame(h));
    }
    m.id_to_region
        .get(&id)
        .copied()
        .map(ZTarget::Region)
        .ok_or_else(|| mlua::Error::runtime("stale animation owner"))
}
pub(super) fn install(lua: &Lua, t: &Table, kind: &str) -> mlua::Result<()> {
    if !matches!(kind, "Scale" | "Rotation") {
        return Ok(());
    }
    t.raw_set("_origin", "CENTER")?;
    t.raw_set("_origin_x", 0.0)?;
    t.raw_set("_origin_y", 0.0)?;
    t.set(
        "SetOrigin",
        lua.create_function(|_, (t, point, x, y): (Table, String, f64, f64)| {
            if !matches!(
                point.as_str(),
                "TOPLEFT"
                    | "TOP"
                    | "TOPRIGHT"
                    | "LEFT"
                    | "CENTER"
                    | "RIGHT"
                    | "BOTTOMLEFT"
                    | "BOTTOM"
                    | "BOTTOMRIGHT"
            ) || !x.is_finite()
                || !y.is_finite()
            {
                return Err(mlua::Error::runtime("invalid animation origin"));
            }
            t.raw_set("_origin", point)?;
            t.raw_set("_origin_x", x)?;
            t.raw_set("_origin_y", y)
        })?,
    )?;
    t.set(
        "GetOrigin",
        lua.create_function(|_, t: Table| {
            Ok((
                t.raw_get::<String>("_origin")?,
                t.raw_get::<f64>("_origin_x")?,
                t.raw_get::<f64>("_origin_y")?,
            ))
        })?,
    )?;
    if kind == "Scale" {
        t.raw_set("_scale_x", 1.0)?;
        t.raw_set("_scale_y", 1.0)?;
        t.set(
            "SetScale",
            lua.create_function(|_, (t, x, y): (Table, f64, f64)| {
                if !x.is_finite() || !y.is_finite() {
                    return Err(mlua::Error::runtime("invalid animation scale"));
                }
                t.raw_set("_scale_x", x)?;
                t.raw_set("_scale_y", y)
            })?,
        )?;
        t.set(
            "GetScale",
            lua.create_function(|_, t: Table| {
                Ok((t.raw_get::<f64>("_scale_x")?, t.raw_get::<f64>("_scale_y")?))
            })?,
        )?;
    } else {
        scalar(lua, t, "_radians", "Radians", 0.0)?;
        t.set(
            "SetDegrees",
            lua.create_function(|_, (t, v): (Table, f64)| {
                if !v.is_finite() {
                    return Err(mlua::Error::runtime("invalid animation angle"));
                }
                t.raw_set("_radians", v.to_radians())
            })?,
        )?;
        t.set(
            "GetDegrees",
            lua.create_function(|_, t: Table| Ok(t.raw_get::<f64>("_radians")?.to_degrees()))?,
        )?;
    }
    Ok(())
}
pub(super) fn clear(lua: &Lua, g: &Table) {
    lua.app_data_mut::<Model>()
        .expect("model")
        .animation_transforms
        .remove(&(g.to_pointer() as usize));
}
pub(super) fn apply(lua: &Lua, g: &Table, animations: &[Table]) -> mlua::Result<()> {
    let mut ops = Vec::new();
    for a in animations {
        let kind: String = a.raw_get("_kind")?;
        if !matches!(kind.as_str(), "Scale" | "Rotation") {
            continue;
        }
        let p = a.raw_get::<f64>("_smooth_progress")? as f32;
        ops.push(Transform {
            origin: a.raw_get("_origin")?,
            offset: (a.raw_get("_origin_x")?, a.raw_get("_origin_y")?),
            scale: if kind == "Scale" {
                (
                    1.0 + (a.raw_get::<f32>("_scale_x")? - 1.0) * p,
                    1.0 + (a.raw_get::<f32>("_scale_y")? - 1.0) * p,
                )
            } else {
                (1.0, 1.0)
            },
            radians: if kind == "Rotation" {
                a.raw_get::<f32>("_radians")? * p
            } else {
                0.0
            },
        });
    }
    if !ops.is_empty() {
        let owner = target(lua, &g.raw_get::<Table>("_parent")?)?;
        lua.app_data_mut::<Model>()
            .expect("model")
            .animation_transforms
            .insert(g.to_pointer() as usize, (owner, ops));
    }
    Ok(())
}

pub(crate) fn extract(model: &Model, out: &mut [super::super::ExtractedQuad]) {
    for q in out {
        let mut chain = vec![q.target];
        let mut frame = match q.target {
            ZTarget::Frame(f) => model.arena.frame(f).and_then(|f| f.parent),
            ZTarget::Region(r) => model.arena.region(r).map(|r| r.owner),
        };
        while let Some(f) = frame {
            chain.push(ZTarget::Frame(f));
            frame = model.arena.frame(f).and_then(|f| f.parent);
        }
        let mut matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let mut changed = false;
        for target in chain {
            let rect = match target {
                ZTarget::Frame(f) => model.resolved.get(&f),
                ZTarget::Region(r) => model.region_resolved.get(&r),
            };
            let Some(r) = rect else { continue };
            for (owner, ops) in model.animation_transforms.values() {
                if *owner != target {
                    continue;
                }
                for op in ops {
                    let x = if op.origin.contains("LEFT") {
                        r.left
                    } else if op.origin.contains("RIGHT") {
                        r.right
                    } else {
                        (r.left + r.right) / 2.0
                    } + op.offset.0;
                    let y = if op.origin.contains("TOP") {
                        r.top
                    } else if op.origin.contains("BOTTOM") {
                        r.bottom
                    } else {
                        (r.bottom + r.top) / 2.0
                    } + op.offset.1;
                    let (sin, cos) = op.radians.sin_cos();
                    let (a, b, c, d) = (
                        cos * op.scale.0,
                        sin * op.scale.0,
                        -sin * op.scale.1,
                        cos * op.scale.1,
                    );
                    let (tx, ty) = (x - a * x - c * y, y - b * x - d * y);
                    let [e, f, g, h, i, j] = matrix;
                    matrix = [
                        a * e + c * f,
                        b * e + d * f,
                        a * g + c * h,
                        b * g + d * h,
                        a * i + c * j + tx,
                        b * i + d * j + ty,
                    ];
                    changed = true;
                }
            }
        }
        q.animation_transform = changed.then_some(matrix);
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::{ScriptDialect, UiScript};
    #[test]
    fn scale_rotation_render_only_pause_bounce_and_restore() {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        s.run(r#"f=CreateFrame('Frame','TransformTest'); f:SetSize(100,40); f:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',10,20)
            tex=f:CreateTexture(); tex:SetAllPoints(f); tex:SetTexture(1,1,1)
            g=f:CreateAnimationGroup(); g:SetLooping('BOUNCE')
            a=g:CreateAnimation('Scale'); a:SetScale(3,2); a:SetDuration(1); a:SetOrigin('BOTTOMLEFT',0,0)
            r=g:CreateAnimation('Rotation'); r:SetDegrees(180); r:SetDuration(1); r:SetOrigin('BOTTOMLEFT',0,0)
            g:Play()"#).unwrap();
        s.tick(0.5);
        s.resolve();
        let q = s
            .extract()
            .into_iter()
            .find(|q| q.animation_transform.is_some())
            .unwrap();
        let m = q.animation_transform.unwrap();
        assert!(m[0].abs() < 0.0001 && (m[1] - 2.0).abs() < 0.0001);
        assert!((m[2] + 1.5).abs() < 0.0001);
        assert_eq!(s.eval::<f64>("return f:GetWidth()").unwrap(), 100.0);
        assert_eq!(s.eval::<f64>("return f:GetScale()").unwrap(), 1.0);
        s.run("g:Pause()").unwrap();
        s.tick(0.2);
        assert_eq!(
            s.extract()
                .into_iter()
                .find(|q| q.animation_transform.is_some())
                .unwrap()
                .animation_transform,
            Some(m)
        );
        s.run("g:Play()").unwrap();
        s.tick(1.0);
        let bounce = s
            .extract()
            .into_iter()
            .find(|q| q.animation_transform.is_some())
            .unwrap()
            .animation_transform
            .unwrap();
        for (x, y) in m.into_iter().zip(bounce) {
            assert!((x - y).abs() < 0.001);
        }
        s.run("g:Stop()").unwrap();
        assert!(s.extract().iter().all(|q| q.animation_transform.is_none()));
        s.run("assert(r:GetDegrees()==180); assert(math.abs(r:GetRadians()-math.pi)<0.000001); assert(a:GetScale()==3)").unwrap();
    }
}
