//! Build 12340's spell-description expressions, the body of a `${…}` (`0x579980`): parsed into a
//! postfix program (`0x5778b0`) and run on a 32-float stack (`0x578d10`).
//!
//! - Lexing (`0x576bb0`): leading spaces skipped, then one delimiter of `()[]^*/%+-#,> ` or a run
//!   of anything else, so `$pl`, `$<glyph` and `1.5` are single words.
//! - Grammar: `expr := term (('+'|'-') term)*`, `term := power (('*'|'/'|'%') primary ('^'
//!   primary)*)*`, `power := primary ('^' primary)*`. An operator is read at the raw position
//!   after its left operand, spaces not skipped, so `${42 / 5}` is 42.
//! - Primary (`0x577530`): `(expr)`, unary `-`, `$<name>` (a description variable, 0 when
//!   undefined), `$func(a, …)` (twelve functions, matched ignoring case), `$[spell id]token` (140
//!   names, case-sensitive), else a number through `atof`.
//! - Every intermediate is a single-precision float, as the reference stores each result.

/// What an expression reads besides its literals.
pub(super) trait Operand {
    /// Token `index` (`22..=161`, [`TOKENS`]) of spell `spell`, or of the described spell for
    /// `None`; `None` when that spell has no record, which fails the expression (`0x578d10`).
    fn token(&self, index: u8, spell: Option<u32>) -> Option<f32>;
    /// A description variable's value (`0x5774d0`): 0 when it is not defined.
    fn variable(&self, name: &str) -> f32;
}

/// `0x576b10`'s table from `0xace898`: name and argument count, ops 10 to 21.
const FUNCTIONS: [(&str, usize); 12] = [
    ("abs", 1),
    ("ceil", 1),
    ("floor", 1),
    ("min", 2),
    ("max", 2),
    ("gt", 2),
    ("lt", 2),
    ("gte", 2),
    ("lte", 2),
    ("eq", 2),
    ("cond", 3),
    ("clamp", 3),
];

/// `0x576b50`'s table from `0xace8f8`: the 140 value tokens, ops 22 to 161.
pub(super) const TOKENS: [&str; 140] = [
    "STR", "AGI", "STA", "INT", "SPI", "str", "agi", "sta", "int", "spi", "m1", "m2", "m3", "M1",
    "M2", "M3", "a1", "a2", "a3", "A1", "A2", "A3", "d", "D", "c", "C", "p", "P", "x1", "x2", "x3",
    "X1", "X2", "X3", "t1", "t2", "t3", "T1", "T2", "T3", "h", "H", "n", "N", "b1", "b2", "b3",
    "B1", "B2", "B3", "u", "U", "v", "V", "e1", "e2", "e3", "E1", "E2", "E3", "i", "I", "f1", "f2",
    "f3", "F1", "F2", "F3", "q1", "q2", "q3", "Q1", "Q2", "Q3", "ap", "AP", "rap", "RAP", "mwb",
    "MWB", "owb", "OWB", "rwb", "RWB", "mw", "MW", "ow", "OW", "rw", "RW", "ar", "AR", "mws",
    "MWS", "ows", "OWS", "rws", "RWS", "pl", "PL", "hnd", "HND", "sp", "SP", "sph", "spfi", "spn",
    "spfr", "sps", "spa", "SPH", "SPFI", "SPN", "SPFR", "SPS", "SPA", "bh", "BH", "ph", "pfi",
    "pn", "pfr", "ps", "pa", "PH", "PFI", "PN", "PFR", "PS", "PA", "pbh", "pBH", "pbhd", "pBHD",
    "bc1", "bc2", "bc3", "BC1", "BC2", "BC3",
];

/// The table name of token op `index`.
pub(super) fn token_name(index: u8) -> &'static str {
    TOKENS[usize::from(index) - 22]
}

/// The delimiter set at `0xa11544`.
const DELIMITERS: &[u8] = b"()[]^*/%+-#,> ";

#[derive(Debug, Clone, PartialEq)]
enum Op {
    /// Op 0: a literal.
    Number(f32),
    /// Op 1: the next token reads this spell.
    Spell(u32),
    /// Op 2: a description variable's value, read at parse time (`0x577530`).
    Variable(f32),
    /// Ops 3 to 9.
    Pow,
    Neg,
    Mul,
    Div,
    Mod,
    Add,
    Sub,
    /// Ops 10 to 21, by [`FUNCTIONS`] index.
    Function(u8),
    /// Ops 22 to 161, by [`TOKENS`] op.
    Token(u8),
}

struct Parser<'a, O: Operand + ?Sized> {
    text: &'a [u8],
    ops: Vec<Op>,
    /// `*param_1 = 1`: any error fails the whole expression.
    failed: bool,
    operand: &'a O,
}

impl<O: Operand + ?Sized> Parser<'_, O> {
    fn at(&self, pos: usize) -> u8 {
        self.text.get(pos).copied().unwrap_or(0)
    }

    /// `0x576bb0`: the word at `pos` and the position past it; `None` at the end.
    fn lex(&self, mut pos: usize) -> (Option<&[u8]>, usize) {
        while self.at(pos) == b' ' {
            pos += 1;
        }
        if self.at(pos) == 0 {
            return (None, pos);
        }
        if DELIMITERS.contains(&self.at(pos)) {
            return (Some(&self.text[pos..pos + 1]), pos + 1);
        }
        let start = pos;
        while self.at(pos) != 0 && !DELIMITERS.contains(&self.at(pos)) {
            pos += 1;
        }
        (Some(&self.text[start..pos]), pos)
    }

    /// `0x576c80`: the next word must be `want`, ignoring case.
    fn expect(&mut self, want: &[u8], pos: usize) -> usize {
        let (word, next) = self.lex(pos);
        if !word.is_some_and(|w| w.eq_ignore_ascii_case(want)) {
            self.failed = true;
        }
        next
    }

    /// `0x5778b0`.
    fn expr(&mut self, pos: usize) -> usize {
        let mut pos = self.term(pos);
        while matches!(self.at(pos), b'+' | b'-') {
            let op = if self.at(pos) == b'+' {
                Op::Add
            } else {
                Op::Sub
            };
            pos = self.term(pos + 1);
            self.ops.push(op);
        }
        pos
    }

    /// `0x5777f0`: the right operand is a primary, not a power, then its own `^` chain.
    fn term(&mut self, pos: usize) -> usize {
        let mut pos = self.power(pos);
        while matches!(self.at(pos), b'*' | b'/' | b'%') {
            let op = match self.at(pos) {
                b'*' => Op::Mul,
                b'/' => Op::Div,
                _ => Op::Mod,
            };
            pos = self.primary(pos + 1);
            while self.at(pos) == b'^' {
                pos = self.primary(pos + 1);
                self.ops.push(Op::Pow);
            }
            self.ops.push(op);
        }
        pos
    }

    /// `0x5777a0`.
    fn power(&mut self, pos: usize) -> usize {
        let mut pos = self.primary(pos);
        while self.at(pos) == b'^' {
            pos = self.primary(pos + 1);
            self.ops.push(Op::Pow);
        }
        pos
    }

    /// `0x577530`.
    fn primary(&mut self, pos: usize) -> usize {
        let (Some(word), next) = self.lex(pos) else {
            // No word: nothing is pushed, which the evaluator reads as a short stack.
            self.failed = true;
            return pos;
        };
        let word = word.to_vec();
        if word == b"(" {
            let pos = self.expr(next);
            return self.expect(b")", pos);
        }
        if word == b"-" {
            let pos = self.primary(next);
            self.ops.push(Op::Neg);
            return pos;
        }
        if word[0] == b'$' {
            let name = &word[1..];
            if name.first() == Some(&b'<') {
                let var = String::from_utf8_lossy(&name[1..]);
                self.ops.push(Op::Variable(self.operand.variable(&var)));
                return self.expect(b">", next);
            }
            if let Some(f) = FUNCTIONS
                .iter()
                .position(|(n, _)| n.as_bytes().eq_ignore_ascii_case(name))
            {
                let mut pos = self.expect(b"(", next);
                pos = self.expr(pos);
                for _ in 1..FUNCTIONS[f].1 {
                    pos = self.expect(b",", pos);
                    pos = self.expr(pos);
                }
                self.ops.push(Op::Function(f as u8));
                return self.expect(b")", pos);
            }
            let digits = name.iter().take_while(|b| b.is_ascii_digit()).count();
            if digits > 0 {
                let id = atoi(&name[..digits]);
                self.ops.push(Op::Spell(id));
            }
            let token = &name[digits..];
            if let Some(t) = TOKENS.iter().position(|n| n.as_bytes() == token) {
                self.ops.push(Op::Token(t as u8 + 22));
                return next;
            }
            self.failed = true;
            return next;
        }
        if !word[0].is_ascii_digit() && word[0] != b'.' {
            self.failed = true;
            return next;
        }
        self.ops.push(Op::Number(atof(&word)));
        next
    }
}

/// The CRT's `atof` over a word: the longest decimal prefix, exponent included; 0 for none.
fn atof(word: &[u8]) -> f32 {
    let text = String::from_utf8_lossy(word);
    (1..=text.len())
        .filter(|&i| text.is_char_boundary(i))
        .filter_map(|i| text[..i].parse::<f64>().ok())
        .next_back()
        .unwrap_or(0.0) as f32
}

/// Decimal digits to an id, wrapping as the CRT's `atoi` does.
fn atoi(digits: &[u8]) -> u32 {
    digits.iter().fold(0u32, |n, b| {
        n.wrapping_mul(10).wrapping_add(u32::from(b - b'0'))
    })
}

/// `0x577ad0`'s and `0x577900`'s arithmetic, each result stored as a single.
fn apply(op: &Op, stack: &mut Vec<f32>) -> Option<()> {
    let one = |v: bool| if v { 1.0 } else { 0.0 };
    let value = match op {
        Op::Neg => -stack.pop()?,
        Op::Pow | Op::Mul | Op::Div | Op::Mod | Op::Add | Op::Sub => {
            let right = f64::from(stack.pop()?);
            let left = f64::from(stack.pop()?);
            (match op {
                Op::Pow => left.powf(right),
                Op::Mul => left * right,
                Op::Div => left / right,
                // The reference's remainder (`0x577ad0`): `l - frac(l / r) * r`, the quotient
                // rounded to single first.
                Op::Mod => {
                    let q = f64::from((left / right) as f32);
                    left - (q - q.floor()) * right
                }
                Op::Add => left + right,
                _ => left - right,
            }) as f32
        }
        Op::Function(f) => {
            let n = FUNCTIONS[usize::from(*f)].1;
            if stack.len() < n {
                return None;
            }
            let a = stack.split_off(stack.len() - n);
            match f {
                0 => a[0].abs(),
                1 => a[0].ceil(),
                2 => a[0].floor(),
                3 => a[0].min(a[1]),
                4 => a[0].max(a[1]),
                5 => one(a[0] > a[1]),
                6 => one(a[0] < a[1]),
                7 => one(a[0] >= a[1]),
                8 => one(a[0] <= a[1]),
                9 => one((a[0] - a[1]).abs() < 1.0e-4),
                10 => {
                    if a[0] == 0.0 {
                        a[2]
                    } else {
                        a[1]
                    }
                }
                // `0x577900`: the larger of the first two, unless the third is not above it.
                _ => {
                    let high = a[0].max(a[1]);
                    if high < a[2] {
                        high
                    } else {
                        a[2]
                    }
                }
            }
        }
        _ => return None,
    };
    stack.push(value);
    Some(())
}

/// The value of one expression body, `None` when it fails to parse or evaluate.
pub(super) fn evaluate(body: &str, operand: &(impl Operand + ?Sized)) -> Option<f32> {
    let mut parser = Parser {
        text: body.as_bytes(),
        ops: Vec::new(),
        failed: false,
        operand,
    };
    parser.expr(0);
    if parser.failed {
        return None;
    }
    let mut stack: Vec<f32> = Vec::with_capacity(32);
    let mut spell = None;
    for op in &parser.ops {
        match op {
            Op::Number(v) | Op::Variable(v) => stack.push(*v),
            Op::Spell(id) => spell = Some(*id),
            Op::Token(t) => {
                stack.push(operand.token(*t, spell.take())?);
            }
            op => apply(op, &mut stack)?,
        }
        if stack.len() > 32 {
            return None;
        }
    }
    stack.pop()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed;
    impl Operand for Fixed {
        fn token(&self, index: u8, spell: Option<u32>) -> Option<f32> {
            match (token_name(index), spell) {
                ("m1", None) => Some(40.0),
                ("m1", Some(7)) => Some(3.0),
                ("pl", None) => Some(70.0),
                (_, Some(_)) => None,
                _ => Some(0.0),
            }
        }
        fn variable(&self, name: &str) -> f32 {
            if name == "mult" {
                2.5
            } else {
                0.0
            }
        }
    }

    fn eval(body: &str) -> Option<f32> {
        evaluate(body, &Fixed)
    }

    #[test]
    fn arithmetic_follows_the_references_precedence() {
        assert_eq!(eval("42/5*18"), Some(151.2));
        assert_eq!(eval("($m1*3)+17"), Some(137.0));
        assert_eq!(eval("2+3*4"), Some(14.0));
        assert_eq!(eval("2*3^2"), Some(18.0));
        assert_eq!(eval("-$m1+1"), Some(-39.0));
        assert_eq!(eval("90000/-1000"), Some(-90.0));
    }

    #[test]
    fn an_operator_after_a_space_ends_the_expression() {
        assert_eq!(eval("42 / 5"), Some(42.0));
        assert_eq!(eval(" 42/5"), Some(8.4));
    }

    #[test]
    fn the_remainder_is_the_references_own() {
        // `7 - frac(7 / 2) * 2` = 6, not 1.
        assert_eq!(eval("7%2"), Some(6.0));
    }

    #[test]
    fn functions_match_ignoring_case_and_tokens_do_not() {
        assert_eq!(eval("$COND($GT($pl,15),$pl-15,0)"), Some(55.0));
        assert_eq!(eval("$max(1,$min(5,3))"), Some(3.0));
        assert_eq!(eval("$clamp(9,2,5)"), Some(5.0));
        assert_eq!(eval("$floor(2.7)+$ceil(0.2)"), Some(3.0));
        assert_eq!(eval("$eq(1,1.00001)"), Some(1.0));
        assert_eq!(eval("$PL"), Some(0.0));
        assert_eq!(eval("$Pl"), None);
    }

    #[test]
    fn variables_and_cross_spell_tokens() {
        assert_eq!(eval("$<mult>*2"), Some(5.0));
        assert_eq!(eval("$<unset>+1"), Some(1.0));
        assert_eq!(eval("$7m1+$m1"), Some(43.0));
        assert_eq!(eval("$9m1"), None, "a spell without a record fails");
    }

    #[test]
    fn malformed_bodies_fail() {
        assert_eq!(eval(""), None);
        assert_eq!(eval("(1+2"), None);
        assert_eq!(eval("$nosuch"), None);
        assert_eq!(eval("$max(1)"), None);
    }
}
