"""Minimal JSON-lines bridge to the bundled bscpylgtv library.

All TV audio, Bluetooth, SSH, and settings logic lives in the Tauri Rust app.
This process keeps one webOS connection for remote button presses.
"""

from __future__ import annotations

import asyncio
import json
import sys

from bscpylgtv import WebOsClient


BUTTONS = {
    "UP", "DOWN", "LEFT", "RIGHT", "ENTER", "BACK", "EXIT", "HOME", "MENU", "POWER", "INPUT_HUB",
    "MUTE", "VOLUMEUP", "VOLUMEDOWN", "CHANNELUP", "CHANNELDOWN",
    "PLAY", "PAUSE", "STOP", "REWIND", "FASTFORWARD", "INFO", "GUIDE",
    "RED", "GREEN", "YELLOW", "BLUE", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
}


class MemoryStorage:
    """Hand the library a pairing key supplied by Rust's TOML settings."""

    def __init__(self, key=None):
        self.key = key

    async def get_key(self, _address):
        return self.key

    async def set_key(self, _address, value):
        self.key = value

    async def list_keys(self):
        return {}


async def main() -> None:
    client = None
    identity = None
    while line := await asyncio.to_thread(sys.stdin.readline):
        request_id = None
        try:
            request = json.loads(line)
            request_id = request.get("id")
            if request.get("action") == "ping":
                result = "pong"
            elif request.get("action") == "button":
                button = request.get("button")
                if button not in BUTTONS:
                    raise ValueError("Unsupported remote button")
                ip = request["tv_ip"]
                client_key = request.get("client_key") or None
                if client is None or identity != ip:
                    if client is not None:
                        await client.disconnect()
                    client = await WebOsClient.create(
                        ip, storage=MemoryStorage(client_key), states=[], ping_interval=None,
                        timeout_connect=10, connect_retry_attempts=1,
                    )
                    await client.connect()
                    identity = ip
                try:
                    result = await client.button(button)
                except Exception:
                    await client.disconnect()
                    client = None
                    identity = None
                    raise
            else:
                raise ValueError("Unsupported sidecar action")
            response = {"id": request_id, "ok": True, "data": result}
            if client is not None:
                response["client_key"] = client.client_key
        except Exception as exc:
            response = {"id": request_id, "ok": False, "error": str(exc)}
        print(json.dumps(response, default=str), flush=True)
    if client is not None:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
