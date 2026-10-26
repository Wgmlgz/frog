"""Direct control of paired Bluetooth audio devices on a rooted LG webOS TV."""

from __future__ import annotations

import json
import re
import shutil
import subprocess
from pathlib import Path


ADDRESS = re.compile(r"^[0-9A-Fa-f]{2}(?::[0-9A-Fa-f]{2}){5}$")


class BluetoothError(RuntimeError):
    pass


def luna_call(tv_ip: str, key: Path, method: str, payload: dict,
              service: str = "com.webos.service.bluetooth") -> dict:
    """Call an internal Luna method over root SSH without a TV UI action."""
    if not shutil.which("ssh"):
        raise BluetoothError("OpenSSH ssh is required")
    if not key.is_file():
        raise BluetoothError("Root SSH key is missing; run root_setup.py first")
    if not re.fullmatch(r"[A-Za-z0-9/_]+", method) or not re.fullmatch(r"[A-Za-z0-9.]+", service):
        raise ValueError("Invalid Luna method")
    request = json.dumps(payload, separators=(",", ":"))
    command = f"luna-send -w 5000 -n 1 -f luna://{service}/{method} '{request}' </dev/null\n"
    known_hosts = key.parent / ".tv_known_hosts"
    result = subprocess.run(
        ["ssh", "-i", str(key), "-o", "BatchMode=yes", "-o", "ConnectTimeout=5",
         "-o", "StrictHostKeyChecking=accept-new", "-o", f"UserKnownHostsFile={known_hosts}",
         f"root@{tv_ip}", "sh", "-s"],
        input=command.encode("ascii"), capture_output=True, timeout=12,
    )
    if result.returncode or not result.stdout.strip():
        detail = result.stderr.decode(errors="replace").strip()
        raise BluetoothError(detail or "The TV did not answer the root Bluetooth command")
    try:
        response = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise BluetoothError("Invalid Bluetooth response from TV") from exc
    if response.get("returnValue") is not True:
        raise BluetoothError(response.get("errorText") or str(response))
    return response


def paired_audio_devices(tv_ip: str, key: Path) -> list[dict]:
    response = luna_call(tv_ip, key, "gap/getTrustedDevices", {"service": "audio"})
    return response.get("device", [])


def connect_audio_device(tv_ip: str, key: Path, address: str) -> dict:
    if not ADDRESS.fullmatch(address):
        raise ValueError("Invalid Bluetooth device address")
    devices = paired_audio_devices(tv_ip, key)
    if not any(item.get("address", "").lower() == address.lower() for item in devices):
        raise BluetoothError("This Bluetooth device is not paired with the TV")
    return luna_call(tv_ip, key, "service/connect", {"service": "audio", "address": address})
