# Frog

Frog is a Tauri app for controlling devices on your network. It supports any number of LG webOS TVs and Xiaomi Mi Bedside Lamps. The home screen lists device instances; select one to open its controls, or use **Add device** to create another.

The desktop stack follows the structure of [codex-auth-gui](https://github.com/Wgmlgz/codex-auth-gui): Tauri 2, React 19, TypeScript, Vite, Tailwind CSS 4, TanStack Query, and a small utility-style UI. CI builds Windows, Linux, and both macOS architectures.

## Architecture

- Rust owns device settings, root SSH, internal webOS Luna calls, audio, and Bluetooth reconnect logic.
- Rust also talks directly to the bedside lamp over the local Xiaomi miIO UDP protocol. It supports power, brightness, and color temperature without a Python runtime or a Xiaomi server connection during control.
- The bundled `bscpylgtv` Python library is used only for remote-button emulation. A minimal Python sidecar keeps that library's webOS session open between button presses. The Python runtime and library are packaged into the app; a separate Python install is unnecessary for end users.
- All device instances and credentials live in one `frog.toml` in the OS app data directory. Each instance has a stable ID, name, type, and type-specific settings. The TV's SSH private key, webOS pairing key, and the lamp's miIO token are included in this TOML. The SSH key is materialized as a short-lived file when a TV command runs.
- On first run, Frog imports the previous TV settings and key plus the previous lamp settings. On Windows it can also import the local lamp token file if no previous lamp settings exist. The original files are left in place as a backup; Frog uses `frog.toml` afterward.
- **Export to clipboard** copies the complete TOML, including secrets. **Import from clipboard** validates it and replaces the current device list. Transfer it privately and clear the clipboard afterward. Device settings views do not return saved secrets to the frontend.

The TV has already been rooted and paired in this workspace. A new TV instance needs its network details and the text of its OpenSSH private key in Settings. The remote's first press may require accepting a TV pairing prompt. Audio controls use root SSH and do not create webOS controller notices.

## Development

Clone with submodules:

```text
git clone --recurse-submodules <repository-url>
```

Install prerequisites: Node.js 22+, Rust, Python 3.12+, and OpenSSH client. For a development build:

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

