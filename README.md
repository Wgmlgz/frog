# Frog

Frog is a Tauri desktop app for controlling devices on your network. The first device is an LG webOS TV. The app lives at the repository root; TV-specific code and setup files live in [`tv/`](tv/README.md), [`src/devices/tv/`](src/devices/tv/TvPanel.tsx), and [`src-tauri/src/devices/tv.rs`](src-tauri/src/devices/tv.rs).

The desktop stack follows the structure of [codex-auth-gui](https://github.com/Wgmlgz/codex-auth-gui): Tauri 2, React 19, TypeScript, Vite, Tailwind CSS 4, TanStack Query, and a small utility-style UI. CI builds Windows, Linux, and both macOS architectures.

## Architecture

- Rust owns TV settings, root SSH, internal webOS Luna calls, audio, and Bluetooth reconnect logic.
- The bundled `bscpylgtv` Python library is used only for remote-button emulation. A minimal Python sidecar keeps that library's webOS session open between button presses. The Python runtime and library are packaged into the app; a separate Python install is unnecessary for end users.
- TV IP, MAC, broadcast address, preferred headphones, SSH key path, and the webOS pairing key are kept together in `settings.toml`. The SSH private key remains in its OpenSSH file, referenced by that TOML setting. Neither file is included in installers.

The TV has already been rooted and paired in this workspace. Development uses the ignored [`tv/settings.toml`](tv/settings.example.toml) file; installed builds store `settings.toml` in the OS app data directory. On an installed build, open Settings and point **Root SSH private key path** to your copy of `tv/.tv_root_key`. The remote's first press may require accepting a TV pairing prompt. Audio controls use root SSH and do not create webOS controller notices.

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

