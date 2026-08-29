# AW Switcher

A tiny menu bar / system tray app that switches your monitor's input source
over DDC/CI, triggered by a global hotkey you configure. Written for the
Dell Alienware AW3926QW but works with any monitor that supports DDC/CI
input switching (VCP feature `0x60`).

Runs on macOS and Windows from one codebase.

## Install / build

Requires the [Rust toolchain](https://rustup.rs).

```bash
cargo build --release
```

The binary is at `target/release/aw-switcher` (`aw-switcher.exe` on Windows).
Run it and a small monitor icon appears in your menu bar / system tray.

On first launch it writes a default config file and tries to auto-detect
your monitor.

## Enabling DDC/CI

Most monitors ship with DDC/CI turned off or need it explicitly allowed for
software control. On the AW3926QW: OSD menu → **Menu** → **Others** → set
**DDC/CI** to **On**. If input switching doesn't work, check this first.

## Configuration

Config lives at:

- macOS: `~/Library/Application Support/aw-switcher/config.toml`
- Windows: `%APPDATA%\aw-switcher\config.toml`

```toml
hotkey = "CmdOrCtrl+Alt+I"
monitor_match = "AW3926"

[[inputs]]
name = "HDMI 1"
code = 0x11

[[inputs]]
name = "HDMI 2"
code = 0x12

[[inputs]]
name = "DisplayPort"
code = 0x0f

[[inputs]]
name = "USB-C"
code = 0x1b
```

The defaults above match the AW3926QW's actual ports: 2x HDMI, 1x
DisplayPort 2.1, and 1x USB-C (10Gbps upstream, DisplayPort 2.1 Alt Mode).

- **`hotkey`** — the global shortcut that cycles to the *next* input in the
  `inputs` list. Format is `Modifier+Modifier+Key`, e.g. `Shift+Alt+KeyD`,
  `Ctrl+F13`. `CmdOrCtrl` maps to ⌘ on macOS and Ctrl on Windows. Full key/
  modifier names come from the [`global-hotkey`
  crate](https://docs.rs/global-hotkey/latest/global_hotkey/hotkey/index.html).
- **`monitor_match`** — case-insensitive substring matched against the
  connected display's model name. Only used to pick the right monitor when
  more than one DDC/CI display is connected; if only one is found, it's
  used regardless of this setting.
- **`inputs`** — the list you cycle through with the hotkey, and that shows
  up (with a checkmark on the active one) in the tray menu for switching
  directly. `code` is the VCP `0x60` input value; standard MCCS values are
  `0x0f`/`0x10` for DisplayPort 1/2 and `0x11`/`0x12` for HDMI 1/2. `0x1b`
  is included by default for USB-C, a vendor-specific Dell code seen on
  other Dell/Alienware monitors (e.g. the U3818DW) for "DisplayPort over
  USB-C" — it is **not yet confirmed** against the AW3926QW specifically.
  If a code doesn't work, query the monitor's capabilities string with
  [`ddcutil capabilities`](https://www.ddcutil.com/) (Linux) or a tool like
  [BetterDisplay](https://github.com/waydabber/BetterDisplay) (macOS) to see
  what it actually advertises for feature `60`.

Use the tray menu's **Reload Config & Rescan Monitor** item to pick up
changes without restarting.

## Troubleshooting

Run `cargo run --example probe` for a read-only dump of a connected
monitor's raw VCP `0x60` value and its full DDC/CI capabilities string
(look for a `60(...)` segment listing the input codes it actually
supports — that's ground truth over any guessed defaults). It won't
switch your input.

If clicking an input in the tray does nothing visible:

- **Nothing is connected to that input.** DDC/CI switches happily even to
  a port with no signal; many monitors then show "no signal" briefly and
  auto-revert to the last input that had one, which looks identical to
  the switch not having happened at all.
- **DDC/CI is flaky by nature.** A dropped or truncated reply on an
  otherwise-working link is normal; this app retries reads/writes a few
  times with backoff, but if your setup (an active docking station/hub in
  particular) is especially lossy, that may not be enough.
- **Another DDC tool is running.** Two programs (e.g. this and
  BetterDisplay/MonitorControl) polling or writing DDC/CI at once can
  collide; try quitting the other one.

## How it works

- [`ddc-hi`](https://docs.rs/ddc-hi) talks DDC/CI to the monitor (via
  IOKit on macOS, the Monitor Configuration API on Windows).
- [`global-hotkey`](https://docs.rs/global-hotkey) and
  [`tray-icon`](https://docs.rs/tray-icon) (both from the Tauri project)
  provide the cross-platform hotkey and tray icon/menu.

## License

MIT
