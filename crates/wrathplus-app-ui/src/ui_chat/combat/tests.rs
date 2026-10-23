//! The client-composed lines' tests: the slot orders against the shipped `GlobalStrings.lua`, and
//! the fill.

use super::*;

/// A template's `%s`/`%d` conversions in order; two templates with the same one take the same
/// arguments.
fn signature(template: &str) -> Vec<char> {
    let mut out = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            continue;
        }
        match chars.next() {
            Some('%') => {}
            Some(k) if k == 's' || k == 'd' => out.push(k),
            _ => {}
        }
    }
    out
}

/// The signature a family's declared slots give.
fn declared_signature(family: Family) -> Vec<char> {
    family
        .slots
        .iter()
        .map(|slot| match slot {
            Slot::Subject | Slot::Named => 's',
            Slot::Amount => 'd',
        })
        .collect()
}

/// Every family's key is a GlobalString of the install, and its declared slots give exactly the
/// template's `%s`/`%d` sequence; a slotless family composes to its template verbatim, literal
/// `%` and all.
#[test]
fn every_family_matches_the_shipped_template() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
    let src = chain
        .read_file("Interface\\FrameXML\\GlobalStrings.lua")
        .expect("GlobalStrings.lua in the chain");
    let script = wrathplus_ui::script::UiScript::new().expect("VM");
    script
        .run(&String::from_utf8_lossy(&src))
        .expect("GlobalStrings runs clean");

    for &family in ALL_FAMILIES {
        let template = global_string(&script, family.key)
            .unwrap_or_else(|| panic!("{} is not a GlobalString", family.key));
        if family.slots.is_empty() {
            assert_eq!(
                compose_line(&script, family, &Fills::default()).as_deref(),
                Some(template.as_str()),
                "{} prints verbatim",
                family.key
            );
            continue;
        }
        assert_eq!(
            declared_signature(family),
            signature(&template),
            "{}: our slots {:?} do not match the shipped {template:?}",
            family.key,
            family.slots,
        );
    }
    let gain = Fills {
        named: "Stormwind".into(),
        amount: 25,
        ..Default::default()
    };
    assert_eq!(
        compose_line(&script, FACTION_STANDING_INCREASED, &gain).as_deref(),
        Some("Reputation with Stormwind increased by 25.")
    );
    let made = Fills {
        subject: "Bob".into(),
        named: "Linen Bandage".into(),
        ..Default::default()
    };
    assert_eq!(
        compose_line(&script, TRADESKILL_LOG_THIRDPERSON, &made).as_deref(),
        Some("Bob creates Linen Bandage.")
    );
    assert_eq!(
        compose_line(&script, TRADESKILL_LOG_FIRSTPERSON, &made).as_deref(),
        Some("You create Linen Bandage.")
    );
}

#[test]
fn the_fill_is_vsnprintf_and_refuses_a_mismatch() {
    fn s(v: &str) -> Arg<'_> {
        Arg::Str(v)
    }
    assert_eq!(
        fill("%s hits %s for %d.", &[s("A"), s("B"), Arg::Num(7)]).as_deref(),
        Some("A hits B for 7.")
    );
    assert_eq!(fill("100%% sure", &[]).as_deref(), Some("100% sure"));
    // A `%d` where a string is queued, and vice versa.
    assert_eq!(fill("%d", &[s("A")]), None);
    assert_eq!(fill("%s", &[Arg::Num(1)]), None);
    // Too few, and too many.
    assert_eq!(fill("%s %s", &[s("A")]), None);
    assert_eq!(fill("%s", &[s("A"), s("B")]), None);
    // A conversion outside the vocabulary is a mismatch, not a passthrough.
    assert_eq!(fill("%f", &[Arg::Num(1)]), None);
}
