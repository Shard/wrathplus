//! Shared fixtures for the script test modules.

use crate::script::UiScript;

pub(super) fn script() -> UiScript {
    UiScript::new().expect("construct UiScript")
}

/// A 1.12 host, for the tests that still pin 1.12 behaviour until Vanilla removal step 9.
pub(super) fn vanilla_script() -> UiScript {
    UiScript::new_vanilla().expect("construct UiScript")
}
