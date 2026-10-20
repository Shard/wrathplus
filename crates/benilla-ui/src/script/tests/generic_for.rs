//! Build 12340's Lua 5.1 generic-for and constructor grammar: no 5.0 table generator, no 5.0
//! compat-semicolon.

use super::common::script;

/// A generator takes the normal call path, so a table or a number raises on the first iteration.
#[test]
fn a_generator_is_called_and_a_table_or_number_raises() {
    let s = script();

    let via_pairs: i64 = s
        .eval(
            r#"
            local t = { a = 1, b = 2, c = 3 }
            local n = 0
            for k, v in pairs(t) do n = n + 1 end
            return n
        "#,
        )
        .unwrap();
    assert_eq!(via_pairs, 3);

    let custom: i64 = s
        .eval(
            r#"
            local function upto(state, i)
                i = i + 1
                if i <= state then return i end
            end
            local sum = 0
            for i in upto, 3, 0 do sum = sum + i end
            return sum
        "#,
        )
        .unwrap();
    assert_eq!(
        custom, 6,
        "an explicit generator/state/control triple is untouched"
    );

    let err = s
        .eval::<i64>("local n = 0 for k, v in 42 do n = n + 1 end return n")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("attempt to call a number value"),
        "a non-function generator must raise: {err}"
    );

    let err = s
        .eval::<i64>("local n = 0 for k, v in { 1, 2 } do n = n + 1 end return n")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("attempt to call a table value"),
        "5.1 has no 5.0 table generator: {err}"
    );
}

/// Repeated separators in a constructor and statement-level `;;` are syntax errors in 5.1.
#[test]
fn repeated_semicolons_are_rejected() {
    let s = script();

    let err = s
        .eval::<i64>("local t = { 1;;; 2 } return t[1]")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("unexpected symbol"),
        "a repeated constructor separator is a syntax error: {err}"
    );

    let err = s
        .eval::<i64>("local x = 1;; return x")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("unexpected symbol"),
        "statement-level `;;` is rejected: {err}"
    );
}
