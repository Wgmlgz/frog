# Frog Android TV Python bridge

This local Tauri plugin runs `tv/remote_bridge.py` inside the Android app through Chaquopy. The Rust side passes JSON requests to a Kotlin worker, which keeps one Python event loop and one webOS connection alive across remote button presses.

Run `npm run android:prepare` from the repository root before an Android build. That command copies the shared bridge and vendored `bscpylgtv` source into the plugin's ignored `android/src/main/python/` directory. The Android build scripts refresh this copy automatically.
