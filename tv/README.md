# LG TV controller

Cross-platform Python control for the LG OLED42C21LA at `192.168.31.89`. The browser page exposes volume and direct TV speaker/WH-1000XM6 switching. Its Bluetooth button connects the paired headphones through the TV's internal Bluetooth service, then selects Bluetooth output. Normal page use does not open TV apps or create webOS controller popups.

The upstream [bscpylgtv](https://github.com/chros73/bscpylgtv) library is a Git submodule pinned to v0.5.5. Clone with `git clone --recurse-submodules ...`, or run `git submodule update --init --recursive` after cloning.

## Set up

From the `tv` folder, create a virtual environment and install the checked-out library:

```text
python -m venv .venv
```

Activate the environment for your shell, then run:

```text
python -m pip install -e vendor/bscpylgtv
python tv.py info
python tv.py serve
```

Open <http://127.0.0.1:8765/>. The server listens on this computer only. Stop it with Ctrl+C. The page requires root SSH access set up below.

The general CLI commands use webOS's normal network control API. On the first connection from a new computer, accept the pairing prompt on the TV. The legacy CLI stores its client key in `tv/.aiopylgtv.sqlite`, which Git ignores. Frog keeps each TV's settings, pairing key, and SSH private key in the shared `frog.toml` in the OS app data directory. Existing `tv/settings.toml` and `tv/.tv_root_key` files are imported on first run. Keep these files private.

## One-time root setup

The TV's standard network API cannot connect a particular paired Bluetooth device. This TV was rooted with [dangbro](https://github.com/azoffshowy/dangbro) at pinned revision `1879c4874e3f96d8bc45796fe5fc94a9031753d6`; the helper verifies the upstream file hashes before serving a local copy to the TV. The root script provisions a generated SSH public key and disables Telnet before reboot. It installs [Homebrew Channel](https://github.com/webosbrew/webos-homebrew-channel) v0.7.3 to keep root access available.

This one-time setup is already complete on this TV. Only run `python root_setup.py` to set up another compatible TV. It launches a temporary TV app and requires a full reboot. The private SSH key is `tv/.tv_root_key`; keep it private and back it up securely. Both it and the TV host key record are ignored by Git. Root setup can affect firmware updates and device stability, so use it only for compatible models.

Root access was tested after reboot as `uid=0`; Telnet was confirmed disabled. The local page uses SSH for all of its normal controls. If SSH is temporarily unavailable just after a reboot, wait for the TV to finish starting.

## CLI

```text
python tv.py info
python tv.py volume 30
python tv.py volume-up
python tv.py volume-down
python tv.py get-sound
python tv.py sound tv_speaker
python tv.py sound bt_soundbar
python tv.py bluetooth-devices
python tv.py bluetooth-connect
python tv.py input HDMI_2
python tv.py button HOME
python tv.py button DOWN
python tv.py button ENTER
python tv.py screen-off
python tv.py screen-on
python tv.py off
python tv.py wake
```

`sound bt_soundbar` only selects Bluetooth output. `bluetooth-connect` connects the configured paired headset and selects Bluetooth output without a TV app or remote navigation. An optional address selects another already paired device. Pairing a brand-new device is not implemented. `wake` sends a Wake-on-LAN packet. Standby wake has not been verified and may require **TV On With Mobile** or an equivalent TV setting.
