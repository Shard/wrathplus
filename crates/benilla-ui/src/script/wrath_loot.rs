//! Bounded build-12340 loot ABI for manual solo corpse loot.
//!
//! The mature Vanilla bindings remain the state store. This adapter changes only the Wrath ABI:
//! `GetLootSlotInfo` returns `locked` as its fifth result, and `LootSlot` is the actual take verb.

use mlua::{Function, Lua, MultiValue, Value};

use super::Model;

/// Engine globals used by the audited stock LootFrame closure.
pub(super) const ENGINE_FUNCTIONS: &[&str] = &[
    "CloseLoot",
    "GetItemQualityColor",
    "GetLootSlotInfo",
    "GetLootSlotLink",
    "GetNumLootItems",
    "IsAutoLootKeyDown",
    "IsFishingLoot",
    "LootSlot",
    "LootSlotIsCoin",
    "LootSlotIsItem",
];

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let vanilla_info: Function = globals.get("GetLootSlotInfo")?;

    // Build 12340 returns texture, name, quantity, quality, locked. The existing fifth Vanilla
    // extension is an item id and must not leak into the Wrath stock script as a truthy lock flag.
    globals.set(
        "GetLootSlotInfo",
        lua.create_function(move |_, slot: u32| {
            let mut values: MultiValue = vanilla_info.call(slot)?;
            while values.len() < 4 {
                values.push_back(Value::Nil);
            }
            values.truncate(4);
            values.push_back(Value::Boolean(false));
            Ok(values)
        })?,
    )?;

    // In build 12340 LootButton_OnClick calls LootSlot directly to take the displayed row. This is
    // intentionally distinct from the Vanilla LOOT_BIND-confirmation-only ABI.
    globals.set(
        "LootSlot",
        lua.create_function(|lua, slot: u32| {
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .loot_picks
                .push(slot);
            Ok(())
        })?,
    )?;

    // The host already receives real modifier state each frame. For the supported manual mode the
    // auto-loot key is Shift; no CVar or permanently false key state is invented.
    globals.set(
        "IsAutoLootKeyDown",
        lua.create_function(|lua, ()| {
            let shift = lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .modifiers
                .0;
            Ok(shift)
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{LootRow, LootState, ScriptDialect, UiScript};

    fn row() -> LootRow {
        LootRow {
            name: Some("Linen Cloth".into()),
            texture: Some("Interface\\Icons\\INV_Fabric_Linen_01".into()),
            quantity: 2,
            quality: Some(1),
            is_coin: false,
            item_id: 2589,
            link: Some("|cffffffff|Hitem:2589:0:0:0:0:0:0:0|h[Linen Cloth]|h|r".into()),
            random_property_id: 0,
        }
    }

    #[test]
    fn wrath_loot_slot_is_take_and_info_fifth_result_is_locked() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script.set_loot(Some(LootState {
            rows: vec![Some(row())],
            ..LootState::default()
        }));

        assert!(script
            .eval::<bool>(
                "local texture,name,count,quality,locked=GetLootSlotInfo(1); \
                 return texture~=nil and name=='Linen Cloth' and count==2 and quality==1 \
                    and locked==false"
            )
            .unwrap());
        script.run("LootSlot(1)").unwrap();
        assert_eq!(script.take_loot_picks(), [1]);
        assert!(script.take_loot_confirms().is_empty());
    }

    #[test]
    fn wrath_auto_loot_key_reports_the_real_shift_modifier() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        assert!(!script.eval::<bool>("return IsAutoLootKeyDown()").unwrap());
        script.set_modifiers(true, false, false);
        assert!(script.eval::<bool>("return IsAutoLootKeyDown()").unwrap());
    }

    #[test]
    fn vanilla_loot_slot_abi_is_unchanged() {
        let mut script = UiScript::new().unwrap();
        script.set_loot(Some(LootState {
            rows: vec![Some(row())],
            ..LootState::default()
        }));
        assert_eq!(
            script
                .eval::<i64>("local _,_,_,_,item=GetLootSlotInfo(1); return item")
                .unwrap(),
            2589
        );
        script.run("LootSlot(1)").unwrap();
        assert!(script.take_loot_picks().is_empty());
        assert_eq!(script.take_loot_confirms(), [1]);
    }
}
