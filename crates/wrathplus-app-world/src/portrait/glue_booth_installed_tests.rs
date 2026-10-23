//! The login backdrop against the shipped glue data: the scene's lights and its glow.

use super::*;

/// The stock login screen (`UI_MainMenu_Northrend` on `AccountLogin`'s ModelFFX) is lit by the
/// model's own lights (`AccountLogin` never calls `SetLighting`, so `ResetLights`' defaults stand):
/// one directional with ambient `(183, 212, 237)/255 × 1.3` and no diffuse. 12340 sums the
/// directionals' ambient into the light state (`0x834f60`, `+0x54`) and commits it unclamped next
/// to a diffuse clamped to 1 (`0x873ca0`); the clamped primary colour is ≈ `(0.93, 1, 1)`, the
/// rig lane's `clamp` in `wow_model.wgsl`. The washed-out frame was the glow: the login ModelFFX
/// states `glow="0.08"`, not the 1.12 OnShow pin 0.30.
#[test]
fn installed_login_backdrop_matches_the_stock_glue() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::Chain::open_wrath(&data, "enUS").expect("chain");
    let mut text =
        |path: &str| String::from_utf8_lossy(&chain.read_file(path).expect(path)).into_owned();
    let login = text(r"Interface\GlueXML\AccountLogin.xml");
    let ffx = login
        .lines()
        .find(|l| l.contains(r#"<ModelFFX name="AccountLogin""#))
        .expect("AccountLogin's ModelFFX");
    assert!(ffx.contains(r#"glow="0.08""#), "{ffx}");
    assert_eq!(wrathplus_world::ffx_glow::LOGIN_GLOW_MILLI, 80);
    let parent = text(r"Interface\GlueXML\GlueParent.lua");
    for row in [
        r#"CharModelGlowInfo["HUMAN"] = 0.15;"#,
        r#"CharModelGlowInfo["DWARF"] = 0.15;"#,
        "model:SetGlow(0.3);",
    ] {
        assert!(parent.contains(row), "GlueParent.lua: {row}");
    }
    use wrathplus_world::ffx_glow::scene_glow_milli;
    assert_eq!(scene_glow_milli("Human"), 150);
    assert_eq!(scene_glow_milli("Dwarf"), 150);
    assert_eq!(scene_glow_milli("NightElf"), 300);

    let m2_path = r"Interface\GLUES\MODELS\UI_MainMenu_Northrend\UI_MainMenu_Northrend.M2";
    let m2 = chain.read_file(m2_path).expect("login model");
    let skin = chain
        .read_file(&format!("{}00.skin", &m2_path[..m2_path.len() - 3]))
        .expect("skin 00");
    let model = wrathplus_formats::parse_wrath_render_model(&m2, &skin, "", &[]).expect("parse");
    let lights: Vec<_> = model
        .lights
        .iter()
        .cloned()
        .map(|def| wrathplus_assets::ModelLight {
            def,
            bone_pivot: [0.0; 3],
        })
        .collect();
    let rig = scene_rig(&lights);
    let byte = |c: f32| (c * 255.0).round();
    assert_eq!(rig.ambient.map(|c| byte(c / 1.3)), [183.0, 212.0, 237.0]);
    assert!(
        rig.lobes.iter().all(|(_, c)| *c == [0.0; 3]),
        "no diffuse: {:?}",
        rig.lobes
    );
    assert!(rig.points.is_empty());
    // The committed primary colour: GL clamps the unclamped ambient per vertex.
    let primary = rig.ambient.map(|c| c.clamp(0.0, 1.0));
    assert!((primary[0] - 0.933).abs() < 1e-3 && primary[1] == 1.0 && primary[2] == 1.0);
}
