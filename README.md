# Frog

Frog is a Tauri app for controlling devices on your network. It supports any number of LG webOS TVs, Xiaomi Mi Bedside Lamps, and Yeelight LED Bulb 1S color bulbs. The home screen lists device instances; select one to open its controls, or use **Add device** to scan and set up another. A device is saved only after pairing or local lamp control succeeds.

The desktop stack follows the structure of [codex-auth-gui](https://github.com/Wgmlgz/codex-auth-gui): Tauri 2, React 19, TypeScript, Vite, Tailwind CSS 4, TanStack Query, and a small utility-style UI. CI builds Windows, Linux, and both macOS architectures.

## Architecture

- Rust owns device settings, root SSH, internal webOS Luna calls, audio, and Bluetooth reconnect logic.
- Rust also talks directly to the lamps over the local Xiaomi miIO UDP protocol. It supports power, brightness, color temperature, and RGB color without a Python runtime or a Xiaomi server connection during control.
- Setup scans local miIO devices and probes webOS TVs. Select an unconnected result or enter an IP address to probe a device on another subnet. For a lamp, the wizard can retrieve its token through Xiaomi account sign-in. CAPTCHA characters and email verification codes are entered manually in the wizard. A token obtained elsewhere can also be entered manually. Frog saves the Xiaomi session token in `frog.toml` so the account can be selected for another device; the account password is not saved. An expired session requires another sign-in.
- The bundled `bscpylgtv` Python library is used only for remote-button emulation. A minimal Python sidecar keeps that library's webOS session open between button presses. The Python runtime and library are packaged into the app; a separate Python install is unnecessary for end users.
- All device instances and credentials live in one `frog.toml` in the OS app data directory. Each instance has a stable ID, name, type, and type-specific settings. The TV's SSH private key, webOS pairing key, lamp miIO tokens, and reusable Xiaomi account sessions are included in this TOML. The SSH key is materialized as a short-lived file when a TV command runs.
- On first run, Frog imports the previous TV settings and key plus the previous lamp settings. On Windows it can also import the local lamp token file if no previous lamp settings exist. The original files are left in place as a backup; Frog uses `frog.toml` afterward.
- **Export to clipboard** copies the complete TOML, including secrets. **Import from clipboard** validates it and replaces the current device list. Transfer it privately and clear the clipboard afterward. Device settings views do not return saved secrets to the frontend.

The TV has already been rooted and paired in this workspace. A new TV is added after the pairing prompt is accepted on the TV. An OpenSSH private key can be pasted during setup or later in Settings; remote buttons work without it, while audio and Bluetooth controls require root SSH. Lamp setup supports `yeelink.light.bslamp1` and `yeelink.light.color4`. Routine lamp control remains local after the token has been retrieved.

## Development

Clone with submodules:

```text
git clone --recurse-submodules <repository-url>
```

Install prerequisites: Node.js 22+, Rust, and Python 3.12+. The TV SSH connection uses an embedded Rust client, so no system OpenSSH client is needed. For a development build:

```text
python -m venv tv/.venv
```

Activate `tv/.venv` for your shell, then run:

```text
python -m pip install -e tv/vendor/bscpylgtv pyinstaller
npm ci
npm run sidecar
npm run tauri:dev
```

If using a virtual environment, install the Python packages into that environment. `npm run sidecar` uses `tv/.venv` when it exists and otherwise uses `python` or `python3` from `PATH`.

Checks and packaging:

```text
npm run lint
npm run build
npm run tauri:build
```

The CI workflow in [`.github/workflows/desktop.yml`](.github/workflows/desktop.yml) builds the Python sidecar for each target platform before building its Tauri installer. The older Python local server remains in `tv/` as a setup and troubleshooting tool.

