# CVar defaults against build 12340

Shannon's rule: the default behaves like stock 3.3.5. Every CVar this client registers takes build
12340's own registered default unless a row below says why it does not.

## Where a default comes from

- **Host rows** (`wrathplus-app-core/src/cvars/table.rs` and each feature's `CVARS`): a CVar with a
  host reader. `same` is 12340's registered string; `deviates`, `overridden` and `ours` carry
  their reason in the row.
- **The Lua shim** (`wrathplus-ui/src/script/wrath_compat.lua`, the `stock` table): every other
  stock CVar the FrameXML reads, registered only when the host has not.
- **Uvars**: stock settings with a Lua global (`InterfaceOptionsFrame.lua`'s `uvarInfo`, such as
  `QUEST_FADING_DISABLE`) take the uvar's own default; `InterfaceOptionsFrame_LoadUVars` never
  copies the CVar into it. Matching the CVar's registered default reproduces a fresh install.

## Audit, 2026-10-25

Every `CVar::Register` call (`0x767fc0`) in Wow.exe 12340 was read for its name and default
string, then compared with the host rows and the shim table. Some defaults are built at runtime:
`cameraSmoothStyle`/`cameraSmoothTrackingStyle` read `[0xad1c2c]` ("4"), `serviceTypeFilter` is
`%d` of 3, and the `gx*` and `mouseSpeed` defaults come from the hardware or the OS.

Changed to 12340's value in this pass: `trackerFilter` 7 and `trackerSorting` 0 (the quest
tracker), `autoSelfCast` 1, `ChatBubblesParty` 1, `lod` 0, `cameraSmoothStyle` and
`cameraSmoothTrackingStyle` 4, and in the shim `chatMouseScroll` 1, `consolidateBuffs` 0,
`conversationMode` popout, `displayFreeBagSlots` 0, `enablePVPNotifyAFK` 1, `fctLowManaHealth` 1,
`lootUnderMouse` 1, `predictedHealth` 1, `predictedPower` 1, `questFadingDisable` 1,
`showDispelDebuffs` 0, `toastDuration` 4, the three `Sound_Enable*` speech and emote switches 1,
`EnableMicrophone` and `autojoinPartyVoice` 1, `componentTextureLevel` 8, `textureFilteringMode` 1
and the stereo pair. `autoLootDefault` moved from `ours` to `same`: 12340 registers it "0".

## Deliberate deviations

| CVar | Ours | 12340 | Why |
| --- | --- | --- | --- |
| `ffxGlow`, `ffxDeath` | 1 | 0 | No stock Lua preset sets them and 12340's first-run hardware detection is untraced; the renderer keeps the passes on until that is traced. |
| `nearclip` | 0.1 | 0.2 | The near plane, the submersion test and the self-avatar fade were tuned at 0.1 (`NEARCLIP_DEFAULT`); 0.2 is untried here. |
| `spellEffectLevel` | 2 | 9 | The particle scalar follows 1.12's 0..2 Spell Detail scale; 12340's 0..9 slider is not ported. |
| `cameraSmoothStyle`, `cameraSmoothTrackingStyle` | 4 | 4 | Value matches; `FollowStyle` runs 4 ("Only when moving") as Smart, without its vertical return. |
| `gxApi`, `gxResolution`, `gxColorBits`, `gxDepthBits`, `gxFixLag`, `gxMultisample`, `mouseSpeed` | renderer's / 1 | hardware | 12340 builds these from the device or the OS at startup; ours describe the wgpu swapchain. |
| `weatherDensity` | 3 | 2 | The precipitation rates were derived and graded against the reference's own apitrace captures at 3 (row reason in `table.rs`). |
| `uiScale` | 0.9 | 1.0 | `overridden`: a fresh client never reads it while `useUiScale` is 0 (row reason). |
| `locale` | the install's | `****` | 12340's placeholder is replaced from the install; ours answers the data's own locale. |
| `renderScale`, `boothHalfRate`, `SoundOutputLimiter`, `fpsJournal` | ours | none | The client's own knobs (row reasons). |

## 1.12 names (2026-10-25)

The host once registered 1.12's names for settings 12340 calls something else or does not
register. Each now has a disposition.

| Name | Disposition | Why |
| --- | --- | --- |
| `MasterVolume`, `SoundVolume`, `MusicVolume`, `AmbienceVolume`, `MasterSoundEffects`, `EnableMusic`, `EnableAmbience`, `EnableErrorSpeech`, `SoundReverb`, `SoundListenerAtCharacter`, `EmoteSounds`, `SoundZoneMusicNoDelay` | **mapped** to `Sound_MasterVolume`, `Sound_SFXVolume`, `Sound_MusicVolume`, `Sound_AmbienceVolume`, `Sound_EnableAllSound`, `Sound_EnableMusic`, `Sound_EnableAmbience`, `Sound_EnableErrorSpeech`, `Sound_EnableReverb`, `Sound_ListenerAtCharacter`, `Sound_EnableEmoteSounds`, `Sound_ZoneMusicNoDelay` | The stock 3.3.5 Sound panel (`AudioOptionsPanels.lua`) writes these names; before, its sliders and boxes reached only the shim's Lua copies. A saved `config.toml` is migrated at load (`cvars::renamed`). `Sound_EnableSFX`, new in 3.3.5, is its own mixer switch. `Sound_EnableReverb` registers "0" in 12340, so the old reverb deviation is gone. |
| `SoundBufferSize` | **keep** (`ours`) | The render thread's mix-ahead; 12340 has no counterpart (`Sound_DSPBufferSize` is FMOD Ex's DSP buffer). |
| `statusBarText` | **retired** | Nothing in 3.3.5 reads it: the stock status text uses `playerStatusText` and its siblings (uvars) and `statusTextPercentage`. |
| `cameraBobbingFrequency`, `cameraBobbingLRAmplitude`, `cameraBobbingUDAmplitude`, `cameraBobbingSmoothSpeed` | **keep** | Tuning for the head-bob kernel, which runs only under `cameraBobbing` (12340 registers "0"); 12340 builds them in rather than registering them. |
| `WorldDetail` | **keep** | Backs the `GetWorldDetail`/`SetWorldDetail` verbs and the clutter stop paired with `frillDensity`. |
| `pixelShaders`, `trilinear` | **keep** (`overridden`) | Renderer switches (the FFX pass gate, the trilinear filter); no 3.3.5 panel writes them. |
| `anisotropic` | **keep** | Drives the texture filter. Open: 3.3.5's Video panel writes `textureFilteringMode`, which only the shim holds, so that control does not reach the renderer yet. |
| `doodadAnim`, `M2UseShaders`, `M2UsePixelShaders` | **keep** | No effect here (row comments): `doodadAnim` backs the `Get/SetDoodadAnim` verbs, and the M2 pair is kept for addons that write it. |

## Shim-only names

The `Mac`-client names in the shim (`MovieRecording*`, `iTunes*`) are not in the Windows build;
the stock Mac options panel reads them.
