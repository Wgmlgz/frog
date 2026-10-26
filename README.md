# Frog

Frog is a Tauri app for controlling devices on your network. It supports any number of LG webOS TVs, Xiaomi Mi Bedside Lamps, and Yeelight LED Bulb 1S color bulbs. The home screen lists device instances; select one to open its controls, or use **Add device** to scan and set up another. A device is saved only after pairing or local lamp control succeeds.

The app uses Tauri 2, React 19, TypeScript, Vite, Tailwind CSS 4, and TanStack Query. CI builds Windows, Linux, and both macOS architectures.

## Architecture

- Rust owns device settings, root SSH, internal webOS Luna calls, audio, and Bluetooth reconnect logic.
- Rust also talks directly to the lamps over the local Xiaomi miIO UDP protocol. It supports power, brightness, color temperature, and RGB color without a Python runtime or a Xiaomi server connection during control.
- Setup scans local miIO devices and probes webOS TVs. Select an unconnected result or enter an IP address to probe a device on another subnet. For a lamp, the wizard can retrieve its token through Xiaomi account sign-in. CAPTCHA characters and email verification codes are entered manually in the wizard. A token obtained elsewhere can also be entered manually. Frog saves the Xiaomi session token in `frog.toml` so the account can be selected for another device; the account password is not saved. An expired session requires another sign-in.
- The bundled `bscpylgtv` Python library is used only for remote-button emulation. Desktop builds run it in a packaged Python sidecar. Android builds run the same bridge in embedded Python through Chaquopy. Both keep the webOS session open between button presses; users do not need to install Python.
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

## Android

Install Android Studio with SDK platform 37, Android SDK command-line tools, NDK 28.2.13676358, JDK 17 or newer, and host Python 3.12. Set `ANDROID_HOME`, `NDK_HOME`, and `JAVA_HOME` to those installations. If Chaquopy cannot find Python 3.12 on `PATH`, set `FROG_BUILD_PYTHON` to its executable path. Install the Rust targets you plan to package:

```text
rustup target add aarch64-linux-android x86_64-linux-android
npm ci
npm run android:prepare
npm run android:build:debug
```

`android:prepare` generates the Tauri Android project and stages only the shared remote bridge and vendored `bscpylgtv` package for Chaquopy. Run it once after cloning; the build scripts refresh the staged Python files each time. The installable ARM64 debug APK is written to `src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`. `npm run android:build:apk` builds a release APK, and `npm run android:build:aab` builds an Android App Bundle for ARM64 and x86_64.

For a signed release, create a private Android keystore and put `keystore.properties` in `src-tauri/gen/android/` with `storeFile`, `storePassword`, `keyAlias`, and `keyPassword` entries. The build script connects this file to the generated Gradle project; without it, release artifacts are unsigned. Keep both files outside Git. See [Tauri's Android signing guide](https://v2.tauri.app/distribute/sign/android/) for keystore creation and distribution steps.

The app requests local network access on Android versions that require it. The phone and devices must be on a reachable local network. TV remote buttons use webOS pairing; TV audio and Bluetooth features additionally need the TV's root SSH key. You can transfer existing device settings using Frog's clipboard export and import controls. An APK build verifies packaging; pairing, discovery, lamp control, and TV controls should also be checked on a physical Android device on the same network.

The Android launcher includes a 1×1 **All lights** widget. Add it from the launcher's widget picker. Its white state means all configured lamps are on; dark means they are off or mixed. Tapping turns all lamps off when all are on, and turns them all on otherwise. The widget needs the same local network access as lamp control in the app, and shows **Offline** if it cannot read or change the lamps. It refreshes after lamp or settings changes in Frog and on the launcher's periodic widget update.

Other Android widgets are **One lamp** (1×1), **TV power** (2×1), **TV volume** (2×1), **TV output** (2×1), **TV audio** (2×2), and **TV quick controls** (2×2). Adding one opens a device picker so each instance controls the selected lamp or TV. On the TV power widget, the power button switches the TV off through the paired webOS remote or wakes it with Wake-on-LAN; tapping the larger name/status area opens Frog. TV volume has minus and plus buttons, TV output switches between speakers and headphones, TV audio combines volume and output controls, and TV quick controls combines power and output switching. TV audio widgets require the root SSH key already used by the app, while TV wake requires a correct MAC and broadcast address and Wake-on-LAN enabled on the TV. Widget network operations run in Android background work.

