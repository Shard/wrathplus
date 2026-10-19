# Wrath live-bug backlog

Manager-owned. Live play-test findings land here during finish mode and are worked one by one
afterwards (see `WRATH-NATIVE-PLAN.md`, "Finish mode"). Missing APIs and unhandled packets are not
listed here; they belong to the workstream backlogs.

| # | Found | Symptom | Evidence | Status |
|---|---|---|---|---|
| 1 | 10-15 | Stuck entering Northshire Abbey; run-in-place, then frozen after relog | `target/azb-live/124331` — false whole-group WMO flooding (groupLiquid 0) → Wrath swim hold | Fixed f2af98c1 |
| 2 | 10-16 | Character eyes glow (DK eye glow on a human warrior) | geoset 1703 always on | Fixed 99f04ad5 |
| 3 | 10-16 | Backpack does not open | `SetBagPortraitTexture` missing | Fixed d94f9756 |
| 4 | 10-18 | Character create offers only 1.12 races/classes (no Blood Elf, Draenei, Death Knight) | App's char_create is the 1.12 screen (found by App Migration while scoping paid services) | Fixed (12340 grid) via ecbc97f3 |

## Installed-data test failures kept after Vanilla removal 6c

Tests that fail on the installed 3.3.5 data (`WOW_CLIENT_PROFILE=wrath WOW_LOCALE=enUS WOW_DATA=…`)
and were kept at 7a25e5d5 because each may mark a real 12340 gap, not just a 1.12 value pin. The
hermetic gates skip them. Evidence is the failing assertion on the installed data. An owner either
fixes the 12340 path or converts the test to 3.3.5 truth with evidence, then flips the row.

| Area | Tests | Evidence on 3.3.5 | Owner | Status |
|---|---|---|---|---|
| Terrain liquid (formats `liquid.rs`, the 1.12 MCLQ reader) | `liquid::tests::{a_river_mouth_chunk_builds_both_its_river_and_its_sea, burning_steppes_lava_builds_a_magma_surface_at_the_reported_pin, each_liquid_kind_rides_its_own_verified_depth_divisor, magma_uvs_come_from_the_vertex_and_run_continuous_across_chunks, parses_elwynn_lake_tile}` | 0 liquid chunks on `Azeroth_32_48` (expected 42), 0 magma chunks on Burning Steppes 33_46: 3.3.5 terrain carries liquid in MH2O, so this reader is a 1.12 lane; check the MH2O path covers each case, then delete or port | Experience | Open |
| WMO liquid | `tests/wmo_liquid.rs::{stormwind_canal_group_builds_still_water, wmo_water_repeats_once_per_grid_cell, the_two_water_arms_carry_their_own_inputs, hole_corner_heights_never_reach_the_mesh_bounds}` | `Stormwind_099.wmo` group carries no water; MOGP flags `0x00002a05` put it on the wrong water arm | Experience | Open |
| WMO chunks | `tests/wmo_chunk_overrun.rs::undercity_144_mogp_survives_its_one_byte_overrun`, `tests/wmo_mocv_short_tail.rs::{undercity_144_keeps_its_bake_despite_a_short_mocv, only_one_group_in_the_corpus_has_a_short_mocv}`, `tests/wmo_doorway_fade.rs::abbey_matches_the_reference_capture` | MOGP size 11818 (1.12 file 11819), MOVT 188 vs 290, no short-MOCV group, Abbey group 1 uploads 519 vertices vs the 1.12 capture's 678: likely data changes; confirm and convert | Experience | Open |
| World liquid | `benilla-world liquid::real_data::{the_rogues_quarter_is_not_under_tirisfals_lake, undercitys_upper_channels_do_not_submerge_the_rooms_below}`, `liquid::surface::real_data::{only_the_nibble_six_and_seven_pools_scroll_in_the_shipped_data, a_wmo_pools_fog_block_follows_its_groups_interior_class}` | Tirisfal water covers the Rogues' Quarter XY; Undercity channel reads Slime where the room expects Water; Stormwind's WMO liquid group count is 0 (expected 22) | Experience | Open |
| Billboard cards | `tests/billboards.rs::candelabra_glow_cards_split_per_bone`, `tests/m2_billboard.rs::lamppost_glow_is_spherical_billboard`, `tests/m2_billboard_card_face.rs::{the_shop_signs_chain_cards_face_away_and_its_body_is_untouched, the_questgiver_marker_is_3d_billboard_geometry_not_a_card}`, `tests/m2_color.rs::bonfire_glow_carries_warm_m2color_tint`, `models::m2_batches::tests::questionmark_marker_batch_carries_both_bob_loops`, `characters::geosets::tests::geoset_visibility_keeps_night_elf_eye_glow` | No batch carries `billboard`: the Wrath adapter keeps billboards on the joint palette (`models/wrath.rs`, "Wrath billboards stay on the ordinary joint-palette path"); decide whether glow cards need the card path on 12340 | Experience | Open |
| M2 batch culling | `tests/chain.rs::{culls_constant_zero_alpha_m2_batches, suppresses_white1_invisible_trap_placeholder}` | A normal prop keeps 4 of 5 batches; the WHITE1 invisible-trap placeholder still draws (its suppression lived only in the deleted 1.12 batch reader). Recheck after the key_anim unkeyed-slot fix | Experience | Open |
| M2 env map | `tests/m2_env_map.rs::weapon_rack_splits_env_from_uv_batches` | No ARMORREFLECT env-mapped layer found on the v264 weapon rack | Experience | Open |
| Character compositing (12340 scaled 512² atlas) | `characters::sections::tests::{composite_body_overlays_land_on_real_human_male, composite_body_dresses_the_female_torso_underwear, only_the_tested_equipment_columns_suppress_the_underwear, composite_body_equipment_layers_land_on_real_human_male, composite_body_paints_the_guild_emblem_on_the_real_tabard}` | Atlas 512² (tests assume 256²); the emblem repaints 40137 texels outside the torso tiles; equipment touches the Hand region (1541 texels) | Character compositing owner | Open |
| Wrath particle reader | `particles::tests::real_fireball_effects_rebase_to_clip_time` | `MoltenBlast_Impact_Chest` plume rate reads 80.24 at 133 ms where the 1.12 file burst at 60. The keyed cell track (InsectSwarm's ten cycles, the brazier's falling pair) is fixed and its tests converted; the 12340 flag bit `0x20000` set on most emitters is still untraced (tests compare the low half) | Assets | Partly fixed |
