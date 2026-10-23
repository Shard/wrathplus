use super::*;

/// Load `paths` through the MPQ source and this loader, as the client does.
fn load_installed(paths: &[&str]) -> Option<Vec<M2Model>> {
    let data = wrathplus_formats::wow_data_or_skip!(None);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    crate::register_mpq_source(&mut app, &data).expect("register mpq source");
    app.add_plugins(bevy::asset::AssetPlugin::default());
    app.init_asset::<Image>();
    app.init_asset::<Mesh>();
    app.init_asset::<SkinnedMeshInverseBindposes>();
    app.init_asset::<bevy::animation::AnimationClip>();
    app.init_asset::<AnimationGraph>();
    crate::register_asset_loaders(&mut app);
    let server = app.world().resource::<AssetServer>().clone();
    let handles: Vec<Handle<M2Model>> = paths
        .iter()
        .map(|p| server.load(format!("mpq://{}", p.replace('\\', "/"))))
        .collect();
    // A generous ceiling: HumanMale reads 47 `.anim` files, and a parallel gate starves IO.
    for _ in 0..30_000 {
        app.update();
        let models = app.world().resource::<Assets<M2Model>>();
        if handles.iter().all(|h| models.get(h).is_some()) {
            return Some(
                handles
                    .iter()
                    .map(|h| models.get(h).unwrap().clone())
                    .collect(),
            );
        }
        for (path, h) in paths.iter().zip(&handles) {
            if let Some(bevy::asset::LoadState::Failed(e)) = server.get_load_state(h) {
                panic!("{path}: {e}");
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("{paths:?} did not load");
}

/// The 3.3.5 rig carries what the ranged flex, the clip-event sounds and the weapon grip need:
/// an emitter-only rifle keeps its sequence clips, BowRelease clamps, a player body's clips
/// carry their authored event keys, and both hands get a HandsClosed grip node.
#[test]
fn installed_wrath_models_keep_clips_events_and_the_grip() {
    let Some(models) = load_installed(&[
        "Item\\ObjectComponents\\Weapon\\Firearm_2H_Rifle_A_01.m2",
        "Item\\ObjectComponents\\Weapon\\Bow_1H_Standard_A_01.m2",
        "Character\\Human\\Male\\HumanMale.m2",
    ]) else {
        return;
    };
    let [rifle, bow, human] = &models[..] else {
        unreachable!()
    };

    // The rifle poses no bone; its seven muzzle emitters sample the sequence clock.
    assert_eq!(rifle.emitters.len(), 7);
    let anims = rifle
        .animations
        .as_ref()
        .expect("an emitter-only rifle still gets its clips — the blast forks BowRelease");
    let ids: Vec<(u16, bool)> = anims.clips.iter().map(|c| (c.anim_id, c.looping)).collect();
    assert_eq!(
        ids,
        vec![(0, true), (161, false)],
        "Stand loops, BowRelease clamps"
    );

    let bow = bow.animations.as_ref().expect("the bow flexes its limbs");
    assert!(bow.owns(160) && bow.owns(161), "BowPull and BowRelease");
    assert!(
        bow.clips.iter().all(|c| !c.looping),
        "a bow's clips all clamp"
    );

    let human_anims = human.animations.as_ref().expect("the player rig");
    let keyed = |ident: &[u8; 4]| -> Vec<u16> {
        let mut ids: Vec<u16> = human_anims
            .clips
            .iter()
            .filter(|c| c.events.iter().any(|e| e.ident == *ident))
            .map(|c| c.anim_id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    assert_eq!(
        keyed(b"$BWP"),
        vec![105, 106],
        "LoadBow/LoadRifle key the draw"
    );
    assert_eq!(
        keyed(b"$BWR"),
        vec![46, 49],
        "AttackBow/AttackRifle key the release"
    );
    assert!(
        human_anims
            .clips
            .iter()
            .flat_map(|c| c.events.iter())
            .any(|e| e.ident == *b"$FSD"),
        "footstep keys reach the clips"
    );
    assert!(
        human_anims
            .clips
            .iter()
            .any(|c| c.anim_id == 0 && c.looping)
            && human_anims
                .clips
                .iter()
                .any(|c| c.anim_id == 1 && !c.looping),
        "Stand loops and Death clamps"
    );
    assert!(
        human_anims.hand_close.iter().all(Option::is_some),
        "both hands close on a held weapon"
    );
}

/// Every glue stage the 3.3.5 screens stand: the login's Lich King (`UI_MainMenu_Northrend`)
/// and the nine select/create stages (no Gnome or Troll stage ships). Each loads through the
/// client's loader with its stage point (attachment 0, where the character stands) and an
/// authored light rig to fold.
#[test]
fn installed_glue_stages_load_with_a_stage_point_and_lights() {
    let names = [
        "MainMenu_Northrend",
        "Human",
        "Orc",
        "Dwarf",
        "NightElf",
        "Scourge",
        "Tauren",
        "BloodElf",
        "Draenei",
        "DeathKnight",
    ];
    let paths: Vec<String> = names
        .iter()
        .map(|n| format!("Interface\\Glues\\Models\\UI_{n}\\UI_{n}.m2"))
        .collect();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let Some(models) = load_installed(&refs) else {
        return;
    };
    for (name, model) in names.iter().zip(&models) {
        let moving = model.cameras.first().is_some_and(|c| c.tracks.is_some());
        eprintln!(
            "UI_{name}: {} light(s), attachments {:?}, camera 0 moving {moving}",
            model.lights.len(),
            model.attachments.iter().map(|a| a.id).collect::<Vec<_>>()
        );
        if *name == "Scourge" {
            // The animated stage that used to fail the load: its camera keeps its v264
            // tracks and samples to a finite rig through sequence 0.
            assert!(moving, "UI_Scourge's camera 0 moves");
            let rig = model.cameras[0].at(0, 2_000);
            assert!(
                rig.eye.is_finite() && rig.target.is_finite(),
                "{:?} -> {:?}",
                rig.eye,
                rig.target
            );
        }
        assert!(!model.lights.is_empty(), "UI_{name} has no light rig");
        if *name != "MainMenu_Northrend" {
            assert!(
                model.attachments.iter().any(|a| a.id == 0),
                "UI_{name} has no stage point"
            );
        }
    }
}
