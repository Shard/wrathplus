# Packaging assets

Assets for a future packaged WrathPlus. The bundle pipeline itself is not built yet.

## `icon/`

The app icon is our own art: an icy "W" and a gold "+" on a night-blue plate, in the login
wordmark's palette. No Blizzard assets are involved; `icon/gen` draws it procedurally.

| File | Use |
|---|---|
| `wrathplus-icon-1024.png` | The master render. |
| `wrathplus-icon-256.png` | Embedded in the binary (`wrathplus-app-world::app_icon`): the window icon and, on macOS, the Dock icon. |
| `WrathPlus.iconset/`, `WrathPlus.icns` | The macOS bundle icon (`CFBundleIconFile`). |
| `wrathplus.ico` | The Windows icon. |

To regenerate the icon set:

```sh
cd packaging/icon/gen && cargo run --release
cd .. && iconutil -c icns WrathPlus.iconset -o WrathPlus.icns
```

## `macos/Info.plist.template`

The `Info.plist` for a future `WrathPlus.app`. The packaging step fills `@VERSION@` and
`@BUILD@`.
