//! `GetItemStats(itemLink[, statTable])` and `GetItemStatDelta(itemLink1, itemLink2[, statTable])`,
//! build 12340's `0x608690` and `0x608760`: the client's item-stat aggregate (`0x6207a0`) keyed
//! by `0x6085d0`. A template's numbers come from `0x61f4c0`, an heirloom's from `0x61f550`, and
//! both pass the folds of `0x61ba90`.
//!
//! Labelled, not summed: equip-spell auras (`0x61f350` → `0x61b5e0`), a link's random suffix or
//! property enchantments (`0x61fa20`) and a druid's feral attack power (`0x61a8f0`).

use mlua::{Lua, MultiValue, Table, Value};

use super::{ItemTemplateView, Model};
use crate::heirloom::mod_name;

/// The aggregate's integer fields, keyed by [`key`].
const FIELDS: usize = 73;

/// The aggregate: DPS, then [`FIELDS`] integers (`0x6207a0` clears `0x128` bytes past the DPS).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemStatSums {
    pub(crate) dps: f32,
    pub(crate) fields: [i32; FIELDS],
}

impl Default for ItemStatSums {
    fn default() -> Self {
        Self {
            dps: 0.0,
            fields: [0; FIELDS],
        }
    }
}

/// `(target, parts)` for the folds of `0x61ba90` (`0x61b9e0`): the target's value joins each part,
/// and parts that come out equal collapse back into the target.
const FOLDS: [(usize, &[usize]); 8] = [
    (56, &[53, 52]),     // spell power → damage, healing
    (42, &[27, 28, 29]), // hit → melee, ranged, spell
    (43, &[30, 31, 32]), // crit
    (44, &[33, 34, 35]), // hit taken
    (45, &[36, 37, 38]), // crit taken
    (47, &[39, 40, 41]), // haste
    (61, &[54]),         // mana per 5 → POWER_REGEN0
    (7, &[8, 9]),        // attack power → melee, ranged
];

/// One stat into the aggregate (`0x61b990`): field `stat + 11`, with attack power (38) also
/// counted as melee and ranged (fields 8, 9) and ranged attack power (39) as ranged.
fn add_stat(f: &mut [i32; FIELDS], stat: u32, value: i32) {
    if let Some(x) = f.get_mut(stat as usize + 11) {
        *x += value;
    }
    match stat {
        38 => {
            f[8] += value;
            f[9] += value;
        }
        39 => f[9] += value,
        _ => {}
    }
}

impl ItemStatSums {
    /// An item's aggregate: an heirloom's at `link_level` (`0x61f550`; 0 is its owner's level,
    /// `0x61e740`), any other's from its template (`0x61f4c0`).
    pub(crate) fn of(v: &ItemTemplateView, link_level: u32, owner: u32) -> Self {
        let mut s = Self::default();
        match &v.heirloom {
            Some(h) => {
                let level = match link_level {
                    0 => h.level_for(owner, v.required_level),
                    l => l,
                };
                // No row, no numbers: the client adds nothing (`0x61f594`).
                if let Some(l) = h.at(level) {
                    for &(t, val) in &l.stats {
                        add_stat(&mut s.fields, t, val);
                    }
                    match l.armor {
                        Some(a) => s.fields[0] += a as i32,
                        None => s.add_resistances(v),
                    }
                    match l.dps {
                        Some((lo, hi)) => s.dps = 0.5 * (lo + hi),
                        None => s.add_damage(v),
                    }
                    if let Some(sp) = l.spell_power {
                        s.fields[56] += sp as i32;
                    }
                }
            }
            None => {
                for &(t, val) in &v.stats {
                    add_stat(&mut s.fields, t, val);
                }
                s.add_resistances(v);
                // The socket colour counts (`0x61b930`): meta, red, yellow, blue.
                for c in v.socket_colors {
                    for (bit, field) in [(1, 69), (2, 70), (4, 71), (8, 72)] {
                        if c & bit != 0 {
                            s.fields[field] += 1;
                        }
                    }
                }
                s.fields[60] += v.block as i32;
                s.add_damage(v);
            }
        }
        s.fold();
        s
    }

    /// Armor and the six schools (`0x61b8e0`), fields 0-6.
    fn add_resistances(&mut self, v: &ItemTemplateView) {
        self.fields[0] += v.armor as i32;
        for (i, r) in v.resistances.iter().enumerate() {
            self.fields[i + 1] += r;
        }
    }

    /// The first two damage slots' mean over the speed (`0x61f410`).
    fn add_damage(&mut self, v: &ItemTemplateView) {
        let secs = v.delay_ms as f32 * 0.001;
        if secs > 0.0 {
            for &(min, max, _) in v.damages.iter().take(2) {
                self.dps += (max + min) * 0.5 / secs;
            }
        }
    }

    fn fold(&mut self) {
        let f = &mut self.fields;
        // Melee attack power equal to feral drops the feral line.
        if f[8] == f[10] {
            f[10] = 0;
        }
        for (target, parts) in FOLDS {
            let t = f[target];
            let mut equal = true;
            for (k, &p) in parts.iter().enumerate() {
                f[p] += t;
                if k > 0 && f[p] != f[parts[k - 1]] {
                    equal = false;
                }
            }
            if equal {
                f[target] = f[parts[0]];
                for &p in parts {
                    f[p] = 0;
                }
            } else {
                f[target] = 0;
            }
        }
    }

    /// `a − b`, then folded again (`0x61dbc0`).
    pub(crate) fn delta(a: &Self, b: &Self) -> Self {
        let mut d = Self {
            dps: a.dps - b.dps,
            ..Self::default()
        };
        for (i, x) in d.fields.iter_mut().enumerate() {
            *x = a.fields[i] - b.fields[i];
        }
        d.fold();
        d
    }

    /// The `(key, value)` pairs `0x6085d0` sets: DPS past ±`FLT_EPSILON`, then each non-zero
    /// field but 49 and 50 (attack power, already in 7-9).
    pub(crate) fn entries(&self) -> Vec<(String, f64)> {
        let mut out = Vec::new();
        if self.dps.abs() > f32::EPSILON {
            out.push((
                "ITEM_MOD_DAMAGE_PER_SECOND_SHORT".to_string(),
                f64::from(self.dps),
            ));
        }
        for (i, &v) in self.fields.iter().enumerate() {
            if v != 0 && i != 49 && i != 50 {
                out.push((key(i), f64::from(v)));
            }
        }
        out
    }
}

/// A field's global name (`0x61bb20`).
pub(crate) fn key(i: usize) -> String {
    match i {
        0..=6 => format!("RESISTANCE{i}_NAME"),
        7 => "ITEM_MOD_ATTACK_POWER_SHORT".into(),
        8 => "ITEM_MOD_MELEE_ATTACK_POWER_SHORT".into(),
        9 => "ITEM_MOD_RANGED_ATTACK_POWER_SHORT".into(),
        10 => "ITEM_MOD_FERAL_ATTACK_POWER_SHORT".into(),
        // A stat with no name keeps the bare suffix, as the binary's `"%s%s"` does.
        11..=59 => format!("{}_SHORT", mod_name(i as u32 - 11).unwrap_or("")),
        60 => "ITEM_MOD_BLOCK_VALUE_SHORT".into(),
        61..=67 => format!("ITEM_MOD_POWER_REGEN{}_SHORT", i - 61),
        68 => "ITEM_MOD_HEALTH_REGEN_SHORT".into(),
        _ => format!(
            "EMPTY_SOCKET_{}",
            ["META", "RED", "YELLOW", "BLUE"][(i - 69).min(3)]
        ),
    }
}

/// A link's item id and link level (`0x6207a0`): `strstr` for `item:`, then fields 0 and 8 of
/// `id:enchant:gem1:gem2:gem3:gem4:suffix:unique:level`.
fn parse_link(link: &str) -> Option<(u32, u32)> {
    let rest = &link[link.find("item:")? + 5..];
    let fields: Vec<i64> = rest.split(':').map(super::reference_atoi).collect();
    let id = u32::try_from(*fields.first()?).ok().filter(|&id| id > 0)?;
    let level = fields
        .get(8)
        .and_then(|&l| u32::try_from(l).ok())
        .unwrap_or(0);
    Some((id, level))
}

/// A link's aggregate; `None` for a link that does not parse or an uncached template, whose ask is
/// recorded.
fn sums_of(lua: &Lua, link: &str) -> Option<ItemStatSums> {
    let (id, level) = parse_link(link)?;
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    let owner = model.unit("player").map_or(0, |u| u.level);
    match model.item_templates.get(&id) {
        Some(v) => Some(ItemStatSums::of(v, level, owner)),
        None => {
            model.item_stat_asks.insert(id);
            None
        }
    }
}

/// The given table, or a fresh one, filled.
fn fill_table(lua: &Lua, given: Option<Table>, sums: &ItemStatSums) -> mlua::Result<Table> {
    let t = match given {
        Some(t) => t,
        None => lua.create_table()?,
    };
    for (k, v) in sums.entries() {
        t.set(k, v)?;
    }
    Ok(t)
}

/// The optional table argument: absent or nil is none, a table is used, anything else is misuse.
fn table_arg(v: Option<Value>) -> Result<Option<Table>, ()> {
    match v {
        None | Some(Value::Nil) => Ok(None),
        Some(Value::Table(t)) => Ok(Some(t)),
        Some(_) => Err(()),
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set(
        "GetItemStats",
        lua.create_function(|lua, (link, rest): (Value, MultiValue)| {
            const USAGE: &str = "Usage: GetItemStats(itemLink[, statTable])";
            let link = match link {
                Value::String(s) => s.to_str()?.to_string(),
                Value::Integer(n) => n.to_string(),
                Value::Number(n) => n.to_string(),
                _ => return Err(mlua::Error::RuntimeError(USAGE.into())),
            };
            let given = table_arg(rest.into_iter().next())
                .map_err(|()| mlua::Error::RuntimeError(USAGE.into()))?;
            match sums_of(lua, &link) {
                Some(sums) => Ok(MultiValue::from_vec(vec![Value::Table(fill_table(
                    lua, given, &sums,
                )?)])),
                None => Ok(MultiValue::new()),
            }
        })?,
    )?;
    lua.globals().set(
        "GetItemStatDelta",
        lua.create_function(|lua, (a, b, rest): (Value, Value, MultiValue)| {
            // The binary's own usage text names the old binding.
            const USAGE: &str = "Usage: GetItemStatSummary(itemLink1, itemLink2[, statTable])";
            let text = |v: Value| match v {
                Value::String(s) => Ok(s.to_str()?.to_string()),
                Value::Integer(n) => Ok(n.to_string()),
                Value::Number(n) => Ok(n.to_string()),
                _ => Err(mlua::Error::RuntimeError(USAGE.into())),
            };
            let (a, b) = (text(a)?, text(b)?);
            let given = table_arg(rest.into_iter().next())
                .map_err(|()| mlua::Error::RuntimeError(USAGE.into()))?;
            let (Some(x), Some(y)) = (sums_of(lua, &a), sums_of(lua, &b)) else {
                return Ok(MultiValue::new());
            };
            let d = ItemStatSums::delta(&x, &y);
            Ok(MultiValue::from_vec(vec![Value::Table(fill_table(
                lua, given, &d,
            )?)]))
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "item_stat_table_tests.rs"]
mod tests;
