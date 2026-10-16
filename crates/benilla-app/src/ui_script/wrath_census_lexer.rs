//! Lexical masking for the headless FrameXML census, not a Lua evaluator.
//! Keep byte offsets/newlines so diagnostics can still point into installed sources.

/// Remove Lua comments and strings (including arbitrary-level long brackets), and XML comments.
/// Executable XML handler bodies remain visible; quoted XML attributes cannot create calls.
/// Entity-escaped quotes in XML handlers remain visible (possible false positives, never exclusions).
pub(super) fn executable_text(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        if b[i..].starts_with(b"<!--") {
            i += 4;
            while i < b.len() && !b[i..].starts_with(b"-->") {
                i += 1;
            }
            i = (i + 3).min(b.len());
        } else if b[i..].starts_with(b"--") {
            i += 2;
            if let Some(end) = long_end(b, i) {
                i = end;
            } else {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
        } else if b[i] == b'\'' || b[i] == b'"' {
            let quote = b[i];
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    i = (i + 2).min(b.len());
                } else if b[i] == b'\n' || b[i] == b'\r' {
                    // Invalid short Lua strings cannot swallow later executable lines.
                    break;
                } else if b[i] == quote {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if let Some(end) = long_end(b, i) {
            i = end;
        } else {
            i += 1;
            continue;
        }
        for c in &mut out[start..i] {
            if *c != b'\n' && *c != b'\r' {
                *c = b' ';
            }
        }
    }
    String::from_utf8(out).expect("masked UTF-8")
}

fn long_end(b: &[u8], start: usize) -> Option<usize> {
    if b.get(start) != Some(&b'[') {
        return None;
    }
    let mut p = start + 1;
    while b.get(p) == Some(&b'=') {
        p += 1;
    }
    if b.get(p) != Some(&b'[') {
        return None;
    }
    let level = p - start - 1;
    p += 1;
    while p < b.len() {
        if b[p] == b']'
            && b.get(p + 1..p + 1 + level)
                .is_some_and(|s| s.iter().all(|c| *c == b'='))
            && b.get(p + 1 + level) == Some(&b']')
        {
            return Some(p + level + 2);
        }
        p += 1;
    }
    Some(b.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn broken_short_strings_do_not_hide_next_line_calls() {
        for src in ["x = 'oops\nRealCall()", "x = \"oops\r\nRealCall()"] {
            let text = executable_text(src);
            assert!(text.contains("RealCall()"));
            assert_eq!(text.len(), src.len());
        }
        assert!(!executable_text("'escaped\\\nHidden()'").contains("Hidden()"));
    }
    #[test]
    fn utf8_cdata_and_post_long_comment_calls_survive() {
        let src = "-- café 隠す()\nlocal s = 'é 隠す()'\n--[==[ hidden() ]==] RealCall()\n<OnLoad><![CDATA[HandlerCall()]]></OnLoad>";
        let text = executable_text(src);
        assert_eq!(text.len(), src.len());
        assert!(text.contains("RealCall()"));
        assert!(text.contains("HandlerCall()"));
        assert!(!text.contains("隠す"));
    }
    #[test]
    fn masks_prose_not_real_calls_and_preserves_offsets() {
        let src = "-- FakeCall()\nRealCall('FakeString()') --[=[ HiddenCall() ]=]\nlocal s = [==[ AlsoHidden() ]==]\nNextCall()";
        let text = executable_text(src);
        assert_eq!(text.len(), src.len());
        assert_eq!(text.lines().count(), src.lines().count());
        for name in ["FakeCall", "FakeString", "HiddenCall", "AlsoHidden"] {
            assert!(!text.contains(name));
        }
        assert!(text.contains("RealCall("));
        assert!(text.contains("NextCall()"));
    }
    #[test]
    fn xml_handlers_survive_but_attributes_and_comments_do_not() {
        let src = r#"<Frame name="FakeCall()"><!-- HiddenCall() --><Scripts><OnLoad>RealCall(self, "Name()"); self:RealMethod();</OnLoad></Scripts></Frame>"#;
        let text = executable_text(src);
        assert!(text.contains("RealCall("));
        assert!(text.contains(":RealMethod("));
        assert!(!text.contains("FakeCall"));
        assert!(!text.contains("HiddenCall"));
        assert!(!text.contains("Name()"));
    }
    #[test]
    fn escapes_long_levels_and_unterminated_comments() {
        let text = executable_text("'can\\'t Fake()' Real() --[==[ Hidden() ]=] StillHidden()");
        assert!(text.contains("Real()"));
        assert!(!text.contains("Fake"));
        assert!(!text.contains("Hidden"));
    }
}
