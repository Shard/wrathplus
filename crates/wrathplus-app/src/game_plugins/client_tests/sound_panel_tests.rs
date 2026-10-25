//! The stock 3.3.5 Sound panel drives the mixer: the `Sound_*` CVars its controls write
//! (`AudioOptionsPanels.lua`/`.xml`) are host rows, and a write reaches [`SoundConfig`].

use std::collections::BTreeSet;

use super::cvars::{apply, cvar_app, res};
use crate::cvars::SetOutcome;
use crate::sound::SoundConfig;

/// A mixer knob's answer to the value written.
type Answer = fn(&SoundConfig) -> bool;

/// Each mixer knob the panel drives: its CVar, a value off its default, and the knob's answer.
const KNOBS: &[(&str, &str, Answer)] = &[
    ("Sound_EnableAllSound", "0", |s| !s.enabled),
    ("Sound_EnableSFX", "0", |s| !s.sfx_enabled),
    ("Sound_EnableMusic", "0", |s| !s.music_enabled),
    ("Sound_EnableAmbience", "0", |s| !s.ambience_enabled),
    ("Sound_EnableErrorSpeech", "0", |s| !s.error_speech),
    ("Sound_EnableEmoteSounds", "0", |s| !s.emote_sounds),
    ("Sound_EnableSoundWhenGameIsInBG", "1", |s| {
        s.background_sound
    }),
    ("Sound_EnableReverb", "1", |s| s.reverb),
    ("Sound_ListenerAtCharacter", "0", |s| {
        !s.listener_at_character
    }),
    ("Sound_ZoneMusicNoDelay", "1", |s| s.zone_music_no_delay),
    ("Sound_MasterVolume", "0.25", |s| s.master == 0.25),
    ("Sound_SFXVolume", "0.5", |s| s.sfx == 0.5),
    ("Sound_MusicVolume", "0.75", |s| s.music == 0.75),
    ("Sound_AmbienceVolume", "0.1", |s| s.ambience == 0.1),
];

/// Before, the host held 1.12's `MasterVolume`, `SoundVolume`, … which the 3.3.5 panel never
/// writes: its sliders and boxes changed only the shim's Lua-side copies.
#[test]
fn the_stock_sound_panels_cvars_drive_the_mixer() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::open_chain(&data).expect("the install's archives");
    let mut named = BTreeSet::new();
    for file in [
        "Interface\\FrameXML\\AudioOptionsPanels.lua",
        "Interface\\FrameXML\\AudioOptionsPanels.xml",
    ] {
        let text = String::from_utf8_lossy(&chain.read_file(file).expect(file)).into_owned();
        for (at, _) in text.match_indices("Sound_") {
            let name: String = text[at..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            named.insert(name);
        }
    }
    let mut app = cvar_app();
    for (name, value, answer) in KNOBS {
        assert!(
            named.contains(*name),
            "the stock panel writes {name}: {named:?}"
        );
        assert_eq!(apply(&mut app, name, value), SetOutcome::Changed, "{name}");
        assert!(
            answer(res::<SoundConfig>(&app)),
            "{name} = {value} reaches the mixer"
        );
    }
}
