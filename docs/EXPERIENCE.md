# Client experience

Experience is independent of `WOW_CLIENT_PROFILE` (installed data) and the session protocol.
Without configuration, **Classic** is selected: every Plus feature is disabled, even if a feature
entry says `true`. This is the evaluation default. `WOW_EXPERIENCE=classic|plus` overrides only
the profile; invalid values stop boot rather than silently selecting a different experience.

The existing `BENILLA_HOME/config.toml` supports:

```toml
[experience]
profile = "plus"
packs = ["base-art", "my-art"] # ascending priority, later packs win
allow_classic_overrides = false

[experience.features]
asset_overrides = true
shadows = true
fog = true
bloom = true
tone_mapping = true
colour_grading = true
```

All registered features default on in Plus; each can be disabled separately. These are seams:
a registered feature only has an effect once its consumer is implemented. Unknown feature names
are rejected rather than silently misspelling a toggle. CVar saves preserve the experience table.
Capture runs ignore persistent player configuration, as they do for CVars; an explicit
`WOW_EXPERIENCE=plus` is needed to select Plus for a capture.

`experience::Experience` is the app's Bevy resource and the only feature-query API. Runtime callers
use `set_profile` and `set_feature`; switching back to Classic masks existing feature overrides.
Runtime changes are session-only and do not overwrite the boot configuration. Asset packs are
mounted at boot, not hot-reloaded. Classic asset overrides require a separate explicit
`allow_classic_overrides = true`; that permission never enables graphics features under Classic.
