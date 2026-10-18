# Wrath live-bug backlog

Manager-owned. Live play-test findings land here during finish mode and are worked one by one
afterwards (see `WRATH-NATIVE-PLAN.md`, "Finish mode"). Missing APIs and unhandled packets are not
listed here; they belong to the workstream backlogs.

| # | Found | Symptom | Evidence | Status |
|---|---|---|---|---|
| 1 | 10-15 | Stuck entering Northshire Abbey; run-in-place, then frozen after relog | `target/azb-live/124331` — false whole-group WMO flooding (groupLiquid 0) → Wrath swim hold | Fixed f2af98c1 |
| 2 | 10-16 | Character eyes glow (DK eye glow on a human warrior) | geoset 1703 always on | Fixed 99f04ad5 |
| 3 | 10-16 | Backpack does not open | `SetBagPortraitTexture` missing | Fixed d94f9756 |
| 4 | 10-18 | Character create offers only 1.12 races/classes (no Blood Elf, Draenei, Death Knight) | App's char_create is the 1.12 screen (found by App Migration while scoping paid services) | Port in flight (App Migration, 12340 create screen) |
