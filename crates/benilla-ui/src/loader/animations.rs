//! Installed Wrath XML animations are constructed before the owner's OnLoad.
use super::{children_named, Loader};
use crate::framexml::{self, Element};
use mlua::{ObjectLike, Table};
impl Loader<'_> {
    pub(super) fn apply_animations(
        &mut self,
        el: &Element,
        owner: &Table,
        parent_name: &str,
        dbg: &str,
    ) {
        if self
            .lua()
            .app_data_ref::<crate::script::Model>()
            .expect("model")
            .dialect
            != crate::script::ScriptDialect::Wrath12340
        {
            return;
        }
        for container in children_named(el, "Animations") {
            for group in children_named(container, "AnimationGroup") {
                let name = group.name().map(|n| framexml::resolve_name(n, parent_name));
                let result = owner.call_method::<Table>("CreateAnimationGroup", name);
                let g = match result {
                    Ok(g) => g,
                    Err(e) => {
                        self.report
                            .errors
                            .push(format!("{dbg}: AnimationGroup: {e}"));
                        continue;
                    }
                };
                self.apply_parent_key(group, owner, &g);
                if let Some(looping) = group.attr("looping") {
                    self.call(&g, "SetLooping", looping.to_string(), dbg);
                }
                for anim in &group.children {
                    if anim.tag == "Scripts" {
                        continue;
                    }
                    let name = anim.name().map(|n| framexml::resolve_name(n, parent_name));
                    let a =
                        match g.call_method::<Table>("CreateAnimation", (anim.tag.clone(), name)) {
                            Ok(a) => a,
                            Err(e) => {
                                self.report.errors.push(format!("{dbg}: animation: {e}"));
                                continue;
                            }
                        };
                    self.apply_parent_key(anim, &g, &a);
                    for (attr, method) in [
                        ("duration", "SetDuration"),
                        ("startDelay", "SetStartDelay"),
                        ("endDelay", "SetEndDelay"),
                        ("order", "SetOrder"),
                        ("change", "SetChange"),
                    ] {
                        if let Some(value) = anim.attr(attr).and_then(|v| v.parse::<f64>().ok()) {
                            self.call(&a, method, value, dbg);
                        }
                    }
                    if anim.tag == "Translation" {
                        let x = anim
                            .attr("offsetX")
                            .and_then(|v| v.parse::<f64>().ok())
                            .unwrap_or(0.0);
                        let y = anim
                            .attr("offsetY")
                            .and_then(|v| v.parse::<f64>().ok())
                            .unwrap_or(0.0);
                        self.call(&a, "SetOffset", (x, y), dbg);
                    }
                    if anim.tag == "Scale" {
                        let x = anim
                            .attr("scaleX")
                            .and_then(|v| v.parse::<f64>().ok())
                            .unwrap_or(1.0);
                        let y = anim
                            .attr("scaleY")
                            .and_then(|v| v.parse::<f64>().ok())
                            .unwrap_or(1.0);
                        self.call(&a, "SetScale", (x, y), dbg);
                    }
                    if anim.tag == "Rotation" {
                        if let Some(v) = anim.attr("degrees").and_then(|v| v.parse::<f64>().ok()) {
                            self.call(&a, "SetDegrees", v, dbg);
                        } else if let Some(v) =
                            anim.attr("radians").and_then(|v| v.parse::<f64>().ok())
                        {
                            self.call(&a, "SetRadians", v, dbg);
                        }
                    }
                    for origin in children_named(anim, "Origin") {
                        let point = origin.attr("point").unwrap_or("CENTER").to_string();
                        let x = origin
                            .attr("x")
                            .and_then(|v| v.parse::<f64>().ok())
                            .unwrap_or(0.0);
                        let y = origin
                            .attr("y")
                            .and_then(|v| v.parse::<f64>().ok())
                            .unwrap_or(0.0);
                        self.call(&a, "SetOrigin", (point, x, y), dbg);
                    }
                    if let Some(value) = anim.attr("smoothing") {
                        self.call(&a, "SetSmoothing", value.to_string(), dbg);
                    }
                    if let Some(onload) = self.apply_scripts(anim, &a, dbg) {
                        if let Err(e) = onload.call::<()>(a.clone()) {
                            self.report
                                .errors
                                .push(format!("{dbg}: animation OnLoad: {e}"));
                        }
                    }
                }
                if let Some(onload) = self.apply_scripts(group, &g, dbg) {
                    if let Err(e) = onload.call::<()>(g.clone()) {
                        self.report
                            .errors
                            .push(format!("{dbg}: AnimationGroup OnLoad: {e}"));
                    }
                }
            }
        }
    }
}
