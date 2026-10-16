# Wrath character eye selection

The effective installed HumanMale model includes geoset 1703: eight vertices with
`DeathKnightEyeGlow.blp`, unlit/additive. The old character filter kept every ID
above 1700 visible, importing the Vanilla managed-range boundary into Wrath.
Thus even the zero-appearance human warrior Benchone received the DK eye cards.
This is independent of face-atlas scaling and the base skin texture binding.

Read-only build-12340 evidence:

- `0x4ed900` clears submeshes 0 through 2000 via `0x82c7c0`, enables body 0,
  then enables 19 selected group entries at character-component `+0x144`.
- Group 17 is overridden to 1703 when class at `+0x20` equals 6, or the selected
  face CharSections runtime row has flag 4 at `+0x1c`.
- `0x4ea590` and `0x4ee460` install facial-style row fields at runtime offsets
  `+0x18/+0x1c` into groups 16/17, adding 1600/1700. These are Wrath raw fields
  6/7, previously discarded. The default bases are 1601, 1701, 1801.
- Vanilla's existing managed boundary of 1700 and unselected higher-ID retention
  remain unchanged. The Wrath override is not applied to Vanilla.

The correction changes visibility, not texture pixels, materials, lighting, or
atlas layout. The selected CharSections row's flags follow the same duplicate
precedence as its texture names. World attach, redress, and preview share the
class/face selection helper; portraits using those parts inherit the selection.
Corpse appearance snapshots currently expose no class; they retain selected-face
flag behavior but do not invent class from the absent snapshot field.

Installed evidence is the effective Wrath/enUS chain (including patch overrides),
not stock-install certification. No native executable was launched and no binary
or asset-derived dump is committed. Headless library and installed-data regressions
verify warrior exclusion, DK and flagged-face admission, and Vanilla isolation;
this is not a human visual acceptance claim.

Live redress remains equipment-change gated: a face/class-only descriptor update
is not an independent rebuild trigger. This change does not add dynamic appearance
update support. Preview look equality does include class, so a class-only preview
change invalidates its assembly; merged geometry caches use the selected member set,
while texture-only cache keys remain unchanged.
