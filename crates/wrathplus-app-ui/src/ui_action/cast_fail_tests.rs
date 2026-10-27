use super::*;
use wrathplus_formats::{TotemCategory, TotemCategoryCatalog};

/// Build 12340's shipped `GlobalStrings.lua` values for the keys these tests read.
fn gs() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("ERR_SPELL_FAILED_S", "%s"),
        ("SPELL_FAILED_TOTEM_CATEGORY", "Requires %s"),
        ("SPELL_FAILED_TOTEMS", "Requires %s"),
        ("SPELL_FAILED_REAGENTS", "Missing reagent: %s"),
        ("ERR_SPELL_FAILED_TOTEMS", "%s"),
        ("ERR_SPELL_FAILED_REAGENTS", "%s"),
        ("SPELL_FAILED_NOT_READY", "Not yet recovered"),
        ("ERR_SPELL_COOLDOWN", "Spell is not ready yet."),
        ("ERR_ABILITY_COOLDOWN", "Ability is not ready yet."),
        ("ERR_POTION_COOLDOWN", "You cannot drink any more yet."),
        ("SPELL_FAILED_OUT_OF_RANGE", "Out of range"),
        ("ERR_SPELL_OUT_OF_RANGE", "Out of range."),
        (
            "ERR_SPELL_FAILED_ALREADY_AT_FULL_POWER_S",
            "You are already at full %s",
        ),
        ("RAGE", "Rage"),
        ("ERR_OUT_OF_RAGE", "Not enough rage"),
        ("SPELL_FAILED_FISHING_TOO_LOW", "Requires Fishing %d"),
        ("SPELL_FAILED_CUSTOM_ERROR_1", "Something bad happened"),
        ("SPELL_FAILED_NOT_MOUNTED", "You are mounted."),
    ])
}

/// Resolve `r` with these strings, items and name tables.
fn resolve(r: u8, spell: Option<&SpellDisplay>, args: FailArgs) -> Resolved {
    let strings = gs();
    let get = |k: &str| strings.get(k).map(|s| s.to_string());
    let item_name = |entry: u32| match entry {
        2901 => ItemName::Named("Mining Pick".into()),
        17056 => ItemName::Named("Light Feather".into()),
        999 => ItemName::Unknown,
        _ => ItemName::Pending,
    };
    let subclass = |_: u32, _: u32| None;
    cast_fail_text(
        r,
        spell,
        args,
        &Lookups {
            get: &get,
            item_name: &item_name,
            subclass: &subclass,
        },
    )
}

fn line(key: &'static str, text: &str) -> Resolved {
    Resolved::Line(CastFailLine {
        key,
        text: text.into(),
    })
}

/// The live failure (session 115138): reason 130 with category 162 names the Blacksmith Hammer,
/// where the 1.12 renumbering showed "Unknown reason".
#[test]
fn totem_category_names_the_required_tool() {
    let cats = TotemCategoryCatalog::from_rows(HashMap::from([(
        162,
        TotemCategory {
            name: "Blacksmith Hammer".into(),
            kind: 2,
            mask: 1,
        },
    )]));
    let args = |arg| FailArgs {
        arg: Some(arg),
        categories: Some(&cats),
        ..FailArgs::default()
    };
    assert_eq!(reason::TOTEM_CATEGORY, 130);
    assert_eq!(
        resolve(130, None, args(162)),
        line(PASSTHROUGH, "Requires Blacksmith Hammer")
    );
    assert_eq!(
        resolve(130, None, args(7)),
        Resolved::Silent,
        "no row says nothing"
    );
}

/// The item arms name the wire's item on their own rows; an item in flight waits, an unknown one
/// reads `UNKNOWN`.
#[test]
fn totems_and_reagents_name_the_item() {
    let arg = |a| FailArgs {
        arg: Some(a),
        ..FailArgs::default()
    };
    assert_eq!(
        resolve(reason::TOTEMS, None, arg(2901)),
        line("ERR_SPELL_FAILED_TOTEMS", "Requires Mining Pick")
    );
    assert_eq!(
        resolve(reason::REAGENTS, None, arg(17056)),
        line(
            "ERR_SPELL_FAILED_REAGENTS",
            "Missing reagent: Light Feather"
        )
    );
    assert_eq!(resolve(reason::REAGENTS, None, arg(5)), Resolved::Await);
    assert_eq!(
        resolve(reason::REAGENTS, None, arg(999)),
        line("ERR_SPELL_FAILED_REAGENTS", "Missing reagent: UNKNOWN")
    );
}

/// `0x807f10`'s cooldown rows fork on the category and the ability bit; the row's own string is
/// the line.
#[test]
fn not_ready_picks_its_row_by_category_and_ability() {
    let spell = |category, attributes| SpellDisplay {
        category,
        attributes,
        ..Default::default()
    };
    let none = FailArgs::default();
    assert_eq!(
        resolve(reason::NOT_READY, Some(&spell(0, 0)), none),
        line("ERR_SPELL_COOLDOWN", "Spell is not ready yet.")
    );
    assert_eq!(
        resolve(reason::NOT_READY, Some(&spell(0, 0x10)), none),
        line("ERR_ABILITY_COOLDOWN", "Ability is not ready yet.")
    );
    assert_eq!(
        resolve(reason::NOT_READY, Some(&spell(4, 0)), none),
        line("ERR_POTION_COOLDOWN", "You cannot drink any more yet.")
    );
    assert_eq!(
        resolve(reason::OUT_OF_RANGE, None, none),
        line("ERR_SPELL_OUT_OF_RANGE", "Out of range.")
    );
}

/// The power arms: full power names our power type's noun on the row, no power takes the spell's
/// power row; DONT_REPORT, an out-of-table power type and the numbered arms.
#[test]
fn power_custom_and_numbered_arms() {
    let rage = SpellDisplay {
        power_type: 1,
        ..Default::default()
    };
    let ours = |t| FailArgs {
        power_type: Some(t),
        ..FailArgs::default()
    };
    assert_eq!(
        resolve(reason::ALREADY_AT_FULL_POWER, None, ours(1)),
        line(
            "ERR_SPELL_FAILED_ALREADY_AT_FULL_POWER_S",
            "You are already at full Rage"
        )
    );
    assert_eq!(
        resolve(reason::ALREADY_AT_FULL_POWER, None, ours(9)),
        Resolved::Silent
    );
    assert_eq!(
        resolve(reason::NO_POWER, Some(&rage), FailArgs::default()),
        line("ERR_OUT_OF_RAGE", "Not enough rage")
    );
    assert_eq!(
        resolve(reason::DONT_REPORT, None, FailArgs::default()),
        Resolved::Silent
    );
    let arg = |a| FailArgs {
        arg: Some(a),
        ..FailArgs::default()
    };
    assert_eq!(
        resolve(reason::FISHING_TOO_LOW, None, arg(75)),
        line(PASSTHROUGH, "Requires Fishing 75")
    );
    assert_eq!(
        resolve(reason::CUSTOM_ERROR, None, arg(1)),
        line(PASSTHROUGH, "Something bad happened")
    );
    assert_eq!(
        resolve(reason::NOT_MOUNTED, None, FailArgs::default()),
        line(PASSTHROUGH, "You are mounted.")
    );
    assert_eq!(
        resolve(0xfe, None, FailArgs::default()),
        line(PASSTHROUGH, "Spell failed (0xfe)")
    );
}

/// On the installed data: the live failure's 130/162 against the shipped GlobalStrings and
/// `TotemCategory.dbc`. Skips without an install.
#[test]
fn the_live_failure_reads_requires_blacksmith_hammer_on_the_installed_data() {
    let Some(data) = wrathplus_formats::wow_data() else {
        eprintln!("skipped: no installed client");
        return;
    };
    let mut chain = wrathplus_formats::open_chain(&data).unwrap();
    let cats = wrathplus_formats::load_totem_categories(&mut chain).unwrap();
    let mut s = wrathplus_ui::script::UiScript::new().unwrap();
    crate::ui_script::test_ui::load_ui(&mut s, "Interface\\FrameXML\\GlobalStrings.lua");
    let get = |k: &str| crate::ui_action::global_string(&s, k);
    let item_name = |_: u32| ItemName::Pending;
    let subclass = |_: u32, _: u32| None;
    let args = FailArgs {
        arg: Some(162),
        categories: Some(&cats),
        ..FailArgs::default()
    };
    assert_eq!(
        cast_fail_text(
            130,
            None,
            args,
            &Lookups {
                get: &get,
                item_name: &item_name,
                subclass: &subclass
            }
        ),
        line(PASSTHROUGH, "Requires Blacksmith Hammer")
    );
}
