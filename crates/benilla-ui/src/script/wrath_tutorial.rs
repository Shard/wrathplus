//! Wrath tutorial history (`0x530100`, `0x530300`, `0x5303a0`).
//!
//! History is ordered independently of the completion bits. The native CVar codec stores a
//! `v\x01` header followed by three base-93 digits per entry, not a comma-separated ID list.

const HISTORY_LEN: usize = 60;
const RADIX: u32 = 93;
const DIGIT_BASE: u8 = 35;

/// The safe counterpart of the native sixty-dword history. The native parser can overrun its
/// array on malformed input; this representation deliberately bounds storage to sixty entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct History {
    entries: [u32; HISTORY_LEN],
}

impl Default for History {
    fn default() -> Self {
        Self {
            entries: [0; HISTORY_LEN],
        }
    }
}

impl History {
    /// `0x766f00`: version-zero strings use unsigned base-220 triples, converted through the
    /// modern encoder. Unknown nonzero versions retain the native modern-decoding fallback.
    fn decode(bytes: &[u8]) -> Self {
        if bytes.first() == Some(&b'v') && bytes.get(1).is_some_and(|&v| v != 0) {
            return Self::decode_modern(&bytes[2..]);
        }
        let mut converted = Vec::new();
        for triple in bytes
            .split(|&byte| byte == 0)
            .next()
            .unwrap_or_default()
            .as_chunks::<3>().0
        {
            let value = triple.iter().fold(0u32, |value, &digit| {
                value
                    .wrapping_mul(220)
                    .wrapping_add(u32::from(digit).wrapping_sub(35))
            });
            if value < RADIX.pow(3) {
                converted.extend([
                    (value / RADIX.pow(2)) as u8 + DIGIT_BASE,
                    ((value / RADIX) % RADIX) as u8 + DIGIT_BASE,
                    (value % RADIX) as u8 + DIGIT_BASE,
                ]);
            }
        }
        Self::decode_modern(&converted)
    }

    /// Modern-format load, stopping at zero or an incomplete triple (`0x766f70`).
    fn decode_modern(bytes: &[u8]) -> Self {
        let mut history = Self::default();
        for (slot, digits) in history.entries.iter_mut().zip(bytes.as_chunks::<3>().0) {
            if digits.contains(&0) {
                break;
            }
            // The native reader does not validate the digit range. Wrapping arithmetic models
            // its dword expression without introducing a Rust debug-overflow panic.
            let value = digits.iter().fold(0u32, |value, &digit| {
                value
                    .wrapping_mul(RADIX)
                    .wrapping_add((i32::from(digit as i8) - i32::from(DIGIT_BASE)) as u32)
            });
            if value == 0 {
                break;
            }
            *slot = value;
        }
        history
    }

    /// Teardown serialization (`0x530300`), not a write performed on every append.
    fn encode(&self) -> Vec<u8> {
        let mut bytes = vec![b'v', 1];
        for &value in self.entries.iter().take_while(|&&value| value != 0) {
            // `0x766720` silently skips unsigned values outside its three-digit range.
            if value >= RADIX.pow(3) {
                continue;
            }
            bytes.extend([
                (value / RADIX.pow(2)) as u8 + DIGIT_BASE,
                ((value / RADIX) % RADIX) as u8 + DIGIT_BASE,
                (value % RADIX) as u8 + DIGIT_BASE,
            ]);
        }
        bytes
    }

    /// `0x530100`: append at the first zero, unless a duplicate is encountered first.
    fn append(&mut self, id: u32) {
        for entry in &mut self.entries {
            if *entry == id {
                return;
            }
            if *entry == 0 {
                *entry = id;
                return;
            }
        }
    }

    /// `0x530140`: exclusive forward traversal; 60 is a sentinel even as an actual entry.
    fn next(&self, id: u32) -> Option<u32> {
        let mut found = false;
        for &entry in &self.entries {
            if found && entry != 0 {
                return (entry != 60).then_some(entry);
            }
            if entry == id {
                found = true;
            }
        }
        None
    }

    /// `0x530190` starts one slot past history, at the seen-bank's bit count. Model that
    /// adjacency explicitly, rather than performing an out-of-bounds array read.
    fn previous(&self, id: u32, seen_bit_count: u32) -> Option<u32> {
        let mut found = false;
        for entry in std::iter::once(seen_bit_count).chain(self.entries.iter().rev().copied()) {
            if found && entry != 0 {
                return Some(entry);
            }
            if entry == id {
                found = true;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm() -> super::super::UiScript {
        let mut script =
            super::super::UiScript::new_with_dialect(super::super::ScriptDialect::Wrath12340)
                .unwrap();
        script.seed_cvars([super::super::SeededCvar {
            name: "flaggedTutorials".into(),
            value: "v\x01".into(),
            default: "".into(),
            latched: false,
        }]);
        script.set_wrath_tutorial_banks(Some(vec![0; 32]), Some(vec![0; 32]));
        script
    }

    #[test]
    fn trigger_tutorial_marks_an_unseen_tutorial_and_fires_only_when_shown() {
        let script = vm();
        script
            .run(
                "SEEN = {} local f = CreateFrame('Frame') f:RegisterEvent('TUTORIAL_TRIGGER') \
                 f:SetScript('OnEvent', function(self, event, n) table.insert(SEEN, n) end) \
                 SetCVar('showTutorials', '1') TriggerTutorial(4) TriggerTutorial(4) \
                 SetCVar('showTutorials', '0') TriggerTutorial(5) TriggerTutorial(61)",
            )
            .unwrap();
        assert_eq!(
            script
                .eval::<String>("return table.concat(SEEN, ',')")
                .unwrap(),
            "4",
            "once, and not while tutorials are hidden"
        );
        assert!(script.run("TriggerTutorial()").is_err());
    }

    #[test]
    fn synchronous_mutations_order_and_teardown_roundtrip() {
        let mut script = vm();
        script.run("FlagTutorial(42); assert(IsTutorialFlagged(42)==1); FlagTutorial(3); assert(GetNextCompleatedTutorial(42)==3); assert(GetPrevCompleatedTutorial(256)==3)").unwrap();
        assert_eq!(script.cvar("flaggedTutorials").as_deref(), Some("v\x01"));
        assert_eq!(
            script.take_wrath_tutorial_requests(),
            vec![
                WrathTutorialRequest::Flag(41),
                WrathTutorialRequest::Flag(2)
            ]
        );
        let saved = script.persist_wrath_tutorial_history().unwrap();
        let fresh = vm();
        // A fresh VM loads the actual saved row, not the helper's initial empty history.
        fresh.model_mut().wrath_tutorial.initialized = false;
        fresh.seed_cvars([super::super::SeededCvar {
            name: "flaggedTutorials".into(),
            value: saved.clone(),
            default: "".into(),
            latched: false,
        }]);
        fresh
            .run("assert(GetNextCompleatedTutorial(42)==3)")
            .unwrap();
        script.run("ClearTutorials(); assert(IsTutorialFlagged(60)==1); ResetTutorials(); assert(IsTutorialFlagged(42)==nil); FlagTutorial(17)").unwrap();
        assert_eq!(
            script.take_wrath_tutorial_requests(),
            vec![
                WrathTutorialRequest::Clear,
                WrathTutorialRequest::Reset,
                WrathTutorialRequest::Flag(16)
            ]
        );
        assert_eq!(script.cvar("flaggedTutorials"), Some(saved));
    }

    #[test]
    fn clear_present_bank_preserves_order_absent_bank_fills_and_reset_zeros() {
        let mut script = vm();
        script.run("FlagTutorial(42); FlagTutorial(3); ClearTutorials(); assert(GetNextCompleatedTutorial(42)==3); assert(GetPrevCompleatedTutorial(256)==3); ResetTutorials(); assert(select('#',GetPrevCompleatedTutorial(256))==0)").unwrap();
        script.set_wrath_tutorial_banks(None, None);
        script.run("ClearTutorials(); assert(GetNextCompleatedTutorial(1)==2); assert(GetPrevCompleatedTutorial(60)==59); assert(select('#',GetNextCompleatedTutorial(59))==0); ResetTutorials(); assert(select('#',GetNextCompleatedTutorial(1))==0)").unwrap();
    }

    #[test]
    fn query_arity_arguments_resend_and_reseed() {
        let mut script = vm();
        script.run("assert(select('#',IsTutorialFlagged(0))==0); assert(select('#',IsTutorialFlagged(1))==1); assert(IsTutorialFlagged('1.9')==nil); assert(not pcall(IsTutorialFlagged,{})); FlagTutorial(1); assert(IsTutorialFlagged(1)==1)").unwrap();
        script.set_wrath_tutorial_banks(Some(vec![0; 32]), Some(vec![0; 32]));
        script.run("assert(IsTutorialFlagged(1)==nil)").unwrap();
        script.seed_cvars([super::super::SeededCvar {
            name: "unrelated".into(),
            value: "0".into(),
            default: "0".into(),
            latched: false,
        }]);
        assert_eq!(script.model_mut().wrath_tutorial.history.entries[0], 1);
    }

    #[test]
    fn modern_codec_preserves_order_duplicates_and_stops_at_zero_or_partial_value() {
        let mut history = History::default();
        history.entries[..5].copy_from_slice(&[42, 3, 42, 60, 256]);
        let encoded = history.encode();
        assert_eq!(&encoded[..2], b"v\x01");
        assert_eq!(History::decode_modern(&encoded[2..]), history);
        let mut terminated = encoded[2..].to_vec();
        terminated.extend_from_slice(b"###$$$");
        assert_eq!(History::decode_modern(&terminated), history);
        assert_eq!(History::decode_modern(b"##"), History::default());
        assert_eq!(History::decode_modern(b"#\0$"), History::default());
    }

    #[test]
    fn legacy_unknown_versions_and_signed_modern_bytes_follow_the_reader() {
        assert_eq!(History::decode(b""), History::default());
        assert_eq!(History::decode(b"##M").entries[0], 42);
        assert_eq!(History::decode(b"v\x02##M").entries[0], 42);
        assert_eq!(History::decode(b"v\x01##M").entries[0], 42);
        assert_eq!(History::decode(b"v\x01##\x80").entries[0], (-163i32) as u32);
        assert_eq!(History::decode(b"\xff\xff\xff##$").entries[0], 1);
    }

    #[test]
    fn malformed_long_input_is_bounded_instead_of_emulating_native_memory_corruption() {
        let history = History::decode_modern(&b"##$".repeat(100));
        assert_eq!(history.entries, [1; HISTORY_LEN]);
        assert_eq!(history.encode().len(), 2 + HISTORY_LEN * 3);
    }

    #[test]
    fn append_and_navigation_follow_history_not_numeric_order() {
        let mut history = History::default();
        for id in [42, 3, 17, 3, 60] {
            history.append(id);
        }
        assert_eq!(&history.entries[..5], &[42, 3, 17, 60, 0]);
        assert_eq!(history.next(42), Some(3));
        assert_eq!(history.previous(17, 256), Some(3));
        assert_eq!(
            history.next(17),
            None,
            "actual ID 60 is the native sentinel"
        );
        assert_eq!(history.previous(256, 256), Some(60));
        assert_eq!(history.previous(42, 256), None);
        assert_eq!(history.next(99), None);
        assert_eq!(
            history.previous(0, 256),
            Some(60),
            "zero matches empty slots"
        );
    }
}

/// Ordered Wrath tutorial commands; the app preserves this order on the existing wire path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrathTutorialRequest {
    Flag(u32),
    Clear,
    Reset,
}

#[derive(Default)]
pub(crate) struct State {
    history: History,
    initialized: bool,
    seen: Option<Vec<u8>>,
    requests: Vec<WrathTutorialRequest>,
}

impl State {
    /// Whether any tutorial-seen bit is set: `CanResetTutorials`' test (`0x530700` scans the
    /// flag words `[0xbd1aec]` for a nonzero one).
    pub(super) fn any_seen(&self) -> bool {
        self.seen.as_ref().is_some_and(|s| s.iter().any(|&b| b != 0))
    }
}

pub(super) fn initialize_from_seed(model: &mut super::Model) {
    if model.dialect != super::ScriptDialect::Wrath12340 || model.wrath_tutorial.initialized {
        return;
    }
    let Some(row) = model.cvars.get("flaggedtutorials") else {
        return;
    };
    model.wrath_tutorial.history = History::decode(row.value.as_bytes());
    model.wrath_tutorial.initialized = true;
}

impl super::UiScript {
    /// Serialize only at the shutdown boundary, after the final Lua events. Returning the value
    /// lets the host fold this one setting without enabling generic Wrath CVar persistence.
    pub fn persist_wrath_tutorial_history(&mut self) -> Option<String> {
        let mut model = self.model_mut();
        if model.dialect != super::ScriptDialect::Wrath12340 || !model.wrath_tutorial.initialized {
            return None;
        }
        let bytes = model.wrath_tutorial.history.encode();
        // Encoder digits are ASCII, including DEL, so this is always valid UTF-8.
        let value = String::from_utf8(bytes).expect("tutorial encoding is ASCII");
        if let Some(slot) = model.cvars.get_mut("flaggedtutorials") {
            slot.value.clone_from(&value);
        }
        Some(value)
    }

    /// App-originated acknowledgements share native ordered history without queueing a second send.
    pub fn append_wrath_tutorial_history(&mut self, id: u32) {
        self.model_mut()
            .wrath_tutorial
            .history
            .append(id.wrapping_add(1));
    }

    pub fn take_wrath_tutorial_requests(&mut self) -> Vec<WrathTutorialRequest> {
        std::mem::take(&mut self.model_mut().wrath_tutorial.requests)
    }

    /// Authoritative packet/login baseline, distinct from the app's ordinary local-state echo.
    pub fn set_wrath_tutorial_banks(&mut self, seen: Option<Vec<u8>>, completed: Option<Vec<u8>>) {
        let mut model = self.model_mut();
        model.wrath_tutorial.seen = seen;
        model.tutorial_bank = completed;
    }
}

fn bit(bank: &Option<Vec<u8>>, id: u32) -> Option<bool> {
    bank.as_ref()?
        .get((id / 8) as usize)
        .map(|b| b & (1 << (id % 8)) != 0)
}

pub(super) fn install(lua: &mlua::Lua) -> mlua::Result<()> {
    use super::binding_abi::{flag, number_arg};
    use mlua::{MultiValue, Value};
    let globals = lua.globals();
    globals.set(
        "FlagTutorial",
        lua.create_function(|lua, value: Value| {
            let id = number_arg(lua, value, "Usage: FlagTutorial(\"tutorial\")")?.wrapping_sub(1);
            if !(0..60).contains(&id) {
                return Ok(());
            }
            let mut model = lua.app_data_mut::<super::Model>().expect("model app_data");
            if bit(&model.tutorial_bank, id as u32) != Some(false) {
                return Ok(());
            }
            let mask = 1 << (id % 8);
            if let Some(byte) = model
                .tutorial_bank
                .as_mut()
                .and_then(|b| b.get_mut(id as usize / 8))
            {
                *byte |= mask;
            }
            if let Some(byte) = model
                .wrath_tutorial
                .seen
                .as_mut()
                .and_then(|b| b.get_mut(id as usize / 8))
            {
                *byte |= mask;
            }
            model.wrath_tutorial.history.append(id as u32 + 1);
            model
                .wrath_tutorial
                .requests
                .push(WrathTutorialRequest::Flag(id as u32));
            Ok(())
        })?,
    )?;
    for (name, reset) in [("ClearTutorials", false), ("ResetTutorials", true)] {
        globals.set(
            name,
            lua.create_function(move |lua, ()| {
                let mut model = lua.app_data_mut::<super::Model>().expect("model app_data");
                if let Some(bank) = &mut model.tutorial_bank {
                    bank.fill(if reset { 0 } else { 255 });
                }
                if let Some(bank) = &mut model.wrath_tutorial.seen {
                    bank.fill(if reset { 0 } else { 255 });
                }
                if reset {
                    model.wrath_tutorial.history = History::default();
                } else if model.tutorial_bank.as_ref().is_none_or(|b| b.is_empty()) {
                    for id in 1..=60 {
                        model.wrath_tutorial.history.append(id);
                    }
                }
                model.wrath_tutorial.requests.push(if reset {
                    WrathTutorialRequest::Reset
                } else {
                    WrathTutorialRequest::Clear
                });
                Ok(())
            })?,
        )?;
    }
    // TriggerTutorial(n) (`0x5308d0` → `0x530840`): once the seen bank has loaded, an unseen
    // tutorial 1-60 is marked seen and, with `showTutorials` on, fires `TUTORIAL_TRIGGER` (event
    // `0x175`) with its number. Labelled: the event name for `0x175`, the popup sound and the
    // `showTutorials`-off leg's `0x530450` are taken, not read.
    globals.set(
        "TriggerTutorial",
        lua.create_function(|lua, value: Value| {
            let id = number_arg(lua, value, "Usage: Trigger(\"tutorial\")")?.wrapping_sub(1);
            if !(0..60).contains(&id) {
                return Ok(());
            }
            let show = {
                let mut model = lua.app_data_mut::<super::Model>().expect("model app_data");
                if bit(&model.wrath_tutorial.seen, id as u32) != Some(false) {
                    return Ok(());
                }
                if let Some(byte) = model
                    .wrath_tutorial
                    .seen
                    .as_mut()
                    .and_then(|b| b.get_mut(id as usize / 8))
                {
                    *byte |= 1 << (id % 8);
                }
                model
                    .cvars
                    .get("showtutorials")
                    .is_some_and(|c| c.value.trim().parse::<f64>().is_ok_and(|n| n != 0.0))
            };
            if show {
                super::tick::fire_event_into(
                    lua,
                    "TUTORIAL_TRIGGER",
                    vec![super::ScriptValue::Int(i64::from(id) + 1)],
                );
            }
            Ok(())
        })?,
    )?;
    globals.set(
        "IsTutorialFlagged",
        lua.create_function(|lua, value: Value| {
            let id = number_arg(lua, value, "Usage: Trigger(\"tutorial\")")?.wrapping_sub(1);
            if !(0..60).contains(&id) {
                return Ok(MultiValue::new());
            }
            let model = lua.app_data_ref::<super::Model>().expect("model app_data");
            Ok(MultiValue::from_vec(vec![flag(
                bit(&model.tutorial_bank, id as u32) == Some(true),
            )]))
        })?,
    )?;
    for (name, previous) in [
        ("GetPrevCompleatedTutorial", true),
        ("GetNextCompleatedTutorial", false),
    ] {
        globals.set(
            name,
            lua.create_function(move |lua, value: Value| {
                let id = number_arg(
                    lua,
                    value,
                    if previous {
                        "Usage: GetPrevCompleatedTutorial(\"tutorial\")"
                    } else {
                        "Usage: GetNextCompleatedTutorial(\"tutorial\")"
                    },
                )? as u32;
                let model = lua.app_data_ref::<super::Model>().expect("model app_data");
                let history = &model.wrath_tutorial.history;
                let result = if previous {
                    history.previous(
                        id,
                        model
                            .wrath_tutorial
                            .seen
                            .as_ref()
                            .map_or(0, |b| (b.len() as u32).wrapping_mul(8)),
                    )
                } else {
                    history.next(id)
                };
                Ok(MultiValue::from_vec(
                    result
                        .map(|n| Value::Integer(i64::from(n)))
                        .into_iter()
                        .collect(),
                ))
            })?,
        )?;
    }
    Ok(())
}
