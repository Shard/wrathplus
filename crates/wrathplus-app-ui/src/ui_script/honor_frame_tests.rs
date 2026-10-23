//! The PvP window's honor section, engine only: stock `PVPFrame.lua`'s own `PVPFrame_OnEvent`
//! and `PVPHonor_Update` over the honor bindings, painting the labels `PVPFrame.xml` names. The
//! labels are bare font strings here; the rest of the window (arena teams, `PVPFrame_Update`) is
//! not this section's.

use wrathplus_ui::script::{HonorState, UiScript, UnitState, WrathCurrencySnapshot};

/// Every figure distinct, so a swapped pair of rows fails.
fn state() -> HonorState {
    HonorState {
        today_hk: 17,
        today_honor: 640,
        yesterday_hk: 41,
        yesterday_honor: 1_250,
        lifetime_hk: 3_907,
        highest_rank: 14,
    }
}

fn points(honor: u32, arena: u32) -> WrathCurrencySnapshot {
    WrathCurrencySnapshot {
        honor_current: Some(honor),
        arena_current: Some(arena),
        ..Default::default()
    }
}

/// The labels `PVPHonor_Update` writes.
const LABELS: &[&str] = &[
    "PVPHonorTodayKills",
    "PVPHonorTodayHonor",
    "PVPHonorYesterdayKills",
    "PVPHonorYesterdayHonor",
    "PVPHonorLifetimeKills",
    "PVPFrameHonorPoints",
    "PVPFrameArenaPoints",
];

/// The install's `PVPFrame.lua`, with `PVPFrame` registered for the events its `OnLoad` registers
/// and dispatching to its own `OnEvent`.
fn pvp_frame(data: &std::path::Path) -> UiScript {
    let mut chain = wrathplus_formats::open_chain(data).expect("open chain");
    let src = chain
        .read_file("Interface\\FrameXML\\PVPFrame.lua")
        .expect("PVPFrame.lua in the chain");
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    s.run(&String::from_utf8_lossy(&src))
        .expect("PVPFrame.lua runs");
    let labels = LABELS
        .iter()
        .map(|n| format!("F:CreateFontString({n:?}, \"ARTWORK\")"))
        .collect::<Vec<_>>()
        .join("\n");
    s.run(&format!(
        r#"F = CreateFrame("Frame", "PVPFrame")
           {labels}
           F:SetScript("OnEvent", PVPFrame_OnEvent)
           F:RegisterEvent("PLAYER_PVP_KILLS_CHANGED")
           F:RegisterEvent("PLAYER_PVP_RANK_CHANGED")
           F:RegisterEvent("HONOR_CURRENCY_UPDATE")"#
    ))
    .expect("labels");
    s
}

fn text(s: &mut UiScript, frame: &str) -> String {
    s.eval::<String>(&format!("return {frame}:GetText()"))
        .unwrap_or_else(|e| panic!("{frame}:GetText(): {e}"))
}

#[test]
fn every_figure_lands_in_its_own_row() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut s = pvp_frame(&data);
    s.set_honor(Some(state()));
    s.set_wrath_currency_snapshot(points(8_431, 57));
    s.fire_event("PLAYER_PVP_KILLS_CHANGED", vec![]);
    s.resolve();
    for (frame, want) in [
        ("PVPHonorTodayKills", "17"),
        ("PVPHonorTodayHonor", "640"),
        ("PVPHonorYesterdayKills", "41"),
        ("PVPHonorYesterdayHonor", "1250"),
        ("PVPHonorLifetimeKills", "3907"),
        ("PVPFrameHonorPoints", "8431"),
        ("PVPFrameArenaPoints", "57"),
    ] {
        assert_eq!(text(&mut s, frame), want, "{frame}");
    }
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// Before the first push every getter answers zeros, so the section paints zeros, not an error.
#[test]
fn a_new_character_paints_zeros() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut s = pvp_frame(&data);
    s.fire_event("PLAYER_PVP_KILLS_CHANGED", vec![]);
    s.resolve();
    for frame in LABELS {
        assert_eq!(text(&mut s, frame), "0", "{frame}");
    }
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// An honor gain moves today's contribution and the points together; the currency event the app
/// fires on the points repaints the whole section.
#[test]
fn a_currency_update_repaints_the_section() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut s = pvp_frame(&data);
    s.set_honor(Some(state()));
    s.set_wrath_currency_snapshot(points(8_431, 57));
    s.fire_event("PLAYER_PVP_KILLS_CHANGED", vec![]);
    s.resolve();

    s.set_honor(Some(HonorState {
        today_honor: 700,
        ..state()
    }));
    s.set_wrath_currency_snapshot(points(8_491, 57));
    s.fire_event("HONOR_CURRENCY_UPDATE", vec![]);
    s.resolve();
    assert_eq!(text(&mut s, "PVPHonorTodayHonor"), "700");
    assert_eq!(text(&mut s, "PVPFrameHonorPoints"), "8491");
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// The key we build, `PVP_RANK_<internal>_<team>`, against the install's own `GlobalStrings.lua`,
/// for both teams.
#[test]
fn the_real_global_strings_name_the_rank() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
    let src = chain
        .read_file("Interface\\FrameXML\\GlobalStrings.lua")
        .expect("GlobalStrings.lua in the chain");

    let mut s = UiScript::new().unwrap();
    s.run(&String::from_utf8_lossy(&src)).expect("runs clean");
    for (team, want) in [(1i8, "Knight-Captain"), (0, "Legionnaire")] {
        s.set_unit(
            "player",
            Some(UnitState {
                exists: true,
                // `0x51a9af` gates the unit-token arm on a player.
                is_player: true,
                pvp_team: team,
                ..UnitState::default()
            }),
        );
        let (name, visual) = s
            .eval::<(String, i64)>(r#"return GetPVPRankInfo(12, "player")"#)
            .unwrap();
        assert_eq!((name.as_str(), visual), (want, 8), "rank 12 to team {team}");
    }
}
