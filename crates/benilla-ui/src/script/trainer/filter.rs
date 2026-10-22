//! The service-state filter behind `GetTrainerServiceTypeFilter`/`SetTrainerServiceTypeFilter`.
//!
//! 12340 keeps it in the CVar `serviceTypeFilter` (registered at `0x51ed64`, default `"3"`, record
//! `ds:0xbd09e4`), so it outlives the VM and is saved with the config file. The rebuild (`0x594ba0`)
//! keeps a service when `mask & 1 << state` is set, with the state as [`TrainerServiceCategory`]'s
//! filter slot. The setter (`0x595ec0`) is the CVar's only writer, and no list packet resets it.

use mlua::{Lua, Value};

use super::super::cvars::{set_from_engine, sstr_to_int, CvarSlot};
use super::super::{binding_abi, Model};
use super::{queue_trainer_update, TrainerServiceCategory};

const CVAR: &str = "serviceTypeFilter";
const DEFAULT_MASK: &str = "3";
/// `SetTrainerServiceTypeFilter("all", …)` stores every bit (`0x595f06`).
const ALL_MASK: u32 = 7;

/// The live mask, read through the CVar's integer value like `*(rec + 0x30)`.
fn mask(model: &Model) -> u32 {
    let value = model
        .cvars
        .get(&CVAR.to_ascii_lowercase())
        .map_or(DEFAULT_MASK, |s| s.value.as_str());
    sstr_to_int(value) as u32
}

/// Whether the filter lets services of this state through.
pub(super) fn shows(model: &Model, category: TrainerServiceCategory) -> bool {
    mask(model) & 1 << category.filter_slot() != 0
}

/// `0x594fe0`: write the CVar, then the caller's rebuild and `TRAINER_UPDATE`. A VM the host never
/// seeded registers the row itself, as the client always has it.
fn store(model: &mut Model, mask: u32) {
    model
        .cvars
        .entry(CVAR.to_ascii_lowercase())
        .or_insert_with(|| CvarSlot {
            name: CVAR.to_string(),
            value: DEFAULT_MASK.to_string(),
            default: DEFAULT_MASK.to_string(),
            latched: false,
            pending: None,
        });
    set_from_engine(model, CVAR, mask.to_string());
    queue_trainer_update(model);
}

/// The type string's slot (`0x593db0`), else the bindings' own error.
fn slot(kind: Option<&str>, bad: &'static str) -> mlua::Result<usize> {
    TrainerServiceCategory::from_filter_str(kind.unwrap_or_default())
        .map(TrainerServiceCategory::filter_slot)
        .ok_or_else(|| mlua::Error::runtime(bad))
}

pub(super) fn register(lua: &Lua, g: &mlua::Table) -> mlua::Result<()> {
    // `0x593e10`: 1 or nil; a missing or unknown type raises.
    g.set(
        "GetTrainerServiceTypeFilter",
        lua.create_function(|lua, kind: Option<String>| {
            if kind.is_none() {
                return Err(mlua::Error::runtime(
                    "Usage: GetTrainerServiceTypeFilter(\"type\")",
                ));
            }
            let bit = slot(
                kind.as_deref(),
                "Bad service type in GetTrainerServiceTypeFilter",
            )?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(binding_abi::flag(mask(&model) & 1 << bit != 0))
        })?,
    )?;
    // `0x595ec0`: "all" stores every bit; otherwise the bit goes on or off, or on alone when the
    // third argument is set. Every accepted call rebuilds and fires, moved bit or not.
    g.set(
        "SetTrainerServiceTypeFilter",
        lua.create_function(
            |lua, (kind, on, exclusive): (Option<String>, Value, Value)| {
                let Some(kind) = kind else {
                    return Err(mlua::Error::runtime(
                        "Usage: SetTrainerServiceTypeFilter(\"type\", on/off [, exclusive])",
                    ));
                };
                let mut model = lua.app_data_mut::<Model>().expect("model app_data");
                if kind.eq_ignore_ascii_case("all") {
                    store(&mut model, ALL_MASK);
                    return Ok(());
                }
                let bit = 1
                    << slot(
                        Some(&kind),
                        "Bad service type in SetTrainerServiceTypeFilter",
                    )?;
                if on.is_nil() {
                    return Err(mlua::Error::runtime(
                        "Missing on/off parameter in SetTrainerServiceTypeFilter",
                    ));
                }
                let old = mask(&model);
                let new = if !super::super::action::truthy_nonzero(&on) {
                    old & !bit
                } else if super::super::action::truthy_nonzero(&exclusive) {
                    bit
                } else {
                    old | bit
                };
                store(&mut model, new);
                Ok(())
            },
        )?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::UiScript;

    fn mask(s: &UiScript) -> Option<String> {
        s.cvar("serviceTypeFilter")
    }

    /// `0x595ec0`'s three arms over the CVar, and the errors `0x593e10`/`0x595ec0` raise.
    #[test]
    fn the_setter_moves_one_bit_all_bits_or_one_bit_alone() {
        let mut s = UiScript::new().unwrap();
        let on = |s: &UiScript, kind: &str| {
            s.eval::<bool>(&format!(
                "return GetTrainerServiceTypeFilter('{kind}') == 1"
            ))
            .unwrap()
        };
        assert!(on(&s, "available") && on(&s, "unavailable") && !on(&s, "used"));
        s.run("SetTrainerServiceTypeFilter('used', 1)").unwrap();
        assert_eq!(mask(&s).as_deref(), Some("7"));
        s.run("SetTrainerServiceTypeFilter('available', 0)")
            .unwrap();
        assert_eq!(mask(&s).as_deref(), Some("6"));
        s.run("SetTrainerServiceTypeFilter('unavailable', 1, 1)")
            .unwrap();
        assert_eq!(mask(&s).as_deref(), Some("2"), "exclusive: the bit alone");
        s.run("SetTrainerServiceTypeFilter('all')").unwrap();
        assert_eq!(mask(&s).as_deref(), Some("7"), "\"all\" needs no on/off");
        assert_eq!(
            s.take_cvar_changes().last(),
            Some(&("serviceTypeFilter".to_string(), "7".to_string())),
            "every write queues for the config file"
        );
        assert!(s.run("GetTrainerServiceTypeFilter('bogus')").is_err());
        assert!(s.run("SetTrainerServiceTypeFilter('used')").is_err());
        assert!(s.run("SetTrainerServiceTypeFilter()").is_err());
    }
}
