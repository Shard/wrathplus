//! Build-12340 completed-quest history: `QueryQuestsCompleted` (`0x5b1bb0`) and
//! `GetQuestsCompleted` (`0x5b5290`), fed by the reply handler `0x5b5190`. Evidence: one headless
//! Ghidra round on the pinned Wow.exe (`/tmp/azb-app-mig/ghidra/quests-completed.log`).
//!
//! - `QueryQuestsCompleted()` writes opcode `0x500` with an empty body, sends it and returns no
//!   values; nothing throttles it. Here it records one pending query the host drains
//!   ([`UiScript::take_wrath_quests_completed_queries`]), so the VM never sends.
//! - The reply replaces the whole stored list, then fires `QUEST_QUERY_COMPLETE` (event `0x296`)
//!   with no arguments; the host does both ([`UiScript::set_wrath_quests_completed`]).
//! - `GetQuestsCompleted([t])` fills `t`, or a new table when arg 1 is not a table, with
//!   `t[questID] = true` for every stored id, never clearing existing keys, and returns it.
use super::{Model, UiScript};
use mlua::{Lua, Value};

/// The stored reply and the queries no host has drained yet.
#[derive(Default)]
pub(super) struct State {
    quest_ids: Vec<u32>,
    pending_queries: u32,
}

impl UiScript {
    /// Replace the stored completed-quest list (`0x5b5190`); the caller then fires
    /// `QUEST_QUERY_COMPLETE`. The list is the reply's ids in wire order, duplicates kept.
    pub fn set_wrath_quests_completed(&mut self, quest_ids: Vec<u32>) {
        self.model_mut().wrath_quests_completed.quest_ids = quest_ids;
    }

    /// The `QueryQuestsCompleted` calls since the last drain, each one `CMSG 0x500`.
    pub fn take_wrath_quests_completed_queries(&mut self) -> u32 {
        std::mem::take(&mut self.model_mut().wrath_quests_completed.pending_queries)
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set(
        "QueryQuestsCompleted",
        lua.create_function(|lua, ()| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let state = &mut model.wrath_quests_completed;
            state.pending_queries = state.pending_queries.saturating_add(1);
            Ok(())
        })?,
    )?;
    g.set(
        "GetQuestsCompleted",
        lua.create_function(|lua, arg: Value| {
            let table = match arg {
                Value::Table(t) => t,
                _ => lua.create_table()?,
            };
            let ids = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                model.wrath_quests_completed.quest_ids.clone()
            };
            for id in ids {
                table.raw_set(f64::from(id), true)?;
            }
            Ok(table)
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{ScriptDialect, UiScript};

    #[test]
    fn queries_are_counted_and_the_reply_fills_the_passed_table() {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        assert_eq!(s.arity("QueryQuestsCompleted()").unwrap(), 0);
        s.run("QueryQuestsCompleted() QueryQuestsCompleted()")
            .unwrap();
        assert_eq!(s.take_wrath_quests_completed_queries(), 3);
        assert_eq!(s.take_wrath_quests_completed_queries(), 0);

        // Nothing stored yet: an empty table.
        assert_eq!(
            s.eval::<i64>("local n=0 for _ in pairs(GetQuestsCompleted()) do n=n+1 end return n")
                .unwrap(),
            0
        );
        s.set_wrath_quests_completed(vec![7, 12, 7]);
        s.run("T = { [99] = 'kept' } R = GetQuestsCompleted(T)")
            .unwrap();
        assert!(s
            .eval::<bool>("return R == T and T[7] == true and T[12] == true and T[99] == 'kept'")
            .unwrap());
        // A non-table argument gets a fresh table.
        assert!(s
            .eval::<bool>(
                "local t = GetQuestsCompleted(5) return type(t) == 'table' and t[12] == true"
            )
            .unwrap());
        // A new reply replaces the list.
        s.set_wrath_quests_completed(vec![3]);
        assert!(s
            .eval::<bool>("local t = GetQuestsCompleted() return t[3] == true and t[7] == nil")
            .unwrap());
    }

    #[test]
    fn the_vanilla_dialect_has_neither_function() {
        let s = UiScript::new_with_dialect(ScriptDialect::Vanilla).unwrap();
        assert!(s
            .eval::<bool>("return QueryQuestsCompleted == nil and GetQuestsCompleted == nil")
            .unwrap());
    }
}
