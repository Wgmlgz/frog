"""Cross-platform CLI and local audio page for an LG webOS TV."""

from __future__ import annotations

import argparse
import asyncio
import json
import re
import socket
import time
import tomllib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Lock
from urllib.parse import urlsplit

from bscpylgtv import WebOsClient
from bluetooth import BluetoothError, connect_audio_device, luna_call, paired_audio_devices


HERE = Path(__file__).resolve().parent
SETTINGS_FILE = HERE / "settings.toml"
CONFIG = tomllib.loads((SETTINGS_FILE if SETTINGS_FILE.exists() else HERE / "settings.example.toml").read_text(encoding="utf-8"))
KEY_FILE = HERE / ".aiopylgtv.sqlite"
TV_LOCK = Lock()
ROOT_KEY = HERE / ".tv_root_key"


def get_paired_headphones():
    return paired_audio_devices(CONFIG["tv_ip"], ROOT_KEY)


def rooted_call(service, method, payload):
    return luna_call(CONFIG["tv_ip"], ROOT_KEY, method, payload, service)


def rooted_audio_state():
    status = rooted_call("com.webos.service.audio", "master/getVolume", {})["volumeStatus"]
    return {"volume": status["volume"], "sound_output": status["soundOutput"]}


def rooted_set_volume(volume):
    output = rooted_audio_state()["sound_output"]
    return rooted_call("com.webos.service.audio", "master/setVolume",
                       {"soundOutput": output, "volume": volume})


def rooted_change_output(output):
    return rooted_call("com.webos.service.apiadapter", "audio/changeSoundOutput", {"output": output})


def connect_headphones(address=None):
    address = address or CONFIG.get("preferred_bluetooth_address")
    if not address:
        devices = get_paired_headphones()
        if len(devices) != 1:
            raise ValueError("Select a paired Bluetooth device address")
        address = devices[0]["address"]
    with TV_LOCK:
        result = connect_audio_device(CONFIG["tv_ip"], ROOT_KEY, address)
        stable = 0
        for attempt in range(12):
            time.sleep(1)
            devices = get_paired_headphones()
            connected = any(d.get("address", "").lower() == address.lower() and d.get("state")
                            for d in devices)
            output = rooted_audio_state()["sound_output"]
            if output == "bt_soundbar" and connected:
                stable += 1
                if stable >= 2:
                    return result
            else:
                stable = 0
            if connected and attempt >= 2 and output != "bt_soundbar":
                rooted_change_output("bt_soundbar")
        raise BluetoothError("Headphones did not connect; check that they are powered on")


async def call_tv(method: str, *args):
    """Connect, invoke one library method, then disconnect."""
    client = await WebOsClient.create(
        CONFIG["tv_ip"],
        key_file_path=str(KEY_FILE),
        states=[],
        ping_interval=None,
        timeout_connect=2,
        connect_retry_attempts=3,
    )
    await client.connect()
    try:
        return await getattr(client, method)(*args)
    finally:
        await client.disconnect()


def run_tv(method: str, *args):
    with TV_LOCK:
        return asyncio.run(call_tv(method, *args))


async def read_tv_state():
    client = await WebOsClient.create(
        CONFIG["tv_ip"],
        key_file_path=str(KEY_FILE),
        states=[],
        ping_interval=None,
        timeout_connect=2,
        connect_retry_attempts=3,
    )
    await client.connect()
    try:
        return {
            "volume": await client.get_volume(),
            "sound_output": await client.get_sound_output(),
        }
    finally:
        await client.disconnect()


def get_tv_state():
    with TV_LOCK:
        return rooted_audio_state()


def wake_tv():
    mac = bytes.fromhex(re.sub(r"[:-]", "", CONFIG["tv_mac"]))
    if len(mac) != 6:
        raise ValueError("tv_mac must contain six bytes")
    packet = b"\xff" * 6 + mac * 16
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        sock.sendto(packet, (CONFIG["broadcast_ip"], 9))
    return "Wake packet sent"


class TvHandler(BaseHTTPRequestHandler):
    def respond(self, status: int, data: dict):
        body = json.dumps(data).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = urlsplit(self.path).path
        if path == "/":
            body = (HERE / "index.html").read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.end_headers()
            self.wfile.write(body)
        elif path == "/api/state":
            try:
                self.respond(200, get_tv_state())
            except Exception as exc:
                self.respond(503, {"error": str(exc)})
        elif path == "/api/bluetooth/devices":
            try:
                self.respond(200, {"devices": get_paired_headphones()})
            except Exception as exc:
                self.respond(503, {"error": str(exc)})
        else:
            self.respond(404, {"error": "Not found"})

    def do_POST(self):
        path = urlsplit(self.path).path
        if path not in ("/api/volume", "/api/audio", "/api/bluetooth/connect"):
            self.respond(404, {"error": "Not found"})
            return
        origin = self.headers.get("Origin")
        if origin and origin != f"http://{self.headers.get('Host')}":
            self.respond(403, {"error": "Cross-origin requests are blocked"})
            return
        if self.headers.get("Content-Type", "").split(";")[0].strip() != "application/json":
            self.respond(415, {"error": "JSON required"})
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if not 0 < length <= 1024:
                raise ValueError("Invalid request size")
            data = json.loads(self.rfile.read(length))
            if path == "/api/volume":
                volume = data.get("volume")
                if type(volume) is not int or not 0 <= volume <= 100:
                    raise ValueError("Volume must be an integer from 0 to 100")
                with TV_LOCK:
                    rooted_set_volume(volume)
            elif path == "/api/audio":
                output = data.get("output")
                if output not in ("tv_speaker", "bt_soundbar"):
                    raise ValueError("Output must be tv_speaker or bt_soundbar")
                with TV_LOCK:
                    rooted_change_output(output)
            elif path == "/api/bluetooth/connect":
                connect_headphones(data.get("address"))
            self.respond(200, get_tv_state())
        except (ValueError, json.JSONDecodeError) as exc:
            self.respond(400, {"error": str(exc)})
        except Exception as exc:
            self.respond(503, {"error": str(exc)})


def main():
    parser = argparse.ArgumentParser(description="Control the LG TV at 192.168.31.89")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("serve", help="Start the local audio page")
    sub.add_parser("info", help="Read volume")
    sub.add_parser("model", help="Read TV model")
    sub.add_parser("get-sound", help="Read current sound output")
    sub.add_parser("bluetooth-devices", help="List Bluetooth audio devices paired with the TV")
    p = sub.add_parser("bluetooth-connect", help="Connect the preferred paired headphones directly")
    p.add_argument("address", nargs="?", help="Optional paired device Bluetooth address")
    sub.add_parser("wake", help="Send Wake-on-LAN packet")
    sub.add_parser("off", help="Put TV in standby")
    sub.add_parser("screen-off", help="Turn panel off while TV stays on")
    sub.add_parser("screen-on", help="Turn panel back on")
    sub.add_parser("volume-up")
    sub.add_parser("volume-down")
    sub.add_parser("mute", help="Toggle mute")
    for name, help_text in (
        ("volume", "Set volume from 0 to 100"),
        ("input", "Switch to an input such as HDMI_2"),
        ("sound", "Select an output such as tv_speaker or bt_soundbar"),
        ("button", "Send a remote button such as HOME or DOWN"),
    ):
        p = sub.add_parser(name, help=help_text)
        p.add_argument("value")
    args = parser.parse_args()

    if args.command == "serve":
        server = ThreadingHTTPServer(("127.0.0.1", 8765), TvHandler)
        print("Audio page: http://127.0.0.1:8765/", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass
        finally:
            server.server_close()
        return

    mapping = {
        "info": "get_volume",
        "model": "get_system_info",
        "get-sound": "get_sound_output",
        "off": "power_off",
        "screen-off": "turn_screen_off",
        "screen-on": "turn_screen_on",
        "volume-up": "volume_up",
        "volume-down": "volume_down",
        "mute": "button",
        "volume": "set_volume",
        "input": "set_input",
        "sound": "change_sound_output",
        "button": "button",
    }
    if args.command == "wake":
        result = wake_tv()
    elif args.command == "bluetooth-devices":
        result = get_paired_headphones()
    elif args.command == "bluetooth-connect":
        result = connect_headphones(args.address)
    else:
        value = getattr(args, "value", None)
        if args.command == "volume":
            try:
                value = int(value)
            except ValueError:
                parser.error("volume must be an integer from 0 to 100")
            if not 0 <= value <= 100:
                parser.error("volume must be an integer from 0 to 100")
        if args.command == "mute":
            value = "MUTE"
        if args.command == "button":
            value = value.upper()
        result = run_tv(mapping[args.command], *([value] if value is not None else []))
    if result is not None:
        print(json.dumps(result, indent=2, default=str) if isinstance(result, (dict, list)) else result)


if __name__ == "__main__":
    main()
