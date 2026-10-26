"""One-time, locally served dangbro root setup for the supported LG TV."""

from __future__ import annotations

import asyncio
import hashlib
import http.server
import ipaddress
import shutil
import socket
import subprocess
import threading
import urllib.request
from pathlib import Path

from tv import CONFIG, HERE, call_tv


REVISION = "1879c4874e3f96d8bc45796fe5fc94a9031753d6"
BASE = f"https://raw.githubusercontent.com/azoffshowy/dangbro/{REVISION}/docs/resources"
SOURCES = {
    "dangbro/index.html": "8c3de3c39a40b83c7c2b4be51df7165a64e6f034fee5a1ff989d04fdfbc2884a",
    "root_persistence.sh": "d9024fcd064bf0f7f730efb8e3dbaf55e56747c6e6fbf8224426c109a4dedfd2",
}
KEY = HERE / ".tv_root_key"


def fetch(name: str) -> str:
    data = urllib.request.urlopen(f"{BASE}/{name}", timeout=20).read()
    if hashlib.sha256(data).hexdigest() != SOURCES[name]:
        raise RuntimeError(f"Unexpected upstream content: {name}")
    return data.decode("utf-8")


def local_ip() -> str:
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.connect((CONFIG["tv_ip"], 9))
        address = sock.getsockname()[0]
    return str(ipaddress.ip_address(address))


def ssh_key() -> str:
    if not KEY.exists():
        if not shutil.which("ssh-keygen"):
            raise RuntimeError("OpenSSH ssh-keygen is required")
        subprocess.run(
            ["ssh-keygen", "-t", "ed25519", "-N", "", "-f", str(KEY)],
            check=True,
            stdout=subprocess.DEVNULL,
        )
    return Path(str(KEY) + ".pub").read_text(encoding="ascii").strip()


def prepare(host: str, port: int) -> tuple[bytes, bytes]:
    public_key = ssh_key()
    if "'" in public_key or "\n" in public_key:
        raise ValueError("Unexpected SSH public key")
    page = fetch("dangbro/index.html")
    script = fetch("root_persistence.sh")
    old_url = "https://azoffshowy.github.io/dangbro/resources/root_persistence.sh"
    if page.count(old_url) != 1 or script.count('show_final_alert "success"') != 1:
        raise RuntimeError("Upstream root flow changed")
    base = f"http://{host}:{port}"
    page = page.replace(old_url, base + "/root_persistence.sh")
    secure = f"""
# Provision private, key-only remote access before the first reboot.
mkdir -p /home/root/.ssh
printf '%s\\n' '{public_key}' > /home/root/.ssh/authorized_keys
chmod 700 /home/root/.ssh
chmod 600 /home/root/.ssh/authorized_keys
touch /var/luna/preferences/webosbrew_telnet_disabled
touch /var/luna/preferences/webosbrew_sshd_enabled
sync
curl -fsS --max-time 5 -X POST '{base}/success' >/dev/null 2>&1 || true
"""
    script = script.replace('show_final_alert "success"', secure + '\nshow_final_alert "success"')
    return page.encode(), script.encode()


def main() -> None:
    host, port = local_ip(), 8766
    page, script = prepare(host, port)
    success = threading.Event()
    hits: list[str] = []

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            hits.append(self.path)
            body = page if self.path == "/" else script if self.path == "/root_persistence.sh" else None
            if body is None:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Type", "text/html" if self.path == "/" else "text/plain")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            if self.path == "/success":
                success.set()
                self.send_response(204)
                self.end_headers()
            else:
                self.send_error(404)

        def log_message(self, format, *args):
            print(format % args, flush=True)

    server = http.server.ThreadingHTTPServer((host, port), Handler)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    print(f"Serving pinned root payload at http://{host}:{port}/", flush=True)
    try:
        result = asyncio.run(call_tv("launch_app_with_params", "com.webos.app.dangbei-overlay", {
            "source": "ssap-dangbro", "target": f"http://{host}:{port}/"
        }))
        print(f"Launch response: {result}", flush=True)
        if success.wait(120):
            print("Root setup reported success. Reboot the TV using its on-screen prompt.", flush=True)
            print(f"After reboot: ssh -i {KEY} root@{CONFIG['tv_ip']}", flush=True)
        else:
            print(f"No root success callback within two minutes. HTTP requests: {hits}", flush=True)
            print("Check the TV screen for an error; it may be patched or unable to reach this PC.", flush=True)
    finally:
        server.shutdown()


if __name__ == "__main__":
    main()
