//! The reference's XML boolean, shared by every flag attribute.

/// The reference's XML boolean (`0x8154e0` → `0x815400`, default false): a leading `1`-`9`, `T`,
/// `t`, `Y` or `y` is true and `0`, `F`, `f`, `N` or `n` false; else `on`/`enabled` true and
/// `off`/`disabled` false, case-insensitively; anything else the default. So stock
/// `AccountLoginPasswordEdit`'s `password="1"` (`AccountLogin.xml:239`) is on.
pub(super) fn xml_bool(v: &str) -> bool {
    match v.as_bytes().first() {
        Some(b'1'..=b'9' | b'T' | b't' | b'Y' | b'y') => true,
        Some(b'0' | b'F' | b'f' | b'N' | b'n') | None => false,
        _ => ["on", "enabled"].iter().any(|w| v.eq_ignore_ascii_case(w)),
    }
}

/// [`xml_bool`] for a flag attribute, `None` for an empty value, which the reference skips.
pub(super) fn xml_flag(v: &str) -> Option<bool> {
    (!v.is_empty()).then(|| xml_bool(v))
}

#[cfg(test)]
#[test]
fn the_reference_table() {
    for v in [
        "true", "TRUE", "1", "2", "9", "yes", "Y", "t", "on", "ON", "enabled",
    ] {
        assert!(xml_bool(v), "{v}");
    }
    for v in [
        "false", "0", "no", "N", "f", "off", "disabled", "", "maybe", "-1",
    ] {
        assert!(!xml_bool(v), "{v}");
    }
}
