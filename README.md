<div align="center">
  <h1>benilla (AzerothBench fork)</h1>
  <p><b>A World of Warcraft 3.3.5a (build 12340) client, written from scratch in Rust and <a href="https://bevy.org">Bevy</a></b></p>
</div>

This fork ports [benilla](https://github.com/samwhosung/benilla), a from-scratch 1.12.1 client,
to Wrath of the Lich King 3.3.5a against an AzerothCore server. The 1.12 client paths are being
removed ([`docs/VANILLA-REMOVAL-PLAN.md`](docs/VANILLA-REMOVAL-PLAN.md)); the product is renamed
`wrathplus` in one scheduled step ([`docs/WRATHPLUS-RENAME-PLAN.md`](docs/WRATHPLUS-RENAME-PLAN.md)).
Every file format, the network protocol and the interface engine are written from scratch, with
no original client code and no bundled game assets. The fork's scope and its evaluation contract
are in [`docs/AZEROTHBENCH.md`](docs/AZEROTHBENCH.md).

## Status

The native 3.3.5 refactor is in progress. [`docs/WRATH-NATIVE-PLAN.md`](docs/WRATH-NATIVE-PLAN.md)
holds the workstreams and what is done in each, [`docs/WRATH-OPCODE-COVERAGE.md`](docs/WRATH-OPCODE-COVERAGE.md)
the wire coverage, and [`docs/WRATH-BUG-BACKLOG.md`](docs/WRATH-BUG-BACKLOG.md) what play-testing
has found. A feature counts as working only with tests and live evidence on 3.3.5 data
([`docs/AZEROTHBENCH.md`](docs/AZEROTHBENCH.md), "Port gates"); this README makes no capability
claims beyond those documents.

Two experience presets exist ([`docs/EXPERIENCE.md`](docs/EXPERIENCE.md)): **Classic**, the
default, matches the stock 3.3.5 client; **Plus** is an optional modern preset.

## What's inside

- **Formats:** readers for the 3.3.5 asset stack (the MPQ patch chain and its locale archives, BLP,
  DBC with the build-12340 layouts, ADT with MH2O liquid, WDT, WDL, M2 version 264 with its
  external skins and `.anim` files, and WMO), wired into Bevy as an asset source.
- **World:** streamed terrain, portal-culled buildings, doodads and ground clutter, liquid, sky,
  weather and the client's lighting.
- **Networking:** the build-12340 authentication and world session, and the object mirror into
  the ECS.
- **Interface:** a FrameXML and Lua engine that runs the stock 3.3.5 interface from your install's
  archives.
- **Audio:** music, ambience and effects.

The format readers and the UI engine core are plain Rust with no Bevy in them, and the world
renderer runs with no game attached. [`docs/MAP.md`](docs/MAP.md) maps every crate and subsystem,
generated from the code.

## Running it

You need:

- **A 3.3.5a client (build 12340)** for the game data. The client only reads it. `enUS` is the
  default locale; `WOW_LOCALE` selects another.
- **An AzerothCore 3.3.5a server**, on the stock auth port by default.
- **Stable Rust and a C compiler**, because the client's Lua is built from source: on macOS the
  Xcode command line tools, on Linux the ALSA and udev development packages and pkg-config, on
  Windows the MSVC build tools that the Rust installer sets up.

```sh
WOW_DATA=/path/to/WoW/Data cargo run --release -p wrathplus
```

`WOW_DATA` names the install's `Data` folder; a link to the install named `WoW` at the repo root
does the same. The server defaults to `127.0.0.1:3724`, or what the install's
`Data/<locale>/realmlist.wtf` sets; change it from the Realmlist button on the login screen, or for
one session with `--server <host[:port]>` (`WOW_HOST`). Credentials go in at the login screen.
Flags skip steps, each only its own: `--account <name>` with `--password-env <VAR>` or
`--password-stdin` logs in (the password is never an argument), `--realm <name>`,
`--character <name>` and `--enter-world`; `WOW_USER`, `WOW_PASS`, `WOW_REALM` and `WOW_CHAR`
(pick and enter) map onto the same steps. See `crates/wrathplus-app/src/launch_plan.rs`.

Settings, screenshots and addons live in `benilla-config/` at the repo root; addons go in
`benilla-config/AddOns/`. [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) has the rest, from the
player build to the tests.

---

benilla, by its author and contributors, is the upstream this fork builds on. Early inspiration
and file format guidance came from the [wowemulation-dev](https://github.com/wowemulation-dev)
community, and [warcraft-rs](https://github.com/wowemulation-dev/warcraft-rs) in particular.

This is an independent fan project, not affiliated with or endorsed by Blizzard Entertainment. It
ships **no Blizzard content**: no art, models, sounds, maps, MPQ contents or FrameXML. You provide
your own legally obtained 3.3.5a client, and the stock interface runs off its FrameXML at runtime.
The few files under `crates/wrathplus-app/assets/ui/` are our own, not copies of it: adapters over
stock files, and the settings windows and script error log the client draws itself.

World of Warcraft is a trademark of Blizzard Entertainment, Inc. Our own code is licensed under
[MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option. The two vendored components
under `third_party/`, the kira audio engine and a patched Lua 5.1, keep their own upstream
licenses, alongside each; their `BENILLA.md` files record what was changed and why.
