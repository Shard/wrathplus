//! The entity display model's skin resolution and part assembly.

use super::*;

/// Both kinds carry the same empty `parts` once built, so only the handle tells a display that
/// named no model from one whose model draws nothing.
#[test]
fn only_a_display_with_no_model_file_earns_the_cube() {
    assert!(
        !empty_display().names_a_model(),
        "a display that resolved to no model must stay cube-eligible"
    );
    assert!(
        !DisplayModel {
            handle: ModelHandle::None,
            parts: Some(Vec::new()),
            ..empty_shell()
        }
        .names_a_model(),
        "the handle decides, not the parts"
    );
    assert!(
        DisplayModel {
            handle: ModelHandle::M2(Handle::default()),
            // Built with nothing to draw, like `InvisibleStalker.m2`'s zero batches.
            parts: Some(Vec::new()),
            ..empty_shell()
        }
        .names_a_model(),
        "a display that named an M2 is answered by that model, empty parts included"
    );
    assert!(
        DisplayModel {
            handle: ModelHandle::Wmo(Handle::default()),
            ..empty_shell()
        }
        .names_a_model(),
        "a WMO display names a model too"
    );
}
