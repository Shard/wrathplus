//! Build 12340's description arms beyond 1.12's (`0x579e50`): `${…}` expressions (`0x579980`,
//! [`super::super::expr`]), `$<name>` description variables (`0x577c40`, defined by the spell's
//! `SpellDescriptionVariables` row, `0x579d60`) and `$?cond[then][else]` (`0x579ba0`).
//!
//! A failed arm prints `$` (`0xa11570`) and the text resumes where the arm stopped reading: past
//! the `}` of a failed expression, at the `<` of an unknown variable.

use super::super::expr::{self, Operand};
use super::{duration_ms, effect_points, modify_float, modify_int, SpellDisplay, TokenContext};
use crate::TokenNumber;

/// The active player, as the 12340 arms read it; every default is "no player".
pub trait PlayerTokens {
    /// `$?s<id>`: the spell is in the player's book (`0x7260e0`).
    fn knows(&self, _spell: u32) -> bool {
        false
    }
    /// `$?a<id>`: the aura is on the player (`0x7282a0`).
    fn has_aura(&self, _spell: u32) -> bool {
        false
    }
    /// `UNIT_FIELD_BYTES_0` byte 2: `$g` takes its second arm on non-zero (`0x576cd0`).
    fn gender(&self) -> u8 {
        0
    }
    /// An expression's player value by its table name (`STR`, `AP`, `pl`, `SPH`, …, `0x5782d0`);
    /// `None` reads 0, as the reference does without a player.
    fn value(&self, _name: &str) -> Option<f32> {
        None
    }
}

/// One defined variable (a `0x38`-byte slot of `[0xbeb670]`): its name, printed text, value and
/// rounded value, which `$l` keys on.
pub(super) struct Variable {
    name: String,
    text: String,
    value: f32,
    rounded: i32,
}

/// The spell's variables, in definition order; a later definition reads the earlier ones.
#[derive(Default)]
pub(super) struct Variables(Vec<Variable>);

impl Variables {
    fn get(&self, name: &str) -> Option<&Variable> {
        self.0.iter().find(|v| v.name == name)
    }
}

/// An expression's token reads for one described spell (`0x5782d0`).
struct Values<'a, 'c> {
    spell: &'a SpellDisplay,
    ctx: &'a TokenContext<'c>,
    level: u32,
    vars: &'a Variables,
}

impl Operand for Values<'_, '_> {
    fn token(&self, index: u8, spell: Option<u32>) -> Option<f32> {
        let (d, level) = match spell {
            Some(id) => {
                let s = (self.ctx.lookup)(id)?;
                let level = if s.max_level > 0 {
                    self.level.min(s.max_level)
                } else {
                    self.level
                };
                (s, level)
            }
            None => (self.spell, self.level),
        };
        Some(token(index, d, self.ctx, level))
    }

    fn variable(&self, name: &str) -> f32 {
        self.vars.get(name).map_or(0.0, |v| v.value)
    }
}

/// `0x5782d0`'s value for token op `index` on `d`. Letter pairs read alike but `m`/`M`, the
/// minimum and the maximum; the `q` arms index past their table (`0x5782d0`) and read 0.
fn token(index: u8, d: &SpellDisplay, ctx: &TokenContext, level: u32) -> f32 {
    let slot = |first: u8| usize::from((index - first) % 3);
    match index {
        32..=37 => {
            let (min, max) = effect_points(d, slot(32), ctx, level);
            if index < 35 {
                min
            } else {
                max
            }
        }
        38..=43 => ctx
            .radii
            .get(d.effect_radius_index[slot(38)])
            .map_or(0.0, |r| modify_float(ctx, d, 6, r.radius)),
        44 | 45 => duration_ms(d, ctx) as f32 * 0.001,
        50..=55 => modify_int(ctx, d, 17, d.effect_chain_targets[slot(50)] as i32) as f32,
        56..=61 if d.proc_flags & 1 != 0 => 5000.0,
        56..=61 => modify_int(ctx, d, 19, d.effect_amplitude[slot(56)] as i32) as f32,
        62 | 63 => modify_int(ctx, d, 18, d.proc_chance as i32) as f32,
        64 | 65 => modify_int(ctx, d, 4, d.proc_charges as i32) as f32,
        66..=71 => d.effect_points_per_combo_point[slot(66)],
        72 | 73 => d.stack_amount as f32,
        74 | 75 => d.max_target_level as f32,
        76..=81 => modify_float(ctx, d, 27, d.effect_multiple_value[slot(76)]),
        82 | 83 => d.max_affected_targets as f32,
        84..=89 => d.damage_multiplier[slot(84)],
        90..=95 => 0.0,
        156..=161 => modify_float(ctx, d, 24, d.effect_bonus_multiplier[slot(156)]),
        // The player's own values, and `$c`/`$p` (`0x8012f0`, `0x7ff100`), which are untraced.
        _ => ctx
            .player
            .and_then(|p| p.value(expr::token_name(index)))
            .unwrap_or(0.0),
    }
}

/// An expression's printed text, value and rounded value.
type Printed = (String, f32, i32);

/// `0x579980` on the body of the `{` at `open`: the printed value, its rounded value and the
/// position after the `}` and any `.N` precision (`%.Nf`, else `%.0f`); `None` on failure, with
/// the position after the `}`. `None` overall when there is no `}`.
fn braced(text: &str, open: usize, values: &Values) -> Option<(Option<Printed>, usize)> {
    let close = open + text[open..].find('}')?;
    let bytes = text.as_bytes();
    let (format, next) = match (bytes.get(close + 1), bytes.get(close + 2)) {
        (Some(b'.'), Some(d @ b'1'..=b'9')) => (format!("%.{}f", *d as char), close + 3),
        _ => ("%.0f".to_string(), close + 1),
    };
    let Some(value) = expr::evaluate(&text[open + 1..close], values) else {
        return Some((None, next.min(close + 1)));
    };
    let printed = (values.ctx.printf)(&format, &[TokenNumber::Float(f64::from(value))]);
    Some((Some((printed, value, value.round_ties_even() as i32)), next))
}

/// `0x579d60`: each `$name=` line of the row, defined by its `${…}` or by the chosen bracket's
/// `${…}` of a `$?` (`0x579ba0` with its define flag); anything else defines nothing.
pub(super) fn define(spell: &SpellDisplay, ctx: &TokenContext, level: u32) -> Variables {
    let mut vars = Variables::default();
    let Some(text) = spell.description_variables.as_deref() else {
        return vars;
    };
    let mut at = text.find('$');
    while let Some(dollar) = at {
        let name_start = dollar + 1;
        let Some(eq) = text[name_start..].find('=').map(|e| name_start + e) else {
            break;
        };
        let name: String = text[name_start..eq].chars().take(14).collect();
        let Some(value_at) = text[name_start..].find('$').map(|p| name_start + p + 1) else {
            break;
        };
        let values = Values {
            spell,
            ctx,
            level,
            vars: &vars,
        };
        // The brace to evaluate, and where the scan resumes: past a chain's last `]`, past `}`.
        let (open, mut resume) = match text.as_bytes().get(value_at) {
            Some(b'{') => (Some(value_at), value_at + 1),
            Some(b'?') => match choose(text, value_at + 1, &values) {
                Some((start, end, next)) => (text[start..end].find('{').map(|b| start + b), next),
                None => (None, value_at + 1),
            },
            _ => (None, value_at),
        };
        if let Some(open) = open {
            let (defined, next) = braced(text, open, &values).unwrap_or((None, open + 1));
            if text.as_bytes()[value_at] == b'{' {
                resume = next;
            }
            vars.0.push(match defined {
                Some((printed, value, rounded)) => Variable {
                    name,
                    text: printed,
                    value,
                    rounded,
                },
                // A failed definition still takes its slot, unnamed (`0x579b30`).
                None => Variable {
                    name: String::new(),
                    text: String::new(),
                    value: 0.0,
                    rounded: 0,
                },
            });
        }
        at = text[resume..].find('$').map(|p| resume + p);
    }
    vars
}

/// `0x577030` then `0x5797f0` from `at` (just past `?`): the chosen bracket's inner span and the
/// position past the last `]` of the chain; `None` without a well-formed `[…][…]`.
fn choose(text: &str, at: usize, values: &Values) -> Option<(usize, usize, usize)> {
    let bytes = text.as_bytes();
    let mut cond = at;
    while bytes.get(cond) == Some(&b' ') {
        cond += 1;
    }
    let open = cond + text[cond..].find('[')?;
    let close = open + text[open..].find(']')?;
    if bytes.get(close + 1) == Some(&b'?') {
        // A chained `?`: its own chain supplies the else and the end (`0x577030`).
        let rest = choose(text, close + 2, values)?;
        return Some(if condition(&bytes[cond..open], values) {
            (open + 1, close, rest.2)
        } else {
            rest
        });
    }
    let else_open = close + text[close..].find('[')?;
    let else_close = else_open + text[else_open..].find(']')?;
    Some(if condition(&bytes[cond..open], values) {
        (open + 1, close, else_close + 1)
    } else {
        (else_open + 1, else_close, else_close + 1)
    })
}

/// `0x5797f0` over one condition: `s<id>` a known spell (`0x7260e0`), `a<id>` an aura on the
/// player (`0x7282a0`), `!` negating the next test or group, `&`/`|` short-circuiting
/// right-associatively and `(…)` grouping; anything else is skipped.
fn condition(text: &[u8], values: &Values) -> bool {
    fn walk(text: &[u8], pos: &mut usize, values: &Values) -> bool {
        let mut result = false;
        while let Some(&c) = text.get(*pos) {
            match c {
                b'!' => {
                    *pos += 1;
                    while let Some(&c) = text.get(*pos) {
                        if b"sSaA".contains(&c) {
                            result = !test(text, pos, values);
                            break;
                        }
                        if c == b'(' {
                            *pos += 1;
                            result = !walk(text, pos, values);
                            *pos += 1;
                            break;
                        }
                        *pos += 1;
                    }
                }
                b'&' if !result => return false,
                b'|' if result => return true,
                b'&' | b'|' => {
                    *pos += 1;
                    result = walk(text, pos, values);
                }
                b'(' => {
                    *pos += 1;
                    result = walk(text, pos, values);
                    *pos += 1;
                }
                b')' => return result,
                b's' | b'S' | b'a' | b'A' => result = test(text, pos, values),
                _ => *pos += 1,
            }
        }
        result
    }
    fn test(text: &[u8], pos: &mut usize, values: &Values) -> bool {
        let letter = text[*pos];
        *pos += 1;
        let digits = text[*pos..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if digits == 0 {
            return false;
        }
        let id = text[*pos..*pos + digits].iter().fold(0u32, |n, b| {
            n.wrapping_mul(10).wrapping_add(u32::from(b - b'0'))
        });
        *pos += digits;
        let player = values.ctx.player;
        if letter.eq_ignore_ascii_case(&b'a') {
            player.is_some_and(|p| p.has_aura(id))
        } else {
            player.is_some_and(|p| p.knows(id))
        }
    }
    walk(text, &mut 0, values)
}

/// What one 12340 arm at `at` (just past a `$`) produced.
pub(super) enum Arm {
    /// Text to append, the value `$l` keys on if the arm sets one, and where the text resumes.
    Text(String, Option<f64>, usize),
    /// A failed arm: `$`, then the text from here.
    Failed(usize),
}

/// The `{`, `<` and `?` arms, `None` for any other `$`. `expand` runs the whole expander over a
/// chosen bracket (`0x57aaf0`), `None` when a token inside failed.
pub(super) fn arm(
    text: &str,
    at: usize,
    spell: &SpellDisplay,
    ctx: &TokenContext,
    level: u32,
    vars: &Variables,
    expand: &dyn Fn(&str) -> Option<String>,
) -> Option<Arm> {
    let values = Values {
        spell,
        ctx,
        level,
        vars,
    };
    match text.as_bytes().get(at)? {
        b'{' => {
            let (value, next) = braced(text, at, &values)?;
            Some(match value {
                Some((printed, _, rounded)) => Arm::Text(printed, Some(f64::from(rounded)), next),
                None => Arm::Failed(next),
            })
        }
        b'<' => {
            let close = at + text[at..].find('>')?;
            let name: String = text[at + 1..close].chars().take(15).collect();
            Some(match vars.get(&name) {
                Some(v) => Arm::Text(v.text.clone(), Some(f64::from(v.rounded)), close + 1),
                None => Arm::Failed(at),
            })
        }
        b'?' => Some(match choose(text, at + 1, &values) {
            Some((start, end, next)) => match expand(&text[start..end]) {
                Some(inner) => Arm::Text(inner, None, next),
                // `0x579ba0` appends `$` for the bracket and fails, so its caller adds another.
                None => Arm::Failed(next),
            },
            None => Arm::Text(String::new(), None, at + 1),
        }),
        _ => None,
    }
}

/// `$/N;` or `$*N;` just after the `$` (`507738`-`507790`): the text up to the `;` (at most seven
/// characters) read by `atoi` (`0x64ac60`), the scale its value or, for `/`, its reciprocal in
/// single precision (`507784`); a zero leaves 1.0 (`507774`). Returns the scale and the index past
/// the `;`, or `None` without a `;`, where the reference consumes nothing.
pub(super) fn scale_prefix(text: &str, at: usize) -> Option<(f32, usize)> {
    let op = *text.as_bytes().get(at)?;
    if op != b'/' && op != b'*' {
        return None;
    }
    let semi = at + text[at..].find(';')?;
    let digits = &text.as_bytes()[at + 1..semi.min(at + 8)];
    let n = atoi(digits);
    let scale = match (n, op) {
        (0, _) => 1.0,
        (n, b'/') => (1.0 / f64::from(n)) as f32,
        (n, _) => n as f32,
    };
    Some((scale, semi + 1))
}

/// The CRT's `atoi`: leading whitespace, an optional sign, then decimal digits up to the first
/// other character; nothing parsed is 0.
fn atoi(bytes: &[u8]) -> i32 {
    let mut rest = bytes
        .iter()
        .copied()
        .skip_while(u8::is_ascii_whitespace)
        .peekable();
    let negative = match rest.peek() {
        Some(b'-') => {
            rest.next();
            true
        }
        Some(b'+') => {
            rest.next();
            false
        }
        _ => false,
    };
    let n = rest.take_while(u8::is_ascii_digit).fold(0i32, |n, b| {
        n.wrapping_mul(10).wrapping_add(i32::from(b - b'0'))
    });
    if negative {
        n.wrapping_neg()
    } else {
        n
    }
}

#[cfg(test)]
#[path = "tokens_wrath_tests.rs"]
mod tests;
