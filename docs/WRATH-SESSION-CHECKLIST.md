# Wrath 3.3.5 session acceptance checklist

The bridge from "every family is wired" to the bug-by-bug phase. One full session walks a character
from Northshire through Goldshire and Stormwind to Northrend and exercises everything a player does
on the way. Each item says what to do, what native 3.3.5 does, how much can be proven without a
human, and what only play can judge. A failed item becomes a row in `WRATH-BUG-BACKLOG.md`.

## Evidence classes

| Code | What | Who runs it |
|---|---|---|
| **H** | Headless, no client: `cargo test` suites, installed-data tests (`WOW_CLIENT_PROFILE=wrath WOW_LOCALE=enUS WOW_DATA=…`), the asset route (`benilla-formats --test wrath_asset_route`), and offline replay of a recorded capture through `WrathEventAdapter::dispatch` | any worker |
| **P** | Scripted live probe against the local dev realm: `target/azb-live/probe.sh` with `WOW_PROBE*` variables (`WOW_PROBE=list` prints them; `WOW_PROBE_CHAT` parks the character with `.go xyz`, `WOW_PROBE_LUA` presses buttons through the live UI VM, `WOW_PROBE_KEY` synthesizes keys, `WOW_LIVE_SHOT` screenshots) | the manager only; it opens a window |
| **R** | Packet record and replay: `WOW_WRATH_PACKET_RECORD=<new file>` on a P or S run (`WRATH-PACKET-RECORDING.md`; inbound, decrypted, never committed), then an H replay that counts unhandled opcodes and decode failures per leg. The replay harness does not exist yet (the recorder has no playback); building it is the first follow-up | manager records, any worker replays |
| **S** | Shannon plays with `target/azb-live/launch.sh`: feel, visuals, timing, sound, anything a probe cannot judge | Shannon |

Rules: workers never launch the client; probes never run while Shannon plays on the same account;
captures stay local. A probe marked *(1.12-era)* was written against vmangos and must be re-pointed
at Wrath before its verdict counts.

## Session setup

1. Fresh character per run (`WOW_PROBE_CHARCREATE`, or the select screen in S): a Human Warrior for
   the route, plus a Death Knight for the 3.3.5-only start (runes, vehicles, phasing).
2. One capture file per leg (Northshire, Goldshire, Stormwind, sea crossing, Borean Tundra,
   Dalaran), named `<date>-<leg>.azbwrpk`.
3. After each leg, the H replay prints: opcodes seen, handled / capture-only / unhandled counts,
   and every decode failure with its opcode. Any decode failure or an unhandled opcode not listed as
   out of scope in `WRATH-OPCODE-COVERAGE.md` is a finding.

## A. Login, character select and world entry

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| A0 | Login screen (stock GlueXML, the default) | `launch.sh`; click into Account Name, type, Tab to Password, type, Enter; open the Realmlist button, change the address, Okay; then Cancel/Quit | The stock 3.3.5 AccountLogin over the Northrend main-menu scene. The Realmlist button is our one non-stock element: it edits and saves the logon address. The `--server`/`--account`/`--password-env` flags still log in unattended. `WOW_NATIVE_GLUE=1` brings back the native Rust screens (the fallback, removed after the first sessions) | H: glue host tests; installed glue/login/realm_select/char_select/char_create tests | Typing goes to the box you clicked, with a caret; Tab, Enter and Esc; every click lands on its button; the look (scene, fog, fonts, button art); the Realmlist button and its dialog |
| A1 | Realm and character list | Log in, read the realm list and characters | Stock RealmList and CharacterSelect: realm tabs and rows, the character list with 3.3.5 races/classes, the race stage, models and gear | P: `probe-identity.sh`; H: auth/char-enum codec tests | Clicking a realm, a character, double-click to enter; rotate buttons; the stage per race (Death Knight's own); models, gear |
| A2 | Character create | Create Human Warrior and Draenei, Blood Elf, Death Knight | Stock CharacterCreate: 3.3.5 grid (10 races, 10 classes, DK gated by a level-55 character), customization, random name | P: `WOW_PROBE_CHARCREATE` (create + delete) | Customization previews, the race stages |
| A3 | Enter world | Select and enter | Loading screen for the zone, intro cinematic on first login, tutorial flags | R: login leg replays clean; H: session-meta tests | Cinematic plays, loading art matches zone |
| A4 | Addons and account data | Open AddOns list, change a key binding, relog | Stock addons listed; bindings/macros persist via account data | H: session-meta capture tests | Binding survives relog |
| A5 | Logout and relog | `/logout`, `/camp`, relog | 20 s countdown out of rest, instant in inn; same position and state on relog | P: `WOW_PROBE_LUA` Logout + relog | Countdown, return to select |

## B. Movement and transport

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| B1 | Ground movement | Run, walk, strafe, backpedal, jump, turn with mouse | Server accepts every heartbeat; no rubber-banding | P: `WOW_PROBE_KEY`, `WOW_PROBE_LOOK`; R: no movement corrections | Feel, camera |
| B2 | Collision | Walk Northshire Abbey, Goldshire inn stairs, Stormwind gates | No falling through floors or walls (backlog #1 regression) | H: asset route (WMO groups, liquids); P: `.go xyz` parks | Doors, stairs, ramps |
| B3 | Swimming | Goldshire lake, Stormwind canals: swim, dive, surface | Swim state, breath bar after ~60 s submerged, pitch while swimming | P: `WOW_PROBE_PITCH`; H: mirror-timer tests (01D9–01DB) | Breath bar, water visuals |
| B4 | Falling | Jump from Stormwind walls | Fall damage scaled by height | R: environmental damage log | Damage feels right |
| B5 | Mounting | Learn riding and summon a mount at 20 | Cast, mount model, 60% speed; dismount on combat | P: `WOW_PROBE_LUA` CastSpellByName | Mount model, animations |
| B6 | Flight path | Stormwind gryphon master: discover, fly to Sentinel Hill and back | Taxi map, path flight, landing at destination | P: `WOW_PROBE=taxi`; H: flight tests | Path smoothness, camera |
| B7 | Boat crossing | Stormwind harbor boat to Valiance Keep | Transport boards, map seam survives, arrival in Borean Tundra | P: `WOW_PROBE=crossing` with `WOW_PROBE_DOCK` | Ride feel, loading |
| B8 | Flying | Cold Weather Flying at 77: fly, ascend, descend, land | Flight speed and pitch; dismounting in the air drops the character, with fall damage | P: `WOW_PROBE_KEY` space/X; R: flight speed orders | Control feel |
| B9 | Vehicles | A Borean Tundra vehicle quest; DK Eye of Acherus | Vehicle UI, seat switching, exit, vehicle abilities on the bar | R: vehicle opcodes (Wire A M4); H: vehicle DBC tests | Vehicle UI, camera |
| B10 | Hearthstone | Bind at an inn, hearth back | 10 s cast, teleport to bind point, cooldown | P: `WOW_PROBE_BINDER`, `WOW_PROBE=castcancel` | Cast bar, cooldown |

## C. Combat, death and resurrection

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| C1 | Melee | Auto-attack Northshire wolves | Swing timer, hit/miss/crit floaters, rage | P: `WOW_PROBE=melee`; H: combat-log tests | Animations, floaters |
| C2 | Spells and cooldowns | Cast class spells, trinket, potion | Cast bar, GCD, cooldown sweeps, mana/rage/runes | H: spell-control tests; R: spell-go/failure | Sweep visuals |
| C3 | Auras | Buffs/debuffs on self and target | Aura icons with timers, dispel types | H: aura tracker tests | Icon layout |
| C4 | Loot | Kill and loot; group loot rolls | Loot window, roll frames, money split | H: loot codec tests; R | Window behaviour |
| C5 | Death | Die to mobs | Release prompt, ghost run, corpse arrow | H: death tests; R | Ghost visuals |
| C6 | Spirit healer | Resurrect at the spirit healer | XP-loss confirm (0222), resurrection sickness | H: world-objects tests (0222) | Confirm dialog |
| C7 | Corpse | Resurrect at the corpse | Reclaim after delay, at corpse position | R | Timing |

## D. Questing

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| D1 | Accept | Northshire quests from Deputy Willem | Quest log entry, objectives, tracker | H: quest tests; R | Gossip and quest frame text |
| D2 | Objectives | Kill, collect, use-object objectives | Tracker counts update | R: quest update packets | Tracker |
| D3 | GameObject quests | A wanted-poster or crate quest | Use object (00B1), quest offered/credited | P: `WOW_PROBE_GOQUEST`, `WOW_PROBE_CHEST` | Highlight, interaction |
| D4 | Turn in | Return and choose a reward | XP, money, reputation (QuestFactionReward), item | H: quest reward DBC tests | Reward frame |
| D5 | Map POIs | Open world map with quests active | Quest blobs/POIs on the map, zone discovery | H: AreaPOI/WorldMapArea installed tests | Map art |
| D6 | Daily reset | Check daily quest reset time | `GetQuestResetTime` from 01CF | H: session-misc tests | — |

## E. NPC services

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| E1 | Vendor | Buy, sell, buyback, repair in Goldshire | Merchant frame, money changes, durability | P: `WOW_PROBE_VENDOR_SWAP` *(1.12-era)* | Frame |
| E2 | Trainer | Learn class spells in Stormwind | Trainer list, costs, learn | P: `WOW_PROBE_LUA` | Spellbook update |
| E3 | Innkeeper | Bind at Goldshire inn | Bind confirm | P: `WOW_PROBE_BINDER` | — |
| E4 | Bank | Deposit, withdraw, buy slot | Bank frame, bag slots | P: `WOW_PROBE_BANK` *(1.12-era)* | Frame |
| E5 | Gossip and guards | Ask a Stormwind guard for directions | POI flag on the map | P: `WOW_PROBE=guardpoi` | Map marker |
| E6 | Service ladder | Right-click NPCs of each flag shape | The right window opens | P: `WOW_PROBE_SERVICE` | — |

## F. Inventory and items

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| F1 | Bags | Open backpack and bags | All bags open (backlog #3 regression) | P: `WOW_PROBE_LUA` | Bag art |
| F2 | Equip | Equip, swap, split, destroy | Character frame and model update | H: inventory tests | Model gear |
| F3 | Containers | Open a clam or lockbox | Loot window | P: `WOW_PROBE_CLAM` | — |
| F4 | Equipment manager | Save and use a set | Swap, `EQUIPMENT_SWAP_FINISHED` | H: equipment-set tests | — |
| F5 | Item text | Read a book or plaque | Item text frame | P: `WOW_PROBE_BOOK` | Text rendering |
| F6 | Sockets and enchants | Socket a gem at level 70+ | Socket frame | H: socket tests | Frame |

## G. Chat and social

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| G1 | Say, yell, emote | Speak in Goldshire | Chat lines, bubbles | P: `WOW_PROBE_CHAT`; H: chat tests | Bubbles |
| G2 | Whisper | Whisper the partner client | Whisper both ways, reply key | P: `WOW_PROBE=partner` second client | — |
| G3 | Channels | General, Trade, LookingForGroup on zone change | Auto-join, roster, leave on zone change | H: channel tests | — |
| G4 | Friends and ignore | Add, remove, notes | List and online status | H: contacts tests | — |
| G5 | Who | `/who` | Who list | H | — |
| G6 | Report and complain | Report a chat line as spam | CanComplainChat finds the line; COMPLAIN sent (03C7) | H: help tests, UnitPopup host tests | Menu entries |

## H. Group and instances

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| H1 | Invite | Invite the partner client | Invite dialog, party frames | P: `WOW_PROBE=partner` | Frames |
| H2 | Leader functions | Loot method, promote, kick, convert to raid | Settings change, roster updates | H: party tests | — |
| H3 | Difficulty | Set dungeon/raid difficulty as leader; try as member | Leader changes it; member gets ERR_NOT_LEADER as a system line | H: UnitPopup host tests | Menu |
| H4 | Ready check and roles | Ready check, role poll | Dialogs, results | H | — |
| H5 | Instances | Enter a dungeon; reset; saved instances | Portal, instance info, reset messages | H: instance-control tests; R | Loading |

## I. Mail, auction house and guild

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| I1 | Mail | Send money and an item, take, delete, COD | Inbox, attachments, money | P: `WOW_PROBE_MAIL` *(1.12-era)* | Frames |
| I2 | Auction house | Browse, bid, buy out, sell, cancel in Stormwind | Lists, owner and bidder tabs | P: `WOW_PROBE_AUCTION` *(1.12-era)* | Frames |
| I3 | Guild charter | Buy, collect signatures, turn in | Charter item, signatures | P: `WOW_PROBE_CHARTER` | — |
| I4 | Guild | Invite, ranks, MOTD, notes, tabard | Guild frame, roster | H: guild tests | Tabard |
| I5 | Guild bank | Deposit, withdraw, tabs, logs | Vault frame | H: guild-bank tests | Frame |

## J. Group finder and PvP

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| J1 | Dungeon finder | Queue as a role for a random dungeon | Role check, proposal, teleport in, back out | H: LFG L1/L2 tests; R | Eye icon, proposal |
| J2 | Raid browser | List and browse | Raid browser entries | H: raid-browser tests | — |
| J3 | Battleground | Queue WSG at the Stormwind battlemaster | Queue, port, scoreboard, leave | P: `WOW_PROBE_BGQUEUE`, `WOW_PROBE_BG=wsg` | Scoreboard |
| J4 | PvP flag | `/pvp`, SetPVP from the portrait menu | Flag set (one-byte TOGGLE_PVP) | H: UnitPopup host tests | Portrait icon |
| J5 | Duel | Duel the partner | Countdown, end, flag | P: `WOW_PROBE=partner` | — |

## K. Professions and gathering

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| K1 | Learn | Learn Mining and Herbalism in Stormwind | Skill lines | H: SkillLine tests | — |
| K2 | Gather | Mine a vein, pick an herb near Goldshire | Use object (00B1), gather cast, loot | P: `WOW_PROBE_CHEST`-style park at a node | Node highlight |
| K3 | Craft | Smelt ore, craft an item | Trade-skill frame, cast, product | P: `WOW_PROBE_LUA` | Frame |
| K4 | Fishing | Fish from the Stormwind canal | Bobber, bite splash (00B3 anim 0), loot or "not hooked"/"escaped" | H: world-objects tests | Bobber animation |
| K5 | Unlearn | Unlearn a profession | Confirm, skill removed (0202) | H: session-misc tests | — |

## L. Character progression and UI

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| L1 | Level up | Gain a level | Level-up text, stats | H: session-meta (01D4) | Effects |
| L2 | Talents and dual spec | Spend talents, preview, activate second spec | Talent frame, glyphs | H: inspect/talent tests, glyph DBC tests | Frame |
| L3 | Achievements | Earn an achievement | Toast, achievement frame | H: achievement tests | Toast |
| L4 | Calendar | Open, create an event | Calendar frame | H: calendar tests | — |
| L5 | Titles and barber | Set a title; barber shop change | Title shown; appearance changes | H: appearance tests | Model |
| L6 | Companions and mounts | Summon and dismiss a pet, mount | Companion tab, dismiss sends 048D | H: companion tests | Models |
| L7 | Help request | File, edit, abandon a GM ticket; report lag | Help frame, ticket status | P: `WOW_PROBE_GMTICKET` | Frame |
| L8 | Every major window | Open each from its micro button, key or NPC: character tabs, spellbook, talents, quest log, bags, world map, friends/who/guild, dungeon finder, PvP, achievements, calendar, macros, key bindings, mail, merchant, trade skill, auction, bank; and the pet, possess and vehicle bars | Each opens with no Lua error | H: `installed_wrath_opens_every_major_window` (census command; 23 windows, 3 bars) | See L9 |
| L9 | The server state those windows need | Play to it, then open the window: level 10 (talents, PvP); level 15 (dungeon finder); a backpack (bags); guild membership (guild tab); at least one earned achievement, which `SMSG_ALL_ACHIEVEMENT_DATA` must report (achievements); a pet with an action bar (pet bar); a possess spell, e.g. Mind Control (possess bar); a vehicle with a UI seat, e.g. a Death Knight start-area vehicle (vehicle bar); a mailbox, vendor, profession, auctioneer and banker (their windows) | The stock toggle opens nothing below its gate; with the state, the window opens | H seeds each one; only play proves the server sends it | Each window opens |

## M. World presentation

| # | Item | Steps | Native behaviour | H / P / R | S |
|---|---|---|---|---|---|
| M1 | Terrain, models, WMOs | Walk the whole route | No missing assets | H: asset route (Northshire, Goldshire, Stormwind, Valiance Keep, Howling Fjord, Dalaran) | Visual parity |
| M2 | Effects | Spell effects, ribbons, lights, torches | Trails, flickering lights, particles | H: effect installed tests | Look |
| M3 | Weather, music, sound | Zone music, weather, object sounds | Music changes by zone, weather packets | H: session-meta and world-objects tests | Sound |
| M4 | Phasing | DK start area, Northrend phased quest areas | Phase-shift mask changes what is visible | H: capture test (047C); phasing is server-side | Visibility |
| M5 | Lighting | Day/night, zone light overrides (Wintergrasp, raids) | Light override (0412) | H: capture only (no renderer consumer yet) | Look |
| M6 | Doors and animated objects | Stormwind gates, elevators | Object animation, transport elevators | P: `.go xyz` park + screenshot | Animation |

## Reporting

Per run: the capture replay summary per leg, every failed row with its evidence (probe log, capture
leg, screenshot path), and a backlog row for each. Rows with no P or H coverage (marked S only)
are the ones to watch most closely in play.
