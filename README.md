# signaged-util (signage-client-service)

The companion binary of the OMNIPLAY digital signage player. It runs continuously next to the
playback daemon `signaged` (repo omnicommander-oc1/omniplay-device) on the device (a Raspberry Pi),
reports vitals to the OMNIPLAY API every 30 seconds, polls the remote actions (restart app, reboot,
screenshot), applies playlist schedules and keeps `data.json` / `playlist.txt` in step with the API.

## Getting Started

Before you can run the client, you need Rust installed: [Rust Getting Started Guide](https://www.rust-lang.org/learn/get-started).

Additionally, a config file `signage.json` needs to be set up properly in the config directory (see
the "Configuration" section for more information).

To run the client, use `cargo run`. This will download and compile all of the dependencies, as well as compile and run the client.

The crate is Linux-only (unix signals, systemd, Wayland/X11 capture tools).

## Configuration

The binary follows the XDG Base Directory spec (`src/paths.rs`, the same rules as `signaged`), so
both binaries read the same config and share one data directory:

1. `$XDG_CONFIG_HOME/signage` for the config file (`signage.json`) and `remoteit.state`;
   `$HOME/.config/signage` when `XDG_CONFIG_HOME` is unset, empty or not an absolute path.
2. `$XDG_DATA_HOME/signage` for `data.json`, `playlist.txt`, the downloaded media and
   `screenshots/`; `$HOME/.local/share/signage` when `XDG_DATA_HOME` is unset, empty or not an
   absolute path.

On the OMNIPLAY Pi OS image (`signaged-util.service`) that is `/var/lib/omniplay/.config/signage`
and `/var/lib/omniplay/.local/share/signage`. On a fielded device whose old unit runs the binary as
root without XDG variables it stays `/root/.config/signage` and `/root/.local/share/signage`.

### Example signage.json
```json
{
  "url": "https://ds-api.omnicommando.com",
  "id": "<client_id>",
  "username": "<username>",
  "password": "<password>"
}
```

## Installation

Devices get the binary through the signage-updater orchestrator release tarball; `/usr/bin/signaged-util`.

On the OMNIPLAY Pi OS image the binary runs as the unprivileged `omniplay` user under
`NoNewPrivileges=yes` and `ProtectSystem=strict`:

* `signaged-util.service` in this repo documents the unit (user, XDG environment, sandbox); the
  image repo ships its own copy, kept in step with this one.
* `polkit/10-omniplay-signaged-util.rules` lets that user restart `signaged.service` and reboot the
  device with plain `systemctl` (systemd and logind authorise the D-Bus request through polkit; no
  sudo). The image installs it as `/etc/polkit-1/rules.d/10-omniplay-signaged-util.rules`.
* Screenshots need `grim` (Wayland, through cage's socket in `XDG_RUNTIME_DIR`). The `ffmpeg x11grab`
  fallback runs only when `DISPLAY` is already set in the environment; the binary never sets it.

When running as root (the fielded fleet's old unit) the binary keeps using `sudo systemctl restart
signaged.service` and `sudo reboot`, and drops to the `pi` user for `grim`.

## Development

Build, test and lint on Linux (or in a `rust:<toolchain>-bookworm` container):

```bash
cargo test && cargo clippy && cargo fmt --check
```

## TODO:

- only download videos from whitelist
- move from tokio to blocking (we actually don't need tokio at all)
