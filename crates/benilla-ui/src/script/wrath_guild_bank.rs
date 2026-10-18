//! Build-12340 guild bank window (Blizzard_GuildBankUI and the census callers in MerchantFrame
//! and UIParent): slots, logs, tab text and the bank gestures, over the bank Wire B feeds
//! ([`super::WrathGuildBankView`], one Wow.exe round: `/tmp/azb-ui-gbank-native.log`).
//!
//! Gestures go out as [`WrathGuildBankRequest`]s (capture-only until App sends them); the bank
//! state is whatever the host publishes. The selected tab is local, as natively (`0xc1dc0d`).

use mlua::{Lua, MultiValue, Value};

use super::{Model, WrathGuildBankRequest};

/// One occupied bank slot, as `SMSG_GUILD_BANK_LIST` carries it (AC `GuildBankItemInfo`), with
/// the icon and nine-field link App resolves from the item cache.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathGuildBankSlot {
    /// 0-based, below 98.
    pub slot: u8,
    pub item_id: u32,
    pub count: i32,
    pub random_property_id: i32,
    /// The suffix factor.
    pub random_property_seed: i32,
    pub enchant_id: i32,
    pub charges: i32,
    pub flags: i32,
    /// `(socket index, gem enchant id)` pairs.
    pub gems: Vec<(u8, i32)>,
    pub texture: Option<String>,
    pub link: Option<String>,
    /// Locked by a pending move (`GetGuildBankItemInfo`'s third value).
    pub locked: bool,
}

/// One money-log row (`0xc11070`, 25 rows).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathGuildBankMoneyLog {
    /// 4 deposit, 5 withdraw, 8 withdrawForTab, 9 buyTab, anything else repair.
    pub kind: u8,
    pub player: Option<String>,
    pub copper: i32,
    pub seconds_ago: u32,
}

/// One item-log row (`0xc0f900`, 25 rows per tab).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathGuildBankItemLog {
    /// 1 deposit, 2 withdraw, 3 move out of this tab, 7 move into it; anything else unknown.
    pub kind: u8,
    pub player: Option<String>,
    /// The item's link; `None` (template not cached) answers "".
    pub link: Option<String>,
    pub count: i32,
    /// The other tab of a move, 1-based.
    pub other_tab: u8,
    pub seconds_ago: u32,
}

/// The age split every 12340 log answers with: years of 8760 hours, months of 720, days of 24.
pub(super) fn age(seconds_ago: u32) -> (i64, i64, i64, i64) {
    let hours = i64::from(seconds_ago / 3600);
    let rest = hours % 8760 % 720;
    (hours / 8760, hours % 8760 / 720, rest / 24, rest % 24)
}

fn arg(args: &MultiValue, i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Nil)
}

/// A rounded 1-based argument as 0-based, `None` when not a number or below 1.
fn index(lua: &Lua, v: &Value) -> Option<Option<usize>> {
    let n = lua.coerce_number(v.clone()).ok().flatten()?;
    Some(usize::try_from(n.round_ties_even() as i64 - 1).ok())
}

fn int(n: impl Into<i64>) -> Value {
    Value::Integer(n.into())
}

fn s(lua: &Lua, v: &Option<String>) -> mlua::Result<Value> {
    Ok(match v {
        Some(v) => Value::String(lua.create_string(v)?),
        None => Value::Nil,
    })
}

fn set(
    lua: &Lua,
    name: &str,
    f: impl Fn(&Lua, MultiValue) -> mlua::Result<MultiValue> + Send + 'static,
) -> mlua::Result<()> {
    lua.globals()
        .set(name, lua.create_function(move |lua, args| f(lua, args))?)
}

fn none() -> mlua::Result<MultiValue> {
    Ok(MultiValue::new())
}

fn push(lua: &Lua, r: WrathGuildBankRequest) {
    lua.app_data_mut::<Model>()
        .expect("model app_data")
        .wrath_friends
        .requests
        .push(r);
}

/// The slot at 0-based `(tab, slot)`.
fn slot_at(model: &Model, tab: usize, slot: usize) -> Option<WrathGuildBankSlot> {
    model
        .wrath_friends
        .bank
        .slots
        .get(tab)?
        .iter()
        .find(|s| usize::from(s.slot) == slot && s.item_id != 0)
        .cloned()
}

/// Whether the player's rank holds `right` (AC `GuildRankRights`) with a daily gold allowance.
fn rank_right(model: &Model, right: u32) -> bool {
    let guild = &model.guild;
    guild.in_guild
        && guild.rights & right != 0
        && usize::try_from(guild.rank_index)
            .ok()
            .and_then(|r| model.wrath_friends.bank.rank_rights.get(r))
            .is_some_and(|r| r.money_per_day != 0)
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    use WrathGuildBankRequest as R;
    let two = |lua: &Lua, args: &MultiValue, usage: &'static str| match (
        index(lua, &arg(args, 0)),
        index(lua, &arg(args, 1)),
    ) {
        (Some(t), Some(s)) => Ok((t, s)),
        _ => Err(mlua::Error::runtime(usage)),
    };

    // GetGuildBankItemInfo(tab, slot) (`0x5a6000`) → texture, count, locked; the miss is
    // nil, 0, nil.
    set(lua, "GetGuildBankItemInfo", move |lua, args| {
        let (t, sl) = two(lua, &args, "Usage: GetGuildBankItemInfo(tab, slot)")?;
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        let hit = t
            .zip(sl)
            .filter(|(t, _)| *t < 6)
            .and_then(|(t, s)| slot_at(&model, t, s));
        Ok(match hit {
            Some(slot) if model.unit_guids.player != 0 => MultiValue::from_vec(vec![
                s(lua, &slot.texture)?,
                int(slot.count),
                super::binding_abi::flag(slot.locked),
            ]),
            _ => MultiValue::from_vec(vec![Value::Nil, int(0), Value::Nil]),
        })
    })?;
    // GetGuildBankItemLink(tab, slot) (`0x5a4d30`): nothing until the link is resolved.
    set(lua, "GetGuildBankItemLink", move |lua, args| {
        let (t, sl) = two(lua, &args, "Usage: GetGuildBankItemLink(tab, slot)")?;
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        let link = t
            .zip(sl)
            .filter(|(t, _)| *t < 6)
            .and_then(|(t, s)| slot_at(&model, t, s))
            .and_then(|slot| slot.link);
        Ok(match link {
            Some(link) => MultiValue::from_vec(vec![Value::String(lua.create_string(&link)?)]),
            None => MultiValue::new(),
        })
    })?;

    set(lua, "GetCurrentGuildBankTab", |lua, _| {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        Ok(MultiValue::from_vec(vec![int(i64::from(
            model.wrath_friends.current_tab,
        ) + 1)]))
    })?;
    // SetCurrentGuildBankTab(tab) (`0x5a43c0`): truncated, 1..6 only.
    set(lua, "SetCurrentGuildBankTab", |lua, args| {
        let Some(n) = lua.coerce_number(arg(&args, 0))? else {
            return Err(mlua::Error::runtime("Usage: SetCurrentTab(tab)"));
        };
        let tab = n.trunc() as i64;
        if (1..=6).contains(&tab) {
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .wrath_friends
                .current_tab = (tab - 1) as u8;
        }
        none()
    })?;

    // ── gestures ──
    for (global, usage) in [
        ("QueryGuildBankTab", "Usage: QueryGuildBankTab(tab)"),
        ("QueryGuildBankText", "Usage: QueryGuildBankText(tab)"),
        ("QueryGuildBankLog", "Usage: QueryGuildBankLog(tab)"),
    ] {
        set(lua, global, move |lua, args| {
            let Some(t) = index(lua, &arg(&args, 0)) else {
                return Err(mlua::Error::runtime(usage));
            };
            if let Some(tab) = t.filter(|t| *t < 7) {
                let tab = tab as u8 + 1;
                push(
                    lua,
                    match global {
                        "QueryGuildBankTab" => R::QueryTab { tab },
                        "QueryGuildBankText" => R::QueryText { tab },
                        _ => R::QueryLog { tab },
                    },
                );
            }
            none()
        })?;
    }
    for (global, usage) in [
        (
            "PickupGuildBankItem",
            "Usage: PickupGuildBankItem(tab, slot)",
        ),
        (
            "AutoStoreGuildBankItem",
            "Usage: AutoStoreGuildBankItem(tab, slot)",
        ),
    ] {
        set(lua, global, move |lua, args| {
            let (t, sl) = two(lua, &args, usage)?;
            if let Some((tab, slot)) = t.zip(sl).filter(|(t, s)| *t < 6 && *s < 98) {
                let (tab, slot) = (tab as u8 + 1, slot as u8 + 1);
                push(
                    lua,
                    if global == "PickupGuildBankItem" {
                        R::PickupItem { tab, slot }
                    } else {
                        R::AutoStoreItem { tab, slot }
                    },
                );
            }
            none()
        })?;
    }
    // SplitGuildBankItem(tab, slot, amount) (`0x5a6600`): 1..stack of an unlocked slot.
    set(lua, "SplitGuildBankItem", move |lua, args| {
        let (t, sl) = two(lua, &args, "Usage: SplitGuildBankItem(tab, slot, amount)")?;
        let Some(n) = lua.coerce_number(arg(&args, 2))? else {
            return Err(mlua::Error::runtime(
                "Usage: SplitGuildBankItem(tab, slot, amount)",
            ));
        };
        let amount = n.trunc() as i64;
        let ok = {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            t.zip(sl)
                .filter(|(t, s)| *t < 6 && *s < 98)
                .and_then(|(t, s)| slot_at(&model, t, s).map(|slot| (t, s, slot)))
                .filter(|(_, _, slot)| {
                    !slot.locked && amount > 0 && amount <= i64::from(slot.count)
                })
                .map(|(t, s, _)| (t as u8 + 1, s as u8 + 1))
        };
        if let Some((tab, slot)) = ok {
            push(
                lua,
                R::SplitItem {
                    tab,
                    slot,
                    count: amount as u32,
                },
            );
        }
        none()
    })?;
    set(lua, "SetGuildBankText", |lua, args| {
        let (Some(t), Some(text)) = (
            index(lua, &arg(&args, 0)),
            lua.coerce_string(arg(&args, 1))?
                .map(|s| s.to_string_lossy()),
        ) else {
            return Err(mlua::Error::runtime("Usage: SetGuildBankText(tab, text)"));
        };
        if let Some(tab) = t.filter(|t| *t < 6) {
            push(
                lua,
                R::SetText {
                    tab: tab as u8 + 1,
                    text,
                },
            );
        }
        none()
    })?;
    // SetGuildBankTabInfo(tab, name, icon) (`0x5a6a00`).
    set(lua, "SetGuildBankTabInfo", |lua, args| {
        let Some(t) = index(lua, &arg(&args, 0)) else {
            return Err(mlua::Error::runtime(
                "Usage: SetGuildBankTabInfo(tab, name, icon)",
            ));
        };
        let name = lua
            .coerce_string(arg(&args, 1))?
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        let icon = lua
            .coerce_string(arg(&args, 2))?
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        if let Some(tab) = t.filter(|t| *t < 6) {
            push(
                lua,
                R::SetTabInfo {
                    tab: tab as u8 + 1,
                    name,
                    icon,
                },
            );
        }
        none()
    })?;
    set(lua, "CloseGuildBankFrame", |lua, _| {
        push(lua, R::Close);
        none()
    })?;

    // ── logs and text ──
    set(lua, "GetNumGuildBankMoneyTransactions", |lua, _| {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        Ok(MultiValue::from_vec(vec![int(
            model.wrath_friends.bank.money_log.len().min(25) as i64,
        )]))
    })?;
    // GetGuildBankMoneyTransaction(index) (`0x5a4e50`) → type, name, amount, years, months,
    // days, hours.
    set(lua, "GetGuildBankMoneyTransaction", |lua, args| {
        let Some(i) = index(lua, &arg(&args, 0)) else {
            return Err(mlua::Error::runtime(
                "Usage: GetGuildBankMoneyTransaction(index)",
            ));
        };
        let row = {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            i.filter(|i| *i < 25)
                .and_then(|i| model.wrath_friends.bank.money_log.get(i).cloned())
                .unwrap_or_default()
        };
        let kind = match row.kind {
            4 => "deposit",
            5 => "withdraw",
            8 => "withdrawForTab",
            9 => "buyTab",
            _ => "repair",
        };
        let (y, m, d, h) = age(row.seconds_ago);
        Ok(MultiValue::from_vec(vec![
            Value::String(lua.create_string(kind)?),
            s(lua, &row.player)?,
            int(row.copper),
            int(y),
            int(m),
            int(d),
            int(h),
        ]))
    })?;
    set(lua, "GetNumGuildBankTransactions", |lua, args| {
        let Some(t) = index(lua, &arg(&args, 0)) else {
            return Err(mlua::Error::runtime(
                "Usage: GetNumGuildBankTransactions(tab)",
            ));
        };
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        let n = t
            .filter(|t| *t < 7)
            .and_then(|t| model.wrath_friends.bank.tab_logs.get(t))
            .map_or(0, |l| l.len().min(25));
        Ok(MultiValue::from_vec(vec![int(n as i64)]))
    })?;
    // GetGuildBankTransaction(tab, index) (`0x5a6dc0`) → type, name, itemLink, count, then for a
    // move the source and destination tabs, then years, months, days, hours.
    set(lua, "GetGuildBankTransaction", move |lua, args| {
        let (t, i) = two(lua, &args, "Usage: GetGuildBankTransaction(tab, index)")?;
        let (row, has_player) = {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let row = t
                .zip(i)
                .filter(|(t, i)| *t < 7 && *i < 25)
                .and_then(|(t, i)| model.wrath_friends.bank.tab_logs.get(t)?.get(i).cloned());
            (row, model.unit_guids.player != 0)
        };
        let (Some(row), true) = (row, has_player) else {
            return none();
        };
        let tab = t.unwrap_or(0) as i64 + 1;
        let kind = match row.kind {
            1 => "deposit",
            2 => "withdraw",
            3 | 7 => "move",
            _ => "unknown",
        };
        let mut out = vec![
            Value::String(lua.create_string(kind)?),
            s(lua, &row.player)?,
            Value::String(lua.create_string(row.link.as_deref().unwrap_or(""))?),
            int(row.count),
        ];
        match row.kind {
            3 => out.extend([int(tab), int(i64::from(row.other_tab))]),
            7 => out.extend([int(i64::from(row.other_tab)), int(tab)]),
            _ => out.extend([Value::Nil, Value::Nil]),
        }
        let (y, m, d, h) = age(row.seconds_ago);
        out.extend([int(y), int(m), int(d), int(h)]);
        Ok(MultiValue::from_vec(out))
    })?;
    // GetGuildBankText(tab) (`0x5a45e0`): a purchased tab's text, nil otherwise.
    set(lua, "GetGuildBankText", |lua, args| {
        let Some(t) = index(lua, &arg(&args, 0)) else {
            return Err(mlua::Error::runtime("Usage: GetGuildBankText(tab)"));
        };
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        let bank = &model.wrath_friends.bank;
        let text = t
            .filter(|t| *t < 6 && *t < bank.tabs.len())
            .map(|t| bank.tab_text.get(t).cloned().unwrap_or_default());
        Ok(MultiValue::from_vec(vec![s(lua, &text)?]))
    })?;

    // ── rights ──
    set(lua, "GetGuildBankWithdrawMoney", |lua, _| {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        Ok(MultiValue::from_vec(vec![Value::Number(
            model.wrath_friends.bank.withdraw_money_remaining as f64,
        )]))
    })?;
    // CanWithdrawGuildBankMoney() (`0x5a5f60`): the player's rank has the withdraw-gold right
    // (0x80000) and a daily allowance. CanGuildBankRepair() (`0x5cc200`): the repair right
    // (0x40000, AC `GR_RIGHT_WITHDRAW_REPAIR`), likewise. Labelled: the repair bit is AC's.
    for (global, right) in [
        ("CanWithdrawGuildBankMoney", 0x80000u32),
        ("CanGuildBankRepair", 0x40000),
    ] {
        set(lua, global, move |lua, _| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(MultiValue::from_vec(vec![super::binding_abi::flag(
                rank_right(&model, right),
            )]))
        })?;
    }
    // CanEditGuildTabInfo() (`0x5cc290`): the guild master. Labelled: from AC, where only the
    // leader renames tabs.
    set(lua, "CanEditGuildTabInfo", |lua, _| {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        Ok(MultiValue::from_vec(vec![super::binding_abi::flag(
            model.guild.in_guild && model.guild.is_leader,
        )]))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{ScriptDialect, UiScript, WrathGuildBankTab, WrathGuildBankView};

    fn vm() -> UiScript {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        s.model_mut().unit_guids.player = 0x10;
        s.set_wrath_guild_bank(WrathGuildBankView {
            tabs: vec![WrathGuildBankTab::default(), WrathGuildBankTab::default()],
            slots: vec![vec![WrathGuildBankSlot {
                slot: 4,
                item_id: 2589,
                count: 20,
                texture: Some("Interface\\Icons\\INV_Fabric_Linen_01".into()),
                link: Some("|Hitem:2589|h[Linen Cloth]|h".into()),
                ..Default::default()
            }]],
            money_log: vec![WrathGuildBankMoneyLog {
                kind: 5,
                player: Some("Ann".into()),
                copper: 300,
                seconds_ago: 9000 * 3600,
            }],
            tab_logs: vec![vec![WrathGuildBankItemLog {
                kind: 3,
                player: Some("Bob".into()),
                link: None,
                count: 2,
                other_tab: 2,
                seconds_ago: 25 * 3600,
            }]],
            tab_text: vec!["hello".into()],
            withdraw_money_remaining: 5000,
            ..Default::default()
        });
        s
    }

    #[test]
    fn slots_links_and_tab_selection() {
        let s = vm();
        assert_eq!(
            s.eval::<(String, i64, bool)>(
                "local t, c, l = GetGuildBankItemInfo(1, 5) return t, c, l == nil"
            )
            .unwrap(),
            ("Interface\\Icons\\INV_Fabric_Linen_01".into(), 20, true)
        );
        assert_eq!(
            s.eval::<(bool, i64)>("local t, c = GetGuildBankItemInfo(1, 6) return t == nil, c")
                .unwrap(),
            (true, 0)
        );
        assert_eq!(
            s.eval::<String>("return GetGuildBankItemLink(1, 5)")
                .unwrap(),
            "|Hitem:2589|h[Linen Cloth]|h"
        );
        assert_eq!(
            s.eval::<i64>("return select('#', GetGuildBankItemLink(1, 6))")
                .unwrap(),
            0
        );
        s.run("SetCurrentGuildBankTab(2) SetCurrentGuildBankTab(9)")
            .unwrap();
        assert_eq!(s.eval::<i64>("return GetCurrentGuildBankTab()").unwrap(), 2);
        assert!(s.run("GetGuildBankItemInfo(1)").is_err());
    }

    #[test]
    fn logs_text_and_gestures() {
        let mut s = vm();
        assert_eq!(
            s.eval::<(String, String, i64, i64, i64, i64, i64)>(
                "return GetGuildBankMoneyTransaction(1)"
            )
            .unwrap(),
            ("withdraw".into(), "Ann".into(), 300, 1, 0, 10, 0)
        );
        assert_eq!(
            s.eval::<(String, String, String, i64, i64, i64, i64)>(
                "local k, n, l, c, a, b, y, m, d, h = GetGuildBankTransaction(1, 1) return k, n, l, c, a, b, d"
            )
            .unwrap(),
            ("move".into(), "Bob".into(), "".into(), 2, 1, 2, 1)
        );
        assert_eq!(
            s.eval::<String>("return GetGuildBankText(1)").unwrap(),
            "hello"
        );
        assert_eq!(s.eval::<String>("return GetGuildBankText(2)").unwrap(), "");
        assert!(s.eval::<bool>("return GetGuildBankText(3) == nil").unwrap());
        assert_eq!(
            s.eval::<f64>("return GetGuildBankWithdrawMoney()").unwrap(),
            5000.0
        );
        s.run(
            "QueryGuildBankTab(1) PickupGuildBankItem(1, 5) SplitGuildBankItem(1, 5, 7) \
             SplitGuildBankItem(1, 5, 99) AutoStoreGuildBankItem(2, 1) SetGuildBankText(1, 'x') \
             SetGuildBankTabInfo(2, 'Mats', 'INV_Box_01') QueryGuildBankLog(7) CloseGuildBankFrame()",
        )
        .unwrap();
        use WrathGuildBankRequest as R;
        assert_eq!(
            s.take_wrath_guild_bank_requests(),
            vec![
                R::QueryTab { tab: 1 },
                R::PickupItem { tab: 1, slot: 5 },
                R::SplitItem {
                    tab: 1,
                    slot: 5,
                    count: 7
                },
                R::AutoStoreItem { tab: 2, slot: 1 },
                R::SetText {
                    tab: 1,
                    text: "x".into()
                },
                R::SetTabInfo {
                    tab: 2,
                    name: "Mats".into(),
                    icon: "INV_Box_01".into()
                },
                R::QueryLog { tab: 7 },
                R::Close,
            ]
        );
    }

    #[test]
    fn rank_rights_gate_withdraw_and_repair() {
        let s = vm();
        assert!(s
            .eval::<bool>(
                "return CanWithdrawGuildBankMoney() == nil and CanGuildBankRepair() == nil"
            )
            .unwrap());
        {
            let mut m = s.model_mut();
            m.guild.in_guild = true;
            m.guild.rank_index = 0;
            m.guild.rights = 0x80000;
            m.wrath_friends.bank.rank_rights = vec![crate::script::WrathGuildBankRankRights {
                money_per_day: 1000,
                ..Default::default()
            }];
        }
        assert!(s
            .eval::<bool>("return CanWithdrawGuildBankMoney() == 1 and CanGuildBankRepair() == nil")
            .unwrap());
    }
}
