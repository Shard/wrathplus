# Wrath appearance: helm/cloak, barber shop, titles (build 12340)

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03` `Server/Packets/CharacterPackets.cpp`
(`ShowingHelm`/`ShowingCloak::Read`: one `bool`), `Handlers/CharacterHandler.cpp`
(`HandleShowingHelmOpcode`, `HandleShowingCloakOpcode`, `HandleAlterAppearance`: four `uint32`
BarberShopStyle ids; `SMSG_BARBER_SHOP_RESULT` 0 done, 1 not enough money, 2 refused),
`Entities/GameObject/GameObject.cpp` (barber chair use: `SMSG_ENABLE_BARBER_SHOP`, empty) and
`Entities/Player/Player.cpp` (`SetTitle`: `SMSG_TITLE_EARNED` bit index, 1 earned / 0 lost).
Codec: `P/wrath_appearance.rs`; App: `A/net/wrath_appearance.rs`, `A/ui_unit.rs`
(`drain_worn_display_toggles`).

- **Helm and cloak.** 1.12's 02B9/02BA were toggles; 3.3.5's carry the shown state. The
  Options binding's flip becomes `NativeAppearance(ShowingHelm/ShowingCloak { show })`, showing what
  the player's hide bits (`PLAYER_FLAGS_HIDE_HELM`/`HIDE_CLOAK`) say is hidden.
- **Barber shop.** 0427 fires `BARBER_SHOP_OPEN`; 0428 result 0 fires `BARBER_SHOP_SUCCESS` then
  `BARBER_SHOP_APPEARANCE_APPLIED`, 1 shows `ERR_NOT_ENOUGH_MONEY`, 2 nothing. The writer is
  `NativeAppearance(AlterAppearance { hair, hair_color, facial_hair, skin_color })`; Native UI owns
  the Blizzard_BarberShopUI getters and will queue the apply (`take_wrath_barber_requests`, wired
  when it lands). `BARBER_SHOP_CLOSE` fires when the player's stand state leaves the chair states
  (`SIT_LOW_CHAIR`..`SIT_HIGH_CHAIR`, 4..6) after having sat in one.
- **Titles.** 0373 prints `NEW_TITLE_EARNED` / `OLD_TITLE_LOST` as a system line with the
  `CharTitles.dbc` title for that bit, formatted with our name and gender. The known-title getters
  are Native UI's; `SetCurrentTitle` is 0374 (`docs/WRATH-SESSION-META.md`).

Labelled (untraced in the client): the barber events' order and the money line.
