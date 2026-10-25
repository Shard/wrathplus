//! The sound CVars, build 12340's `Sound_*` names (registered at `0x4d1215`-`0x4d1953`), which the
//! stock 3.3.5 Sound panel writes (`AudioOptionsPanels.lua`).

use bevy::prelude::*;

use super::SoundConfig;

/// The sound CVars' change callback: volumes clamp to `[0, 1]`, enables are `!= 0`.
pub fn on_cvar(ev: On<crate::cvars::CvarChanged>, mut sound: ResMut<SoundConfig>) {
    let v = ev.num();
    match ev.key().as_str() {
        "sound_mastervolume" => sound.master = v.clamp(0.0, 1.0),
        "sound_sfxvolume" => sound.sfx = v.clamp(0.0, 1.0),
        "sound_musicvolume" => sound.music = v.clamp(0.0, 1.0),
        "sound_ambiencevolume" => sound.ambience = v.clamp(0.0, 1.0),
        "sound_enableallsound" => sound.enabled = v != 0.0,
        "sound_enablesfx" => sound.sfx_enabled = v != 0.0,
        "sound_enablemusic" => sound.music_enabled = v != 0.0,
        "sound_enableambience" => sound.ambience_enabled = v != 0.0,
        "sound_enableerrorspeech" => sound.error_speech = v != 0.0,
        "sound_enablesoundwhengameisinbg" => sound.background_sound = v != 0.0,
        "sound_enablereverb" => sound.reverb = v != 0.0,
        "soundoutputlimiter" => sound.limiter = v != 0.0,
        "sound_listeneratcharacter" => sound.listener_at_character = v != 0.0,
        "sound_enableemotesounds" => sound.emote_sounds = v != 0.0,
        "sound_zonemusicnodelay" => sound.zone_music_no_delay = v != 0.0,
        _ => {}
    }
}
