//! The 12340 arms on synthetic rows, then on the shipped `Spell.dbc`: one real row per form, and
//! a sweep that no installed text keeps a token 12340 expands.

use super::super::{substitute, PlayerTokens, TokenContext, TokenNumber};
use crate::{SpellDisplay, SpellDurationCatalog, SpellRadiusCatalog};

/// `%d` and `%.Nf`, as the CRT prints the values these tests use.
fn printf(template: &str, args: &[TokenNumber]) -> String {
    let mut out = template.to_string();
    for arg in args {
        let at = out.find('%').expect("a hole per argument");
        let end = at + out[at..].find(|c: char| c.is_ascii_alphabetic()).unwrap() + 1;
        let text = match (arg, &out[at..end]) {
            (TokenNumber::Int(n), _) => n.to_string(),
            (TokenNumber::Float(x), spec) => {
                let p: usize = spec[2..spec.len() - 1].parse().unwrap_or(6);
                format!("{x:.p$}")
            }
        };
        out.replace_range(at..end, &text);
    }
    out
}

fn global(key: &str) -> Option<String> {
    Some(
        match key {
            "INT_SPELL_DURATION_SEC" | "INT_SPELL_DURATION_SEC_P1" => "%d sec",
            "INT_SPELL_DURATION_MIN" => "%d min",
            "INT_SPELL_POINTS_SPREAD_TEMPLATE" => "%d to %d",
            "SPELL_POINTS_SPREAD_TEMPLATE" => "%.1f to %.1f",
            "SPELL_DURATION_UNTIL_CANCELLED" => "until cancelled",
            k if k.starts_with("INT_SPELL_DURATION") => "%d units",
            k if k.starts_with("SPELL_DURATION") => "%.2f units",
            _ => return None,
        }
        .into(),
    )
}

#[derive(Default)]
struct Player {
    known: Vec<u32>,
    gender: u8,
    level: f32,
}

impl PlayerTokens for Player {
    fn knows(&self, spell: u32) -> bool {
        self.known.contains(&spell)
    }
    fn gender(&self) -> u8 {
        self.gender
    }
    fn value(&self, name: &str) -> Option<f32> {
        matches!(name, "pl" | "PL").then_some(self.level)
    }
}

fn context<'a>(
    durations: &'a SpellDurationCatalog,
    radii: &'a SpellRadiusCatalog,
    lookup: &'a dyn Fn(u32) -> Option<&'a SpellDisplay>,
    player: &'a Player,
) -> TokenContext<'a> {
    TokenContext {
        durations,
        radii,
        ranges: None,
        skill: &|_| 0,
        lookup,
        mods: None,
        unmodified_points: false,
        home_area: &|| None,
        global: &global,
        printf: &printf,
        player: Some(player),
    }
}

/// A flat `points` in slot 0, with `vars` as its description variables.
fn row(points: i32, vars: Option<&str>) -> SpellDisplay {
    SpellDisplay {
        effect_base_points: [points - 1, 0, 0],
        effect_base_dice: [1, 0, 0],
        effect_die_sides: [1, 0, 0],
        description_variables: vars.map(str::to_string),
        ..Default::default()
    }
}

fn render(text: &str, d: &SpellDisplay, player: &Player) -> String {
    let (durations, radii) = (
        SpellDurationCatalog::default(),
        SpellRadiusCatalog::default(),
    );
    let other = row(30, None);
    let lookup = |id: u32| (id == 77).then_some(&other);
    substitute(text, d, &context(&durations, &radii, &lookup, player))
}

#[test]
fn an_expression_prints_through_its_precision() {
    let d = row(42, None);
    let p = Player::default();
    assert_eq!(
        render("Restores ${$m1/5*18} mana.", &d, &p),
        "Restores 151 mana."
    );
    assert_eq!(render("${$m1/5*18}.2 each", &d, &p), "151.20 each");
    assert_eq!(render("${$77m1+$m1}", &d, &p), "72");
    // A failed expression is a bare `$`; the text resumes past its `}`.
    assert_eq!(render("a ${$nosuch} b", &d, &p), "a $ b");
}

#[test]
fn a_conditional_takes_its_bracket_by_the_players_book() {
    let d = row(10, None);
    let mut p = Player::default();
    let text = "Hits$?s100[ and slows][]. $?s1[one]?s2[two][none]!";
    assert_eq!(render(text, &d, &p), "Hits. none!");
    p.known = vec![100, 2];
    assert_eq!(render(text, &d, &p), "Hits and slows. two!");
    p.known = vec![1, 2];
    assert_eq!(render(text, &d, &p), "Hits. one!");
    // Tokens inside the chosen bracket expand; `!`, `|` and `&` combine tests.
    p.known = vec![5];
    assert_eq!(render("$?!s9&s5[$s1][x]", &d, &p), "10");
    assert_eq!(render("$?s9|s6[y][n]", &d, &p), "n");
}

#[test]
fn variables_define_in_order_and_read_by_name() {
    let vars = "$a=$?s7[${103}][${100}]\r\n$b=$?s8[${106}][${$<a>}]\r\n$c=${$<b>*2}";
    let d = row(10, Some(vars));
    let mut p = Player::default();
    assert_eq!(render("$<b>% and $<c>", &d, &p), "100% and 200");
    p.known = vec![7];
    assert_eq!(render("$<b>% and $<c>", &d, &p), "103% and 206");
    // An unknown name prints `$` and leaves its text.
    assert_eq!(render("$<zz>", &d, &p), "$<zz>");
}

#[test]
fn gender_picks_the_second_arm_off_zero() {
    let d = row(1, None);
    let mut p = Player::default();
    assert_eq!(render("$ghis:her; blade", &d, &p), "his blade");
    p.gender = 1;
    assert_eq!(render("$ghis:her; blade", &d, &p), "her blade");
}

fn real_context(
    chain: &mut crate::Chain,
) -> (
    crate::SpellCatalog,
    SpellDurationCatalog,
    SpellRadiusCatalog,
) {
    (
        crate::load_spell_catalog(chain).expect("Spell.dbc"),
        crate::load_spell_durations(chain).expect("SpellDuration.dbc"),
        crate::load_spell_radii(chain).expect("SpellRadius.dbc"),
    )
}

/// One shipped row per 12340 form. Skips without client data.
#[test]
fn shipped_rows_expand_each_form() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let (spells, durations, radii) = real_context(&mut chain);
    let lookup = |id: u32| spells.get(id);
    let p = Player {
        level: 80.0,
        ..Default::default()
    };
    let p_known = Player {
        known: vec![61331],
        ..Default::default()
    };
    let (plain, knowing) = (
        context(&durations, &radii, &lookup, &p),
        context(&durations, &radii, &lookup, &p_known),
    );
    let run = |id: u32, ctx: &TokenContext| {
        let d = spells.get(id).unwrap_or_else(|| panic!("spell {id}"));
        substitute(d.description.as_deref().expect("a description"), d, ctx)
    };
    // Drink (430), Refreshing Spring Water's: `${$m2/5*18}` over `$d`.
    let drink = run(430, &plain);
    assert!(
        drink.starts_with("Restores 151 mana over 18 sec."),
        "{drink}"
    );
    // Sinister Strike (1757): `$<percent>` from its variables row's `$?s` chain.
    assert!(run(1757, &plain).contains("100%"), "{}", run(1757, &plain));
    assert!(
        run(1757, &knowing).contains("115%"),
        "{}",
        run(1757, &knowing)
    );
    for id in [430, 1757, 70771, 16914] {
        let out = run(id, &plain);
        assert!(
            !out.contains("${") && !out.contains("$?") && !out.contains("$<"),
            "{id}: {out}"
        );
    }
}

/// Every shipped description and aura text, expanded: no `${`, `$?` or `$<` survives, and what
/// does keep a `$` is 12340's own failure output, `$` and the text past a missing row's id or a
/// malformed token. Skips without client data.
#[test]
fn no_shipped_text_keeps_a_token_the_reference_expands() {
    let data = crate::wow_data_or_skip!();
    let mut chain = crate::open_chain(&data).expect("open chain");
    let (spells, durations, radii) = real_context(&mut chain);
    let lookup = |id: u32| spells.get(id);
    let p = Player::default();
    let ranges = crate::load_spell_ranges(&mut chain).ok();
    let ctx = TokenContext {
        ranges: ranges.as_ref(),
        ..context(&durations, &radii, &lookup, &p)
    };
    let mut left = 0;
    for (id, d) in spells.iter() {
        for text in [&d.description, &d.aura_description].into_iter().flatten() {
            let out = substitute(text, d, &ctx);
            for (at, _) in out.match_indices('$') {
                let rest = &out[at + 1..];
                assert!(
                    !rest.starts_with(['{', '?', '<']),
                    "{id}: {}",
                    &out[at..(at + 40).min(out.len())]
                );
                // An id the catalog holds expands; a missing one drops its digits.
                assert!(
                    !rest.starts_with(|c: char| c.is_ascii_digit()),
                    "{id}: {}",
                    &out[at..(at + 40).min(out.len())]
                );
                left += 1;
            }
        }
    }
    // The census of 12340's own failures on this data: missing rows, `$/N` without `;`, `$.`,
    // `$%`, and `$PlayerName` (the `P` arm, `0x7ff100`, untraced).
    assert!(left <= 186, "{left} leftover `$`");
}
