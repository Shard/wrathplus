//! Dedicated-account Wrath protocol probe; creation and initial login require explicit flags.
use anyhow::{bail, Context, Result};
use wrathplus_protocol::{wrath_auth, wrath_world::WrathWorldSession};

fn main() -> Result<()> {
    let host = std::env::var("AZEROTHBENCH_AUTH_HOST").unwrap_or_else(|_| "127.0.0.1:3724".into());
    let username =
        std::env::var("AZEROTHBENCH_TEST_USER").context("dedicated test username required")?;
    let password =
        std::env::var("AZEROTHBENCH_TEST_PASSWORD").context("dedicated test password required")?;
    if !username.eq_ignore_ascii_case("AZBENCH") {
        bail!("this development probe is restricted to dedicated AZBENCH account");
    }
    let login = wrath_auth::logon(&host, &username, &password)?;
    println!("Wrath auth verified; realms={}", login.realms.len());
    for realm in &login.realms {
        println!(
            "realm={} address={} id={}",
            realm.name, realm.address, realm.id
        );
    }
    if std::env::var("AZEROTHBENCH_ENUM").as_deref() == Ok("1") {
        let realm = login.realms.first().context("no realm advertised")?;
        let mut session = WrathWorldSession::connect(
            realm.address.as_str(),
            &username,
            login.session_key,
            u32::from(realm.id),
        )?;
        let mut characters = session.enumerate_characters()?;
        if std::env::var("AZEROTHBENCH_CREATE").as_deref() == Ok("1")
            && !characters
                .iter()
                .any(|character| character.name == "Benchone")
        {
            session.create_character(&wrathplus_protocol::wrath_world::WrathCreateCharacter {
                name: "Benchone".into(),
                race: 1,
                class: 1,
                gender: 0,
                skin: 0,
                face: 0,
                hair_style: 0,
                hair_color: 0,
                facial_hair: 0,
                outfit_id: 0,
            })?;
            characters = session.enumerate_characters()?;
            println!("Normal test character creation verified");
        }
        println!("Wrath world auth verified; characters={}", characters.len());
        if std::env::var("AZEROTHBENCH_ENTER").as_deref() == Ok("1") {
            let character = characters
                .iter()
                .find(|character| character.name == "Benchone")
                .context("dedicated Benchone character missing")?;
            let observed = session.observe_initial_login(character.guid)?;
            println!("Initial world login packets={} verify_world={} account_data={} tutorial={} time_speed={}",
                observed.packets.len(), observed.login_verify_world_count,
                observed.account_data_times_count, observed.tutorial_flags_count,
                observed.login_set_time_speed_count);
        }
        for character in characters {
            println!(
                "character={} level={} map={}",
                character.name, character.level, character.map
            );
        }
    }
    Ok(())
}
