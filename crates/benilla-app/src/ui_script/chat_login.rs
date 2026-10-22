//! The chat windows' login events, as the app fires them, for the test kits.

/// `PLAYER_LOGIN`, then the chat loader's two login events (`0x498a60`): `UPDATE_CHAT_WINDOWS`
/// once, then `UPDATE_CHAT_COLOR` per registry entry.
pub(crate) fn fire_chat_login(s: &mut benilla_ui::script::UiScript) {
    // `UIParent_OnEvent`'s PLAYER_LOGIN loads `Blizzard_CombatLog` (UIParent.lua:480-483), which
    // the combat log window's tab and filters call into.
    s.fire_event("PLAYER_LOGIN", vec![]);
    s.fire_event("UPDATE_CHAT_WINDOWS", vec![]);
    let renorm = |b: u8| f64::from(b as f32 * (1.0f32 / 255.0f32));
    for entry in s.chat_colors() {
        s.fire_event(
            "UPDATE_CHAT_COLOR",
            vec![
                benilla_ui::script::ScriptValue::Str(entry.name),
                benilla_ui::script::ScriptValue::Number(renorm(entry.rgb[0])),
                benilla_ui::script::ScriptValue::Number(renorm(entry.rgb[1])),
                benilla_ui::script::ScriptValue::Number(renorm(entry.rgb[2])),
            ],
        );
    }
}
