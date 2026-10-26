"""Shared webOS remote session for desktop and Android."""

from __future__ import annotations

import asyncio
import json

from bscpylgtv import WebOsClient


BUTTONS = {
    "UP", "DOWN", "LEFT", "RIGHT", "ENTER", "BACK", "EXIT", "HOME", "MENU", "POWER", "INPUT_HUB",
    "MUTE", "VOLUMEUP", "VOLUMEDOWN", "CHANNELUP", "CHANNELDOWN",
    "PLAY", "PAUSE", "STOP", "REWIND", "FASTFORWARD", "INFO", "GUIDE",
    "RED", "GREEN", "YELLOW", "BLUE", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
}


class MemoryStorage:
    def __init__(self, key=None):
        self.key = key

    async def get_key(self, _address):
        return self.key

    async def set_key(self, _address, value):
        self.key = value

    async def list_keys(self):
        return {}


class RemoteBridge:
    def __init__(self):
        self.client = None
        self.identity = None

    async def close(self):
        if self.client is not None:
            try:
                await self.client.disconnect()
            finally:
                self.client = None
                self.identity = None

    async def handle(self, request):
        request_id = request.get("id")
        try:
            action = request.get("action")
            if action == "ping":
                result = "pong"
            elif action in ("button", "pair"):
                button = request.get("button")
                if action == "button" and button not in BUTTONS:
                    raise ValueError("Unsupported remote button")
                ip = request["tv_ip"]
                client_key = request.get("client_key") or None
                if self.client is None or self.identity != ip:
                    await self.close()
                    self.client = await WebOsClient.create(
                        ip, storage=MemoryStorage(client_key), states=[], ping_interval=None,
                        timeout_connect=45 if action == "pair" else 10,
                        connect_retry_attempts=1,
                    )
                    try:
                        await self.client.connect()
                    except Exception:
                        await self.close()
                        raise
                    self.identity = ip
                try:
                    result = await self.client.button(button) if action == "button" else "paired"
                except Exception:
                    await self.close()
                    raise
            else:
                raise ValueError("Unsupported sidecar action")
            response = {"id": request_id, "ok": True, "data": result}
            if self.client is not None:
                response["client_key"] = self.client.client_key
            return response
        except Exception as exc:
            return {"id": request_id, "ok": False, "error": str(exc)}


class MobileBridge:
    """Chaquopy calls this synchronously on one dedicated worker thread."""

    def __init__(self):
        self.loop = asyncio.new_event_loop()
        self.remote = RemoteBridge()

    def request_json(self, source):
        request = json.loads(source)
        return json.dumps(self.loop.run_until_complete(self.remote.handle(request)), default=str)


mobile_bridge = MobileBridge()
