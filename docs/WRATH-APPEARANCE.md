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
  `NativeAppearance(AlterAppearance { hair, hair_color, facial_hair, skin_color })`. Native UI owns
  the Blizzard_BarberShopUI getters (`wrath_barber.rs`). `BARBER_SHOP_CLOSE` fires when the
  player's stand state leaves the chair states (`SIT_LOW_CHAIR`..`SIT_HIGH_CHAIR`, 4..6) after
  having sat in one. While the chair is open, `feed_barber` publishes the shop
  (`set_wrath_barber_shop`):
  - the `BarberShopStyle.dbc` rows for our race and sex, by type (0 hair, 2 facial hair, 3 skin;
    no skin rows is `CanAlterSkin`'s false);
  - `gtBarberShopCostBase` at our level;
  - the race's hair-color count and customization tokens (`CharCreateCatalog`);
  - `PLAYER_BYTES` as the current look.

  It republishes when those move. It drains `take_wrath_barber_requests`: `Apply` sends 0426, and
  `Cancel` sends `CMSG_STANDSTATECHANGE` 0, which closes the window as getting up does. Labelled:
  the native cancel (`0x52e9f0`) is untraced. The preview (`wrath_barber_preview`) dresses our own
  body through `entities::BarberPreview`. `Equipment::look` now carries the shown skin, hair style,
  hair color and facial hair (descriptor bytes, or the preview on our own body), so both a
  previewed and an applied haircut re-dress the model in place.
- **Titles.** 0373 prints `NEW_TITLE_EARNED` / `OLD_TITLE_LOST` as a system line with the
  `CharTitles.dbc` title for that bit, formatted with our name and gender. The known-title getters
  are Native UI's (`wrath_titles.rs`); App feeds them the catalog once per VM
  (`set_wrath_char_titles`) and the descriptor's three `PLAYER__FIELD_KNOWN_TITLES` words with
  `PLAYER_CHOSEN_TITLE` when they move (`set_wrath_player_titles`). `SetCurrentTitle` is 0374
  (`docs/WRATH-SESSION-META.md`).

Labelled (untraced in the client): the barber events' order, the money line and the cancel.
