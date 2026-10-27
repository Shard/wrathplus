use mlua::{ObjectLike, Table};

use crate::framexml::{self, Element};

use super::regions::FontAttrs;
use super::{abs_dim, children_named, Loader};

impl Loader<'_> {
    /// A frame element's LoadXML attributes and simple children (`0x769820`), and a model pane's
    /// own (`0x76cac0`).
    pub(super) fn apply_attrs(&mut self, el: &Element, wrapper: &Table, dbg: &str) {
        // UIParent.xml supplies panel offsets here; inherited templates use the same path.
        // Trusted Wrath attributes are ordinary frame state, not secure execution.
        for attrs in children_named(el, "Attributes") {
            for attr in children_named(attrs, "Attribute") {
                let Some(name) = attr.attr("name") else { continue };
                let text = attr.attr("value").unwrap_or("");
                let value = match attr.attr("type").unwrap_or("string") {
                    "string" => self.lua().create_string(text).map(mlua::Value::String),
                    "number" => text.parse::<f64>().map(mlua::Value::Number)
                        .map_err(mlua::Error::external),
                    "boolean" => Ok(mlua::Value::Boolean(text == "true")),
                    kind => Err(mlua::Error::external(format!("unknown attribute type {kind}"))),
                };
                match value {
                    Ok(value) => self.call(wrapper, "SetAttribute", (name.to_string(), value), dbg),
                    Err(e) => self.report.errors.push(format!("{dbg}: <Attribute> {name}: {e}")),
                }
            }
        }
        if el.attr_bool("hidden") {
            self.call(wrapper, "Hide", (), dbg);
        }
        // An unknown `frameStrata=` is logged (`0x7699a4`) and skipped, not raised as the Lua
        // binding would (`0x774360`): `SetFrameStrata` runs only on a hit (`0x769971`), so the
        // frame keeps the strata it had.
        if let Some(strata) = el.attr("frameStrata") {
            if crate::script::object::strata_from_str(strata).is_some() {
                self.call(wrapper, "SetFrameStrata", strata.to_string(), dbg);
            } else {
                self.report
                    .warnings
                    .push(format!("{dbg}: Unknown frame strata: {strata}"));
            }
        }
        if let Some(level) = el.attr("frameLevel") {
            if let Ok(n) = level.parse::<i64>() {
                self.call(wrapper, "SetFrameLevel", n, dbg);
            }
        }
        if let Some(alpha) = el.attr("alpha") {
            if let Ok(a) = alpha.parse::<f32>() {
                self.call(wrapper, "SetAlpha", a, dbg);
            }
        }

        // A model pane's `file=` is `SetModel` (`0x76cac0`).
        let model_kind = super::model_kind_tag(&el.tag);
        if model_kind {
            if let Some(file) = el.attr("file") {
                let text = self.resolve_text(file, dbg);
                self.call(wrapper, "SetModel", text, dbg);
            }
        }
        // A model pane's `scale=` is the model's scale, the field `SetModelScale` writes
        // (`0x76cb61`); a value of 0 or less is refused, never clamped (`0x76cb92`). A plain
        // frame's `scale=` is ignored; whether `0x769820` reads one is untraced.
        if let Some(scale) = el.attr("scale") {
            if model_kind {
                match scale.trim().parse::<f32>() {
                    Ok(s) if s > 0.0 => self.call(wrapper, "SetModelScale", s, dbg),
                    _ => self.warn_once(
                        &format!("model-scale:{dbg}"),
                        format!("Frame {dbg}: Invalid model scale: {scale}"),
                    ),
                }
            }
        }
        // A model pane's fog: `fogNear`/`fogFar` clamp at 0 here only (`0x76cbbb`, `0x76cbf3`),
        // where the Lua setters store raw; `<FogColor>` is `SetFogColor`, fog bit included.
        if model_kind {
            for (attr, verb) in [("fogNear", "SetFogNear"), ("fogFar", "SetFogFar")] {
                if let Some(v) = el.attr(attr).and_then(|v| v.trim().parse::<f32>().ok()) {
                    self.call(wrapper, verb, v.max(0.0), dbg);
                }
            }
            if let Some(fc) = children_named(el, "FogColor").next() {
                let ch = |k: &str, d: f32| {
                    fc.attr(k)
                        .and_then(|v| v.trim().parse::<f32>().ok())
                        .unwrap_or(d)
                };
                self.call(
                    wrapper,
                    "SetFogColor",
                    (ch("r", 0.0), ch("g", 0.0), ch("b", 0.0), ch("a", 1.0)),
                    dbg,
                );
            }
        }
        if let Some(id) = el.attr("id") {
            if let Ok(n) = id.parse::<i64>() {
                self.call(wrapper, "SetID", n, dbg);
            }
        }
        // `clampedToScreen`: the layout's screen clamp, geometry flag bit 4 (`0x768cc0`).
        if el.attr_bool("clampedToScreen") {
            self.call(wrapper, "SetClampedToScreen", true, dbg);
        }
        if el.attr_bool("enableMouse") {
            self.call(wrapper, "EnableMouse", true, dbg);
        }
        // `<HitRectInsets>`: the mouse rect, inset from the frame's; a side it omits is 0.
        if let Some(ins) = children_named(el, "HitRectInsets").next() {
            let src = children_named(ins, "AbsInset").next().unwrap_or(ins);
            let side = |k: &str| {
                src.attr(k)
                    .and_then(|v| v.trim().parse::<f32>().ok())
                    .unwrap_or(0.0)
            };
            self.call(
                wrapper,
                "SetHitRectInsets",
                (side("left"), side("right"), side("top"), side("bottom")),
                dbg,
            );
        }
        // `<ResizeBounds>` (`0x769baa`): both pairs are always written, an absent one as 0, so a
        // lone `<minResize>` resets the max to unbounded. A `<Size>` outside them is not clamped.
        if let Some(rb) = children_named(el, "ResizeBounds").next() {
            for (tag, verb) in [("minResize", "SetMinResize"), ("maxResize", "SetMaxResize")] {
                let (x, y) = children_named(rb, tag).next().map_or((None, None), abs_dim);
                self.call(wrapper, verb, (x.unwrap_or(0.0), y.unwrap_or(0.0)), dbg);
            }
        }
        // `movable`/`resizable`: flag bits 0x100/0x200, via the setter Lua uses (`0x76a3c0`).
        if el.attr_bool("movable") {
            self.call(wrapper, "SetMovable", true, dbg);
        }
        if el.attr_bool("resizable") {
            self.call(wrapper, "SetResizable", true, dbg);
        }
        // `toplevel`: bit 0x1 of the same flag word (`0x7698ec`).
        if el.attr_bool("toplevel") {
            self.call(wrapper, "SetToplevel", true, dbg);
        }
        // `enableKeyboard` enables both key kinds (`0x769ae8`, `0x769af3`).
        if el.attr_bool("enableKeyboard") {
            self.call(wrapper, "EnableKeyboard", true, dbg);
        }
    }

    /// `<TitleRegion>`: the drag handle, built through the idempotent `CreateTitleRegion` so the
    /// element and a later Lua call name one object (`0x769b2a` takes `0x773910`'s path).
    pub(super) fn apply_title_region(
        &mut self,
        el: &Element,
        wrapper: &Table,
        self_name: &str,
        dbg: &str,
    ) {
        let Some(tr) = children_named(el, "TitleRegion").next() else {
            return;
        };
        let region: Table = match wrapper.call_method("CreateTitleRegion", ()) {
            Ok(r) => r,
            Err(e) => {
                self.report
                    .errors
                    .push(format!("{dbg}: CreateTitleRegion: {e}"));
                return;
            }
        };
        self.apply_region_layout(tr, &region, self_name, dbg, FontAttrs::Own);
    }

    /// `<Size>` (`0x767800`). Every one applies in document order, and expansion puts an
    /// instance's children after its template's, so the instance's own `<Size>` wins.
    pub(super) fn apply_size(&mut self, el: &Element, wrapper: &Table, dbg: &str) {
        for size in children_named(el, "Size") {
            let (x, y) = abs_dim(size);
            if let Some(w) = x {
                self.call(wrapper, "SetWidth", w, dbg);
            }
            if let Some(h) = y {
                self.call(wrapper, "SetHeight", h, dbg);
            }
        }
    }

    /// `<Anchors>` (`0x767800`), after the `setAllPoints` shorthand: `relativePoint` defaults to
    /// `point`, and `relativeTo` expands `$parent` to the nearest named ancestor's name, else
    /// `Top` (`0x76c5b0`).
    pub(super) fn apply_anchors(
        &mut self,
        el: &Element,
        wrapper: &Table,
        parent_name: &str,
        dbg: &str,
    ) {
        if el.attr_bool("setAllPoints") {
            self.call(wrapper, "SetAllPoints", (), dbg);
        }
        for anchors in children_named(el, "Anchors") {
            for anchor in children_named(anchors, "Anchor") {
                let Some(point) = anchor.attr("point") else {
                    self.report
                        .warnings
                        .push(format!("{dbg}: <Anchor> without a point; skipped"));
                    continue;
                };
                let rel_point = anchor.attr("relativePoint").unwrap_or(point).to_string();
                let rel_to: Option<String> = anchor
                    .attr("relativeTo")
                    .map(|r| framexml::resolve_name(r, parent_name));
                let (x, y) = anchor_offset(anchor);
                let args = (
                    point.to_string(),
                    rel_to,
                    rel_point,
                    x.unwrap_or(0.0),
                    y.unwrap_or(0.0),
                );
                let d = super::DeferredAnchor {
                    wrapper: wrapper.clone(),
                    region: false,
                    args,
                    dbg: dbg.to_string(),
                };
                // Deferred while the enclosing subtree has not built its target yet.
                self.apply_anchor(d, true);
            }
        }
    }
}

/// An `<Anchor>`'s offset: its own `x`/`y` attributes, the shape 3.3.5's XML mostly uses
/// (`<Anchor point="TOP" x="-90" y="-463"/>`, `CharacterCreate.xml`), with an `<Offset>` child's
/// `<AbsDimension>` (or its own `x`/`y`) taking precedence where one is given. Labelled: the
/// precedence when both are present is inferred, no stock file writing both.
pub(super) fn anchor_offset(anchor: &Element) -> (Option<f32>, Option<f32>) {
    let (ax, ay) = abs_dim(anchor);
    let (ox, oy) = children_named(anchor, "Offset")
        .next()
        .map(abs_dim)
        .unwrap_or((None, None));
    (ox.or(ax), oy.or(ay))
}

#[cfg(test)]
mod tests {
    use crate::script::UiScript;

    /// `x`/`y` on `<Anchor>` itself offset the point, on a frame and on a region, and an
    /// `<Offset>` child still works; `CharacterCreate.xml`'s race, class and gender buttons hang
    /// off their panel this way (`<Anchor point="TOP" x="-90" y="-463"/>`).
    #[test]
    fn anchor_attributes_offset_frames_and_regions() {
        let mut s = UiScript::new().unwrap();
        s.set_screen_size(800.0, 600.0);
        let doc = crate::framexml::parse(
            r#"<Ui>
                <Frame name="Panel">
                    <Size x="400" y="500"/>
                    <Anchors><Anchor point="TOPLEFT" x="100" y="-50"/></Anchors>
                    <Layers><Layer><FontString name="$parentLabel">
                        <Anchors><Anchor point="TOP" x="10" y="-20"/></Anchors>
                    </FontString></Layer></Layers>
                    <Frames>
                        <Button name="$parentButton">
                            <Size x="38" y="38"/>
                            <Anchors><Anchor point="TOP" x="-90" y="-463"/></Anchors>
                        </Button>
                        <Button name="$parentNext">
                            <Size x="38" y="38"/>
                            <Anchors>
                                <Anchor point="LEFT" relativeTo="$parentButton" relativePoint="RIGHT" x="99">
                                    <Offset x="6" y="0"/>
                                </Anchor>
                            </Anchors>
                        </Button>
                    </Frames>
                </Frame>
            </Ui>"#,
        )
        .expect("valid FrameXML");
        let report = crate::loader::load(&s, &doc, &|_: &str| None);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        s.resolve();
        let at = |s: &UiScript, frame: &str| {
            s.eval::<(f64, f64)>(&format!("return {frame}:GetLeft(), {frame}:GetTop()"))
                .unwrap()
        };
        // Panel's TOPLEFT is (100, 550) y-up and its TOP (300, 550).
        assert_eq!(at(&s, "Panel"), (100.0, 550.0));
        assert_eq!(at(&s, "PanelButton"), (300.0 - 90.0 - 19.0, 550.0 - 463.0));
        assert_eq!(
            at(&s, "PanelNext").0,
            300.0 - 90.0 + 19.0 + 6.0,
            "<Offset> wins over x"
        );
        let label: (f64, f64) = s
            .eval("local _, _, _, x, y = PanelLabel:GetPoint() return x, y")
            .unwrap();
        assert_eq!(label, (10.0, -20.0), "a region's anchor attributes too");
    }
}
