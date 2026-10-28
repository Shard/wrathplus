//! The looter lines of 12340's unit-tooltip sub-builder `0x621070` (`0x622209`..`0x62230a`),
//! after its quest lines: `MASTER_LOOTER: <name>` from the unit's master looter (`unit+0xc40`)
//! and `LOOT: <name>` from its round-robin looter (`unit+0xc48`), both written by
//! `SMSG_LOOT_LIST` (`0x71ca50` → `0x71bc00`). White (`0xad2d30`), `"%s: %s"`, each skipped
//! while the name cache has no name; the app resolves the names.

use std::collections::HashMap;

use mlua::{Lua, Table};

use super::UiScript;

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Per unit guid, the master looter's and the round-robin looter's names.
pub type UnitLooterNames = HashMap<u64, (Option<String>, Option<String>)>;

struct UnitLooters(UnitLooterNames);

impl UiScript {
    /// Replace the looter names the unit tooltip reads.
    pub fn set_unit_looters(&mut self, looters: UnitLooterNames) {
        let changed: std::collections::HashSet<_> = {
            let previous = self.lua().app_data_ref::<UnitLooters>();
            looters
                .keys()
                .chain(previous.iter().flat_map(|p| p.0.keys()))
                .copied()
                .filter(|guid| previous.as_ref().and_then(|p| p.0.get(guid)) != looters.get(guid))
                .collect()
        };
        self.lua().set_app_data(UnitLooters(looters));
        // Rebuild already-visible unit plates when a pending name arrives or ownership clears.
        let plates: Vec<_> = {
            let mut model = self.model_mut();
            let handles: Vec<_> = model
                .arena
                .iter_frames()
                .filter_map(|(h, f)| {
                    if let super::KindState::Tooltip(t) = &f.kind_state {
                        t.unit_guid
                            .filter(|guid| changed.contains(guid))
                            .filter(|_| f.shown && model.frame_to_id.contains_key(&h))
                            .map(|guid| (h, guid))
                    } else {
                        None
                    }
                })
                .collect();
            handles
                .into_iter()
                .map(|(h, guid)| (model.frame_id(h), guid))
                .collect()
        };
        for (id, guid) in plates {
            let result = super::object::frame_wrapper(self.lua(), id).and_then(|this| {
                super::tooltip_unit::render_unit_guid(self.lua(), &this, guid).map(|_| ())
            });
            if let Err(error) = result {
                self.push_error(error);
            }
        }
    }
}

/// Append `guid`'s looter lines, if `SMSG_LOOT_LIST` named any.
pub(super) fn append(lua: &Lua, this: &Table, guid: u64) -> mlua::Result<()> {
    let Some((master, looter)) = lua
        .app_data_ref::<UnitLooters>()
        .and_then(|l| l.0.get(&guid).cloned())
    else {
        return Ok(());
    };
    for (key, name) in [("MASTER_LOOTER", master), ("LOOT", looter)] {
        if let Some(name) = name {
            let Some(label) = crate::strings::global(lua, key) else {
                continue;
            };
            let line = (format!("{label}: {name}"), WHITE);
            super::tooltip::append_line(lua, this, line, None, false)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::UnitState;

    fn fixture() -> UiScript {
        let mut script = UiScript::new().unwrap();
        script.set_unit(
            "target",
            Some(UnitState {
                exists: true,
                guid: 42,
                name: Some("Corpse".into()),
                pvp: true,
                racial_leader: true,
                ..Default::default()
            }),
        );
        script
            .run(
                r#"
            MASTER_LOOTER = "[master]"; LOOT = "[loot]"
            TT = CreateFrame("GameTooltip", "TT")
            TT:SetOwner(CreateFrame("Frame"), "ANCHOR_RIGHT")
            TT:SetUnit("target")
        "#,
            )
            .unwrap();
        script
    }

    #[test]
    fn looter_publication_rebuilds_actual_unit_plate_after_leader_in_white() {
        let mut s = fixture();
        s.set_unit_looters([(42, (Some("Alice".into()), Some("Bob".into())))].into());
        s.run(
            r#"
            assert(TTTextLeft3:GetText() == "Leader")
            assert(TTTextLeft4:GetText() == "[master]: Alice")
            assert(TTTextLeft5:GetText() == "[loot]: Bob")
            local r,g,b = TTTextLeft4:GetTextColor()
            assert(r == 1 and g == 1 and b == 1)
        "#,
        )
        .unwrap();
        assert!(s.take_errors().is_empty());
    }

    #[test]
    fn looter_updates_do_not_rebuild_unrelated_or_hidden_tooltips() {
        let mut s = fixture();
        s.run("sets = 0; TT:SetScript('OnTooltipSetUnit', function() sets = sets + 1 end)")
            .unwrap();
        let names: UnitLooterNames = [(42, (Some("Alice".into()), None))].into();
        s.set_unit_looters(names.clone());
        assert_eq!(s.eval::<i64>("return sets").unwrap(), 1);
        s.set_unit_looters(names.clone());
        assert_eq!(
            s.eval::<i64>("return sets").unwrap(),
            1,
            "unchanged publication is quiet"
        );
        let mut unrelated = names;
        unrelated.insert(99, (None, Some("Other".into())));
        s.set_unit_looters(unrelated);
        assert_eq!(
            s.eval::<i64>("return sets").unwrap(),
            1,
            "another unit does not rebuild this plate"
        );
        s.run("TT:Hide()").unwrap();
        s.set_unit_looters(Default::default());
        s.run("assert(not TT:IsShown()); assert(sets == 1)")
            .unwrap();
        assert!(s.take_errors().is_empty());
    }

    #[test]
    fn unresolved_names_missing_globals_and_replacement_never_fabricate_labels() {
        let mut s = fixture();
        s.set_unit_looters([(42, (None, Some("Bob".into())))].into());
        s.run(r#"assert(TT:NumLines() == 4); assert(TTTextLeft4:GetText() == "[loot]: Bob")"#)
            .unwrap();
        s.run("LOOT = nil; TT:SetUnit('target')").unwrap();
        s.set_unit_looters([(42, (None, Some("Bob".into())))].into());
        s.run("assert(TT:NumLines() == 3)").unwrap();
        s.set_unit_looters([(99, (Some("Other unit".into()), None))].into());
        s.run("TT:SetUnit('target'); assert(TT:NumLines() == 3)")
            .unwrap();
        s.set_unit_looters(Default::default());
        s.run("assert(TT:NumLines() == 3)").unwrap();
        assert!(s.take_errors().is_empty());
    }
}
