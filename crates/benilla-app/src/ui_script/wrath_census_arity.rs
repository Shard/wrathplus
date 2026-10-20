//! The arity audit: for each native the stock 3.3.5 Lua unpacks into several values
//! (`local a, b, c = Name(…)`, `a, b = obj:Name(…)`, `select(n, Name(…))`), the widest
//! unpack across its call sites against what this VM returns.
//!
//! Asserted against two reviewed lists, the [`NATIVE_ABSENT`](super) pattern: a SHORT or UNPROBED
//! name missing from its list fails the census, and so does a listed name that no longer is one,
//! so both lists only shrink. A probe calls the binding with a few argument shapes against the census VM's
//! state, so a getter whose list is empty there answers its miss shape: an answer with no
//! non-nil value is UNPROBED (its width printed as `miss`), not a pass. A hit narrower than the
//! widest unpack is SHORT, a mismatch candidate.

use std::collections::{BTreeMap, HashSet};

/// Names answering narrower than the stock Lua unpacks, each with why: a 12340 shape (evidence) or
/// a pending fix (owner).
const SHORT_REVIEWED: &[(&str, &str)] = &[
    ("GetBinding", "0x55e8d0: the command, then each bound key; an unbound command answers one"),
    ("GetLFGBootProposal", "0x554a60: false alone when no vote is in progress"),
    ("GetQuestLogTitle", "0x5e5cc0: the tenth (displayQuestID) only with that debug cvar set"),
    ("UnitPowerType", "0x60f100: five only for a vehicle with a PowerDisplay row, else two"),
    ("GetAchievementInfoFromCriteria", "0x5b1720: seven; the stock ten-wide unpack reads nils"),
];

/// Names no probe reaches with a hit or a full-width miss, by reason.
const UNPROBED_REVIEWED: &[&str] = &[
    // No Battle.net service in a 3.3.5 session.
    "BNGetBlockedInfo", "BNGetBlockedToonInfo", "BNGetConversationMemberInfo",
    "BNGetCustomMessageTable", "BNGetFOFInfo", "BNGetFriendInfo", "BNGetFriendInfoByID",
    "BNGetFriendInviteInfo", "BNGetFriendToonInfo", "BNGetInfo", "BNGetNumFOF", "BNGetToonInfo",
    // No knowledge-base service.
    "KBArticle_GetData", "KBSetup_GetCategoryData", "KBSetup_GetSubCategoryData",
    // No debug zone map or debug objects in a retail client.
    "GetDebugZoneMap", "GetMapDebugObjectInfo",
    // No voice-chat service: GetVoiceSessionInfo (`0x7db0b0`) answers one nil without sessions.
    "GetVoiceSessionInfo",
    // QuestPOIFrame's blob hit test answers nothing off a drawn blob (the map's blobs, Wire A).
    "UpdateMouseOverTooltip",
];

/// The widest unpack seen for one name: values wanted, where, and how many unpacking sites.
#[derive(Debug, Default, Clone)]
pub(super) struct Want {
    pub want: usize,
    pub at: String,
    pub sites: usize,
}

/// `(is_method, name)` → the widest unpack, across every chunk.
pub(super) type Wants = BTreeMap<(bool, String), Want>;

fn ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The index just past the `)` matching the `(` at `open`, or `None` at end of text.
fn close_paren(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (k, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// The call's value list survives whole: nothing after the `)` truncates or extends it.
fn ends_expression(rest: &str) -> bool {
    let line = rest.split('\n').next().unwrap_or("").trim_start();
    if line.is_empty() || line.starts_with(';') {
        return true;
    }
    if line.starts_with(|c: char| ",.:[(+-*/^%<>=~".contains(c)) {
        return false;
    }
    let word: String = line.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    word != "or" && word != "and"
}

/// The values a statement prefix unpacks from the call that follows it, when it says.
fn unpacked(prefix: &str) -> Option<usize> {
    let p = prefix.trim_end();
    // `select(n, Name(`
    if let Some(p) = p.strip_suffix(',') {
        let p = p.trim_end();
        let digits = p.len() - p.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        if digits == 0 {
            return None;
        }
        let n: usize = p[p.len() - digits..].parse().ok()?;
        let head = p[..p.len() - digits].trim_end();
        return head.strip_suffix('(')?.trim_end().ends_with("select").then_some(n);
    }
    // `local a, b = Name(` / `a.b, c = Name(`
    let lhs = p.strip_suffix('=')?;
    if lhs.ends_with(|c: char| "=~<>".contains(c)) {
        return None;
    }
    let mut lhs = lhs;
    for cut in [" then ", " do ", " else ", ";", " end "] {
        if let Some((_, after)) = lhs.rsplit_once(cut) {
            lhs = after;
        }
    }
    let lhs = lhs.trim();
    let lhs = lhs.strip_prefix("local ").unwrap_or(lhs).trim();
    if lhs.is_empty() || !lhs.chars().all(|c| c.is_ascii_alphanumeric() || "_., []".contains(c)) {
        return None;
    }
    let n = lhs.split(',').count();
    (n >= 2).then_some(n)
}

/// One chunk's unpacking sites, less the names it binds with `local`.
pub(super) fn scan(src: &str, path: &str, locals: &HashSet<String>, wants: &mut Wants) {
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !(bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') || (i > 0 && ident(bytes[i - 1])) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && ident(bytes[i]) {
            i += 1;
        }
        let name = &src[start..i];
        let mut j = i;
        while j < bytes.len() && bytes[j] == b' ' {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'(' || !name.as_bytes()[0].is_ascii_uppercase() {
            continue;
        }
        let before = if start > 0 { bytes[start - 1] } else { b' ' };
        let method = before == b':';
        if before == b'.' || before == b'"' || before == b'\'' || (!method && locals.contains(name)) {
            continue;
        }
        // The statement starts before the receiver: `self.bar:GetPoint(` walks back over `self.bar`.
        let mut stmt = start;
        if method {
            stmt -= 1;
            while stmt > 0 && (ident(bytes[stmt - 1]) || b".:".contains(&bytes[stmt - 1])) {
                stmt -= 1;
            }
        }
        let line_start = src[..stmt].rfind('\n').map_or(0, |n| n + 1);
        let Some(want) = unpacked(&src[line_start..stmt]) else { continue };
        let Some(end) = close_paren(bytes, j) else { continue };
        if !ends_expression(&src[end..]) {
            continue;
        }
        let line = src[..start].matches('\n').count() + 1;
        let leaf = path.rsplit('\\').next().unwrap_or(path);
        let w = wants.entry((method, name.to_string())).or_default();
        w.sites += 1;
        if want > w.want {
            w.want = want;
            w.at = format!("{leaf}:{line}");
        }
    }
}

/// Probes every unpacked name, prints `ARITY …` lines and asserts them against the reviewed
/// lists; returns (short, unprobed).
pub(super) fn audit(
    script: &benilla_ui::script::UiScript,
    wants: &Wants,
    defined: &HashSet<String>,
) -> (usize, usize) {
    let probe = script
        .lua()
        .load(
            r##"
            local method, name = ...
            local kinds = { "Frame", "Button", "CheckButton", "EditBox", "StatusBar", "Slider",
                "ScrollFrame", "MessageFrame", "ScrollingMessageFrame", "GameTooltip", "Model",
                "PlayerModel", "DressUpModel", "TabardModel", "Minimap", "ColorSelect", "SimpleHTML",
                "Cooldown", "LootButton", "MovieFrame" }
            local target, fn, kind = nil, nil, "METHOD"
            if method then
                for _, kind in ipairs(kinds) do
                    local ok, frame = pcall(CreateFrame, kind)
                    if ok and frame then
                        -- A tooltip showing an item and a chat frame holding a line.
                        if kind == "GameTooltip" then
                            pcall(frame.SetOwner, frame, UIParent, "ANCHOR_NONE")
                            pcall(frame.SetHyperlink, frame, "item:6948")
                        elseif kind == "ScrollingMessageFrame" then
                            pcall(frame.AddMessage, frame, "Hello", 1, 1, 1, 1)
                        end
                        -- Anchored and sized, so the geometry getters answer a hit.
                        pcall(frame.SetPoint, frame, "CENTER", UIParent, "CENTER", 0, 0)
                        pcall(frame.SetWidth, frame, 10)
                        pcall(frame.SetHeight, frame, 10)
                        local group = frame.CreateAnimationGroup and frame:CreateAnimationGroup()
                        for _, o in ipairs({ frame, frame:CreateTexture(), frame:CreateFontString(),
                                group, group and group:CreateAnimation("Alpha") }) do
                            if type(o[name]) == "function" then target, fn = o, o[name] break end
                        end
                    end
                    if fn then break end
                end
                if not fn then return "absent", 0, 0 end
            else
                fn = _G[name]
                if type(fn) ~= "function" then return "absent", 0, 0 end
                -- Lua 5.1's string.dump raises on a C function: a dumpable global is a Lua shim.
                kind = pcall(string.dump, fn) and "SHIM" or "GLOBAL"
            end
            -- Counted inside the protected call: Lua 5.1's pcall pushes its status past `ci->top`
            -- after more than LUA_MINSTACK results (CalendarGetEventInfo's 25), an API-check abort.
            -- The width of one answer, and whether any value in it is non-nil (a hit, not a miss).
            local function count(...)
                local n = select("#", ...)
                for k = 1, n do
                    if select(k, ...) ~= nil then return n, true end
                end
                return n, false
            end
            local best, miss, errors = 0, 0, 0
            -- Indexes, unit tokens, bag/slot pairs, aura filters, item and spell ids, quest-item types,
            -- a unit pair (a member against its mob), the player's GUID, a binding command and the
            -- first channel row past the display list's three headers.
            local ladder = { {}, { 1 }, { "player" }, { 1, 1 }, { 1, 1, 1 }, { "player", 1 },
                { 0, 1 }, { "player", 1, "HELPFUL" }, { "player", 1, "HARMFUL" }, { 6948 },
                { "item:6948" }, { 6603 }, { "reward", 1 }, { "choice", 1 }, { "required", 1 },
                { 2, 1 }, { 1, 1, 1, 1 }, { 1, 2 }, { "CRITTER", 1 }, { "MOUNT", 1 }, { 2 },
                { "player", "target" }, { "HeadSlot" }, { UnitGUID and UnitGUID("player") or "0x0" },
                { "MOVEFORWARD" }, { 4, 1 } }
            for _, args in ipairs(ladder) do
                local ok, n, hit
                if method then
                    ok, n, hit = pcall(function() return count(fn(target, unpack(args))) end)
                else
                    ok, n, hit = pcall(function() return count(fn(unpack(args))) end)
                end
                if not ok then
                    errors = errors + 1
                elseif hit then
                    if n > best then best = n end
                elseif n > miss then
                    miss = n
                end
            end
            return kind, best, miss, errors
            "##,
        )
        .into_function()
        .expect("arity probe");
    let (mut short, mut unprobed) = (0, 0);
    let mut lines = Vec::new();
    let (mut short_names, mut unprobed_names) = (Vec::new(), Vec::new());
    for ((method, name), w) in wants {
        if w.want < 2 || (!*method && defined.contains(name)) {
            continue;
        }
        // A Lua global FrameXML does not define is a compat shim, not a native binding.
        let Ok((kind, got, miss, errors)) =
            probe.call::<(String, usize, usize, usize)>((*method, name.as_str()))
        else {
            continue;
        };
        if kind == "absent" {
            continue;
        }
        // An all-nil answer as wide as the widest unpack shows the width too.
        if got == 0 && miss >= w.want {
            continue;
        }
        if got == 0 {
            unprobed += 1;
            unprobed_names.push(name.clone());
            lines.push(format!(
                "ARITY UNPROBED {kind} {name} want={} miss={miss} sites={} at {} errors={errors}",
                w.want, w.sites, w.at
            ));
        } else if got < w.want {
            short += 1;
            short_names.push(name.clone());
            lines.push(format!("ARITY SHORT {kind} {name} want={} got={got} sites={} at {}", w.want, w.sites, w.at));
        }
    }
    eprintln!("arity: {short} short, {unprobed} unprobed, of {} unpacked names", wants.len());
    for line in lines {
        eprintln!("{line}");
    }
    let unlisted: Vec<&String> = short_names
        .iter()
        .filter(|n| !SHORT_REVIEWED.iter().any(|(r, _)| r == n))
        .chain(unprobed_names.iter().filter(|n| !UNPROBED_REVIEWED.contains(&n.as_str())))
        .collect();
    assert!(unlisted.is_empty(), "arity rows missing from the reviewed lists: {unlisted:?}");
    let stale: Vec<&str> = SHORT_REVIEWED
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| !short_names.iter().any(|s| s == n))
        .chain(UNPROBED_REVIEWED.iter().copied().filter(|n| !unprobed_names.iter().any(|s| s == n)))
        .collect();
    assert!(stale.is_empty(), "reviewed arity rows that no longer apply: {stale:?}");
    (short, unprobed)
}

#[test]
fn unpacking_sites_read_the_widest_target_list() {
    let mut wants = Wants::new();
    let src = "local a, b, c = GetThing(1)\nx, y = self.bar:GetPoint(1)\nlocal z = select(4, GetThing(2))\n\
               local p, q = GetOther() or 1\nif ok then m, n = GetMore(1, 2) end\nlocal s = Single()\n";
    scan(src, r"Interface\FrameXML\X.lua", &HashSet::new(), &mut wants);
    let get = |m: bool, n: &str| wants.get(&(m, n.to_string())).map(|w| (w.want, w.sites));
    assert_eq!(get(false, "GetThing"), Some((4, 2)));
    assert_eq!(get(true, "GetPoint"), Some((2, 1)));
    assert_eq!(get(false, "GetOther"), None);
    assert_eq!(get(false, "GetMore"), Some((2, 1)));
    assert_eq!(get(false, "Single"), None);
    assert_eq!(wants[&(false, "GetThing".to_string())].at, "X.lua:3");
}
