"""JSON-lines desktop adapter for the shared webOS remote bridge."""

from __future__ import annotations

import asyncio
import json
import sys

from remote_bridge import RemoteBridge


async def main() -> None:
    remote = RemoteBridge()
    try:
        while line := await asyncio.to_thread(sys.stdin.readline):
            try:
                response = await remote.handle(json.loads(line))
            except Exception as exc:
                response = {"id": None, "ok": False, "error": str(exc)}
            print(json.dumps(response, default=str), flush=True)
    finally:
        await remote.close()


if __name__ == "__main__":
    asyncio.run(main())
