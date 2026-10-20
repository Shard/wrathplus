use mlua::{Function, ObjectLike, Table};

use crate::framexml::Element;

use super::{children_named, Loader};

impl Loader<'_> {
    /// `<Scripts>` (`0x769ef0`): `SetScript` for each handler. Returns the `OnLoad`, which the
    /// caller fires bottom-up.
    pub(super) fn apply_scripts(
        &mut self,
        el: &Element,
        wrapper: &Table,
        dbg: &str,
    ) -> Option<Function> {
        let mut onload = None;
        for scripts in children_named(el, "Scripts") {
            let owner = wrapper
                .call_method::<Option<String>>("GetName", ())
                .ok()
                .flatten()
                .unwrap_or_else(|| "<unnamed>".to_string());
            for handler in &scripts.children {
                let name = handler.tag.clone();
                // The type's script lookup runs first (`0x48fef0`): a name it lacks is logged and
                // skipped, its body never compiled. Labelled: an animation's line uses the second
                // format (`"%s %s: Unknown script element %s"`, `0x5ea608`) with its object type.
                if matches!(
                    wrapper.call_method::<mlua::Value>("HasScript", name.clone()),
                    Ok(mlua::Value::Nil | mlua::Value::Boolean(false)) | Err(_)
                ) {
                    let what = if el.tag.eq_ignore_ascii_case("AnimationGroup")
                        || ANIMATION_TAGS.iter().any(|t| el.tag.eq_ignore_ascii_case(t))
                    {
                        el.tag.clone()
                    } else {
                        "Frame".to_string()
                    };
                    self.report
                        .warnings
                        .push(format!("{what} {owner}: Unknown script element {name}"));
                    continue;
                }
                // An empty body clears the handler to nil, but a whitespace one is an empty
                // function: `SetScript` (`0x7025c0`) tests only NULL and the first byte
                // (`0x7025f8`), and nothing trims the body (`0x6f29d0`). The stock blanks
                // (`BuffFrame.xml:101`) are whitespace, so their `GetScript` answers a function.
                let named = handler
                    .attr("function")
                    // `function=""` names no handler: it clears, like an empty body.
                    .filter(|global| !global.is_empty());
                let cleared = handler.body.is_empty() && named.is_none();
                let func = if let Some(global) = named {
                    match self.lua().globals().get::<Function>(global) {
                        Ok(function) => Some(function),
                        Err(e) => {
                            self.report.errors.push(format!(
                                "{dbg}: resolving <{name} function=\"{global}\">: {e}"
                            ));
                            continue;
                        }
                    }
                } else if cleared {
                    None
                } else {
                    match self.compile_handler(handler, &el.tag, &name, dbg) {
                        Some(f) => Some(f),
                        None => continue,
                    }
                };
                if let Err(e) = wrapper.call_method::<()>("SetScript", (name.clone(), func.clone()))
                {
                    self.report.errors.push(format!("{dbg}: SetScript(\"{name}\"): {e}"));
                    continue;
                }
                // After each `SetScript` the XML walker enables the handler's input kind
                // (`0x769ef0` → `0x76af00`): these five arm the mouse (`OnDragStop` and
                // `OnReceiveDrag` do not), `OnMouseWheel` the wheel (kind 3). The Lua `SetScript`
                // never does (`0x7748d0`).
                const MOUSE_KIND: [&str; 5] = [
                    "OnEnter",
                    "OnLeave",
                    "OnMouseDown",
                    "OnMouseUp",
                    "OnDragStart",
                ];
                if MOUSE_KIND.iter().any(|k| name.eq_ignore_ascii_case(k)) {
                    self.call(wrapper, "EnableMouse", true, dbg);
                }
                if name.eq_ignore_ascii_case("OnMouseWheel") {
                    self.call(wrapper, "EnableMouseWheel", true, dbg);
                }
                // The key handlers, the same rule (`OnChar` kind 0, `OnKeyDown`/`OnKeyUp` kind 1);
                // here any of them sets the one flag both key walks read.
                const KEY_KIND: [&str; 3] = ["OnChar", "OnKeyDown", "OnKeyUp"];
                if KEY_KIND.iter().any(|k| name.eq_ignore_ascii_case(k)) {
                    self.call(wrapper, "EnableKeyboard", true, dbg);
                }
                if name.eq_ignore_ascii_case("OnLoad") {
                    // A cleared `OnLoad` also unsets a template's, or the caller would fire it.
                    onload = func;
                }
            }
        }
        onload
    }

    /// Compile a handler body as build 12340 does: inside its script's formals
    /// ([`native_formals`]) on one line, in a chunk named `*:<script>`.
    pub(super) fn compile_handler(
        &mut self,
        handler: &Element,
        tag: &str,
        name: &str,
        dbg: &str,
    ) -> Option<Function> {
        // The raw body, never trimmed: a whitespace body is a real empty chunk (`apply_scripts`).
        let body = handler.body.as_str();
        if body.is_empty() {
            return None;
        }
        // `0x48fef0` names every XML handler chunk `*:<script>` (`"*:%s"`), and the one-line
        // wrapper keeps an error's line number the body's own.
        let chunk_name = format!("*:{name}");
        let body = format!("return function({}) {body} end", native_formals(tag, name));
        let compiled = self
            .lua()
            .load(&body)
            .set_name(chunk_name)
            .set_mode(mlua::ChunkMode::Text)
            .into_function();
        match compiled {
            Ok(chunk) => match chunk.call::<Function>(()) {
                Ok(f) => Some(f),
                Err(e) => {
                    self.report
                        .errors
                        .push(format!("{dbg}: compiling <{name}>: {e}"));
                    None
                }
            },
            Err(e) => {
                self.report
                    .errors
                    .push(format!("{dbg}: compiling <{name}>: {e}"));
                None
            }
        }
    }

    /// Fire a captured `OnLoad` under the event path's convention, `this` set and restored
    /// (`0x704d50`); an error is recorded, never propagated.
    pub(super) fn fire_onload(&mut self, wrapper: &Table, func: &Function, dbg: &str) {
        if let Err(e) = self.invoke_handler(wrapper, func) {
            self.report.errors.push(format!("{dbg}: OnLoad: {e}"));
        }
    }

    /// Call a frame-wrapper method, recording (not propagating) any error.
    pub(super) fn call(
        &mut self,
        wrapper: &Table,
        method: &str,
        args: impl mlua::IntoLuaMulti,
        dbg: &str,
    ) {
        if let Err(e) = wrapper.call_method::<()>(method, args) {
            self.report.errors.push(format!("{dbg}: {method}: {e}"));
        }
    }

    /// Call a region-wrapper method, recording (not propagating) any error.
    pub(super) fn call_region(
        &mut self,
        region: &Table,
        method: &str,
        args: impl mlua::IntoLuaMulti,
        dbg: &str,
    ) {
        if let Err(e) = region.call_method::<()>(method, args) {
            self.report
                .errors
                .push(format!("{dbg}: region {method}: {e}"));
        }
    }
}

/// The XML element types that make an `Animation`.
const ANIMATION_TAGS: [&str; 6] = ["Animation", "Alpha", "Path", "Rotation", "Scale", "Translation"];

/// Build 12340's formals for script `name` on an element of XML type `tag`: each script lookup
/// hands the compiler its `return function(<formals>) %s end`, else the default `self`
/// (`0x48fef0`). The lookups: the frame base `0x48e680` and `OnEvent` `0x816830`, Button
/// `0x96f200`, EditBox `0x9628f0`, the hyperlink trio `0x978610`, MovieFrame `0x95db80`, GameTooltip
/// `0x61a280`, Slider `0x96bca0` and StatusBar `0x961630`, ScrollFrame `0x96b0b0`, ColorSelect
/// `0x96dc50`, AnimationGroup `0x497800` and Animation `0x497500`.
fn native_formals(tag: &str, name: &str) -> &'static str {
    let is = |script: &str| name.eq_ignore_ascii_case(script);
    let group = tag.eq_ignore_ascii_case("AnimationGroup");
    if group || ANIMATION_TAGS.iter().any(|a| tag.eq_ignore_ascii_case(a)) {
        return if is("OnEvent") {
            "self,event,..."
        } else if is("OnStop") || (group && is("OnFinished")) {
            "self,requested"
        } else if is("OnUpdate") {
            "self,elapsed"
        } else if group && is("OnLoop") {
            "self,loopState"
        } else {
            "self"
        };
    }
    const TABLE: [(&str, &str); 32] = [
        ("OnEvent", "self,event,..."),
        ("OnSizeChanged", "self,w,h"),
        ("OnUpdate", "self,elapsed"),
        ("OnEnter", "self,motion"),
        ("OnLeave", "self,motion"),
        ("OnMouseDown", "self,button"),
        ("OnMouseUp", "self,button"),
        ("OnMouseWheel", "self,delta"),
        ("OnDragStart", "self,button"),
        ("OnChar", "self,text"),
        ("OnKeyDown", "self,key"),
        ("OnKeyUp", "self,key"),
        ("OnAttributeChanged", "self,name,value"),
        ("PreClick", "self,button,down"),
        ("OnClick", "self,button,down"),
        ("PostClick", "self,button,down"),
        ("OnDoubleClick", "self,button"),
        ("OnTextChanged", "self, userInput"),
        ("OnCursorChanged", "self,x,y,w,h"),
        ("OnInputLanguageChanged", "self,language"),
        ("OnCharComposition", "self,text"),
        ("OnHyperlinkEnter", "self,link,text"),
        ("OnHyperlinkLeave", "self,link,text"),
        ("OnHyperlinkClick", "self,link,text,button"),
        ("OnMovieShowSubtitle", "self,text"),
        ("OnTooltipAddMoney", "self,cost,maxcost"),
        ("OnValueChanged", "self,value"),
        ("OnMinMaxChanged", "self,min,max"),
        ("OnHorizontalScroll", "self,offset"),
        ("OnVerticalScroll", "self,offset"),
        ("OnScrollRangeChanged", "self,xrange,yrange"),
        ("OnColorSelect", "self,r,g,b"),
    ];
    TABLE
        .iter()
        .find(|(script, _)| is(script))
        .map_or("self", |(_, formals)| formals)
}
