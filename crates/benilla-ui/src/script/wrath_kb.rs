//! Wrath knowledge base (`KnowledgeBaseFrame.lua`). Build 12340 fetches it over HTTP from the help
//! server (`0x550e10` builds `getKBSetup.xml?languageId=…&numArticles=…&pageNumber=…&locale=…` for
//! the async fetch `0x870040`), whose failure path fires `KNOWLEDGE_BASE_*_LOAD_FAILURE`. That
//! service no longer exists and this client makes no web requests, so every load fails: the
//! `BeginLoading` verbs queue their failure event for the next tick, nothing ever reads as loaded
//! (`[0xacd53c]`/`[0xacd5b4]`/`[0xacd604]` never reach 2), and each getter raises the binary's own
//! "not loaded" error. `KBSystem_GetMOTD`/`GetServerNotice` push their unset strings: nil.

use mlua::{Lua, MultiValue, Value};

use super::{Model, ScriptValue};

fn queue(lua: &Lua, event: &str) {
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    model
        .pending_events
        .push((event.to_string(), Vec::<ScriptValue>::new()));
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    for (name, event) in [
        ("KBSetup_BeginLoading", "KNOWLEDGE_BASE_SETUP_LOAD_FAILURE"),
        (
            "KBArticle_BeginLoading",
            "KNOWLEDGE_BASE_ARTICLE_LOAD_FAILURE",
        ),
    ] {
        g.set(
            name,
            lua.create_function(move |lua, _: MultiValue| {
                queue(lua, event);
                Ok(())
            })?,
        )?;
    }
    for name in ["KBSetup_IsLoaded", "KBQuery_IsLoaded", "KBArticle_IsLoaded"] {
        g.set(name, lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    }
    // A query needs a loaded setup first (`0x552580`), so it raises rather than fetching.
    for (name, what) in [
        ("KBSetup_GetLanguageCount", "setup"),
        ("KBSetup_GetLanguageData", "setup"),
        ("KBSetup_GetCategoryCount", "setup"),
        ("KBSetup_GetCategoryData", "setup"),
        ("KBSetup_GetSubCategoryCount", "setup"),
        ("KBSetup_GetSubCategoryData", "setup"),
        ("KBSetup_GetArticleHeaderCount", "setup"),
        ("KBSetup_GetArticleHeaderData", "setup"),
        ("KBSetup_GetTotalArticleCount", "setup"),
        ("KBQuery_BeginLoading", "setup"),
        ("KBQuery_GetArticleHeaderCount", "query"),
        ("KBQuery_GetArticleHeaderData", "query"),
        ("KBQuery_GetTotalArticleCount", "query"),
        ("KBArticle_GetData", "article"),
    ] {
        let message = format!("{name}() failed because {what} is not loaded");
        g.set(
            name,
            lua.create_function(move |_, _: MultiValue| -> mlua::Result<()> {
                Err(mlua::Error::runtime(message.clone()))
            })?,
        )?;
    }
    for name in ["KBSystem_GetMOTD", "KBSystem_GetServerNotice"] {
        g.set(
            name,
            lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{ScriptDialect, UiScript};

    #[test]
    fn every_load_fails_on_the_next_tick_and_getters_raise() {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        s.run(
            r#"seen = {} local f = CreateFrame("Frame")
            f:RegisterEvent("KNOWLEDGE_BASE_SETUP_LOAD_FAILURE")
            f:RegisterEvent("KNOWLEDGE_BASE_ARTICLE_LOAD_FAILURE")
            f:SetScript("OnEvent", function(_, e) seen[#seen+1] = e end)
            KBSetup_BeginLoading(20, 0) KBArticle_BeginLoading(1, 0)
            assert(#seen == 0, "the fetch is asynchronous")
            assert(KBSetup_IsLoaded() == false and KBSystem_GetMOTD() == nil)
            local ok, err = pcall(KBSetup_GetCategoryCount)
            assert(not ok and tostring(err):find("KBSetup_GetCategoryCount%(%) failed because setup is not loaded"))
            ok, err = pcall(KBQuery_GetTotalArticleCount)
            assert(not ok and tostring(err):find("query is not loaded"))"#,
        )
        .unwrap();
        s.tick(0.1);
        assert_eq!(
            s.eval::<String>("return table.concat(seen, ' ')").unwrap(),
            "KNOWLEDGE_BASE_SETUP_LOAD_FAILURE KNOWLEDGE_BASE_ARTICLE_LOAD_FAILURE"
        );
    }
}
