//! A guard against request loops: every stock request API answered at once by its stock reply,
//! with no request made from the handler of its own reply (the `FRIENDLIST_SHOW` contact-list
//! storm Wire B found was one).

use wrathplus_ui::script::{GuildState, ScriptValue, UiScript};

/// Each request API and the event its server reply fires.
const REPLIES: &[(&str, &str)] = &[
    ("ShowFriends", "FRIENDLIST_UPDATE"),
    ("GuildRoster", "GUILD_ROSTER_UPDATE"),
    ("SendWho", "WHO_LIST_UPDATE"),
    ("RequestRaidInfo", "UPDATE_INSTANCE_INFO"),
    ("ArenaTeamRoster", "ARENA_TEAM_ROSTER_UPDATE"),
    ("QueryGuildBankTab", "GUILDBANKBAGSLOTS_CHANGED"),
    ("QueryGuildBankText", "GUILDBANK_UPDATE_TEXT"),
    ("QueryGuildBankLog", "GUILDBANKLOG_UPDATE"),
    ("QueryGuildEventLog", "GUILD_EVENT_LOG_UPDATE"),
    ("RequestLFDPlayerLockInfo", "LFG_UPDATE_RANDOM_INFO"),
    ("RefreshLFGList", "UPDATE_LFG_LIST"),
    ("RequestBattlefieldScoreData", "UPDATE_BATTLEFIELD_SCORE"),
    ("QueryAuctionItems", "AUCTION_ITEM_LIST_UPDATE"),
    ("NotifyInspect", "INSPECT_TALENT_READY"),
    ("RequestTimePlayed", "TIME_PLAYED_MSG"),
];

/// Called every frame by stock (`WorldStateFrame.xml:1567` `OnUpdate`); the app's gate
/// (`BattlefieldScoreboard::request_due`, `0x54ce30`) bounds what reaches the wire.
const PER_FRAME: &[&str] = &["RequestBattlefieldScoreData"];

/// The panels shown in turn, each for five seconds, by their stock toggles.
const PANELS: &[&str] = &[
    "ToggleFriendsFrame(1)",
    "ToggleFriendsFrame(2)",
    "ToggleFriendsFrame(3)",
    "ToggleFriendsFrame(5)",
    "TogglePVPFrame()",
    "ToggleLFDParentFrame()",
    "ToggleLFRParentFrame()",
    "WorldStateScoreFrame:Show()",
];

fn wrap(s: &UiScript) {
    let pairs: Vec<String> = REPLIES
        .iter()
        .map(|(api, ev)| format!("{api} = \"{ev}\""))
        .collect();
    s.run(&format!(
        "REQ_COUNT, REQ_LOOP, REQ_PENDING = {{}}, {{}}, {{}} \
         for api, ev in pairs({{ {} }}) do \
           local orig = _G[api] \
           if type(orig) == 'function' then \
             _G[api] = function(...) \
               REQ_COUNT[api] = (REQ_COUNT[api] or 0) + 1 \
               if __REPLY == ev then REQ_LOOP[api] = (REQ_LOOP[api] or 0) + 1 end \
               REQ_PENDING[#REQ_PENDING + 1] = ev \
               return orig(...) \
             end \
           end \
         end",
        pairs.join(", ")
    ))
    .unwrap();
}

/// Answers every queued request with its reply, marking the reply being dispatched.
fn answer(s: &mut UiScript) {
    let pending: String = s
        .eval("local p = REQ_PENDING REQ_PENDING = {} return table.concat(p, ',')")
        .unwrap();
    for event in pending.split(',').filter(|e| !e.is_empty()) {
        let args = if event == "TIME_PLAYED_MSG" {
            vec![ScriptValue::Int(0), ScriptValue::Int(0)]
        } else {
            Vec::new()
        };
        s.run(&format!("__REPLY = '{event}'")).unwrap();
        s.fire_event(event, args);
        s.run("__REPLY = nil").unwrap();
    }
}

/// 60 s of frames over the stock UI in a guild, every request seeded once per panel and answered
/// at once: no API is called from the handler of its own reply, and none but the per-frame
/// scoreboard poll runs more than twice a second on average.
#[test]
fn sixty_seconds_of_instant_replies_loop_no_request() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let (mut s, _failures) = super::super::stock_session(&data);
    s.clear_instruction_budget();
    // In a guild, so the guild tab and its roster requests run.
    s.set_guild(GuildState {
        in_guild: true,
        name: "Bench".into(),
        ..Default::default()
    });
    wrap(&s);
    for frame in 0..3600 {
        if frame % 300 == 0 {
            let panel = PANELS[(frame / 300) % PANELS.len()];
            let _ = s
                .run("HideUIPanel(FriendsFrame) HideUIPanel(PVPFrame) WorldStateScoreFrame:Hide()");
            let _ = s.run(panel);
            // Every request once, so each reply's handlers run with the panel up; a handler that
            // re-requests from its own reply then shows in `REQ_LOOP`. Labelled: an XML
            // `function="X"` handler bound the original global at load and bypasses the count.
            let seed: Vec<String> = REPLIES
                .iter()
                .map(|(api, _)| format!("pcall({api})"))
                .collect();
            let _ = s.run(&seed.join(" "));
        }
        s.tick(1.0 / 60.0);
        answer(&mut s);
    }
    let loops: String = s
        .eval("local t = {} for k, v in pairs(REQ_LOOP) do t[#t + 1] = k..'='..v end table.sort(t) return table.concat(t, ' ')")
        .unwrap();
    assert_eq!(loops, "", "requests made from their own reply's handler");
    let counts: String = s
        .eval("local t = {} for k, v in pairs(REQ_COUNT) do t[#t + 1] = k..'='..v end table.sort(t) return table.concat(t, ' ')")
        .unwrap();
    eprintln!("request counts over 60 s: {counts}");
    for row in counts.split(' ').filter(|r| !r.is_empty()) {
        let (api, n) = row.split_once('=').unwrap();
        let n: u32 = n.parse().unwrap();
        assert!(
            PER_FRAME.contains(&api) || n <= 120,
            "{api} ran {n} times in 60 s"
        );
    }
}
