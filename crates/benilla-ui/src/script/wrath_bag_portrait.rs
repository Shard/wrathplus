//! Build-12340 spelling used by ContainerFrame.lua:507 (Vanilla spells it Portait).
use super::{binding_abi::number_arg, Model};
use mlua::{Lua, Value};

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set(
        "SetBagPortraitTexture",
        lua.create_function(|lua, (texture, slot): (Value, Value)| {
            // 0x5d7180: only bag IDs 1..11 update the portrait. Backpack/keyring are no-ops.
            let slot = number_arg(lua, slot, "Usage: SetBagPortraitTexture(texture, slot)")? as i32;
            if slot > 11 {
                return Err(mlua::Error::runtime(
                    "Invalid slot in SetBagPortraitTexture",
                ));
            }
            if slot <= 0 {
                return Ok(());
            }
            let path = {
                let model = lua.app_data_ref::<Model>().expect("model");
                usize::try_from(model.container_inventory_slot(slot))
                    .ok()
                    .and_then(|inv| model.inv_slot("player", inv))
                    .and_then(|item| item.icon.clone())
            };
            // 0x5d7180 leaves the existing portrait alone if the bag object is absent.
            let Some(path) = path else {
                return Ok(());
            };
            let rh = super::region::this_texture(lua, &texture)?;
            let mut model = lua.app_data_mut::<Model>().expect("model");
            let data = model.region_data.entry(rh).or_default();
            data.texture = Some(path);
            data.fill = None;
            data.portrait_unit = None;
            data.circular = true;
            Ok(())
        })?,
    )
}

#[cfg(test)]
mod tests {
    use super::super::{InvSlotView, InventorySlots, ScriptDialect, UiScript};

    #[test]
    fn stock_bag_portrait_spelling_handles_backpack_worn_and_seventh_bank_bag() {
        let mut ui = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        let mut slots: InventorySlots = Default::default();
        slots[20] = Some(InvSlotView {
            icon: Some("Interface\\Icons\\Bag1".into()),
            ..Default::default()
        });
        let mut bank: super::super::BankBagSlots = Default::default();
        bank[6] = Some(InvSlotView {
            icon: Some("Interface\\Icons\\Bag11".into()),
            ..Default::default()
        });
        ui.set_inventory_slots(slots);
        ui.set_bank_bag_slots(bank);
        ui.run(
            r#"
            local p = CreateFrame('Frame'):CreateTexture()
            p:SetTexture('backpack')
            assert(select('#', SetBagPortraitTexture(p, 0)) == 0)
            assert(p:GetTexture() == 'backpack')
            SetBagPortraitTexture(p, -2)
            assert(p:GetTexture() == 'backpack')
            SetBagPortraitTexture(p, '1')
            assert(p:GetTexture() == 'Interface\\Icons\\Bag1')
            SetBagPortraitTexture(p, 2)
            assert(p:GetTexture() == 'Interface\\Icons\\Bag1')
            SetBagPortraitTexture(p, 11)
            assert(p:GetTexture() == 'Interface\\Icons\\Bag11')
            assert(not pcall(SetBagPortraitTexture, p, 12))
            assert(not pcall(SetBagPortraitTexture, p, nil))
        "#,
        )
        .unwrap();
        let vanilla = UiScript::new().unwrap();
        vanilla.run("assert(SetBagPortraitTexture == nil); assert(type(SetBagPortaitTexture) == 'function')").unwrap();
    }
}
