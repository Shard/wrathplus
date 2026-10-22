//! The archive's Blizzard addons outside the third-party walk: the startup one loaded with the
//! core, and the LoadOnDemand rows listed for a session with no walk.

use wrathplus_ui::script::UiScript;

use super::{chain_addons, info_for, Addon, Walk, LOAD_INSTRUCTION_BUDGET, WRATH_BLIZZARD_ADDONS};

/// The archive's LoadOnDemand Blizzard rows alone, for a session load with no addon walk: the
/// stock client always lists them, and stock FrameXML loads some itself (`UIParent_OnEvent`'s
/// `PLAYER_LOGIN` runs `CombatLog_LoadUI`, UIParent.lua:480-483). A caller that registered its
/// own addons keeps them; no saved-variable folders are attached.
fn register_wrath_blizzard_on_demand(script: &mut UiScript) {
    if script.eval::<i64>("return GetNumAddOns()").unwrap_or(0) > 0 {
        return;
    }
    let infos: Vec<_> = chain_addons(&WRATH_BLIZZARD_ADDONS)
        .iter()
        .filter(|addon| addon.toc.load_on_demand())
        .map(|addon| {
            let mut info = info_for(addon);
            info.enabled = addon.toc.default_state();
            info
        })
        .collect();
    if infos.is_empty() {
        return;
    }
    script.set_addon_chain_reader(Box::new(super::super::reference_ui::read));
    script.register_addons(infos, None, None, None);
}

/// The archive's startup Blizzard addons (`Blizzard_TokenUI`), loaded right after the core, as a
/// clean login has them. Outside the registry walk, whose rows stay the LoadOnDemand ones, which
/// it lists when no caller has registered any; no folder is read.
pub(in super::super) fn load_wrath_startup_blizzard(script: &mut UiScript) -> Vec<String> {
    register_wrath_blizzard_on_demand(script);
    let startup: Vec<Addon> = chain_addons(&WRATH_BLIZZARD_ADDONS)
        .into_iter()
        .filter(|addon| !addon.toc.load_on_demand())
        .collect();
    let mut state = Walk {
        version_check: true,
        client_interface: 30300,
        ..Walk::default()
    };
    for addon in &startup {
        script.set_instruction_budget(LOAD_INSTRUCTION_BUDGET);
        let _ = state.load(script, &startup, &addon.name);
    }
    state.failures
}
