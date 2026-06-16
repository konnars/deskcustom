#!/usr/bin/env python3
"""Local update server for Deskcustom — run on any PC in your LAN.

Usage:
  python3 scripts/local-update-server.py dist

Then in Deskcustom (Windows) set update URL to:
  http://YOUR_LAN_IP:8765/latest.json

Put the NSIS installer in the folder before starting:
  dist/Deskcustom_0.1.1_x64-setup.exe
"""

from __future__ import annotations

import json
import os
import socket
import sys
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

PORT = int(os.environ.get("DESKCUSTOM_UPDATE_PORT", "8765"))
VERSION = os.environ.get("DESKCUSTOM_VERSION", "0.1.1")


def lan_ip() -> str:
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.connect(("8.8.8.8", 80))
            return sock.getsockname()[0]
    except OSError:
        return "127.0.0.1"


def write_manifest(folder: Path, host: str) -> None:
    manifest = {
        "version": VERSION,
        "notes": "Локальное обновление Deskcustom с Mac/PC в LAN",
        "platforms": {},
    }

    for file in sorted(folder.iterdir()):
        if not file.is_file():
            continue
        name = file.name.lower()
        if name.endswith("-setup.exe") or name.endswith(".msi"):
            manifest["platforms"]["windows-x86_64"] = {
                "url": f"http://{host}:{PORT}/{file.name}"
            }
        elif name.endswith(".dmg"):
            manifest["platforms"]["darwin-aarch64"] = {
                "url": f"http://{host}:{PORT}/{file.name}"
            }
        elif name.endswith(".app.tar.gz") or name.endswith(".app.zip"):
            manifest["platforms"]["darwin-aarch64"] = {
                "url": f"http://{host}:{PORT}/{file.name}"
            }

    text = json.dumps(manifest, indent=2, ensure_ascii=False)
    (folder / "latest.json").write_text(text, encoding="utf-8")
    print("Wrote latest.json:")
    print(text)


class Handler(SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Access-Control-Allow-Origin", "*")
        super().end_headers()

    def log_message(self, format, *args):
        print(f"[update-server] {self.address_string()} {format % args}")


def main() -> None:
    folder = Path(sys.argv[1] if len(sys.argv) > 1 else "dist").resolve()
    folder.mkdir(parents=True, exist_ok=True)

    host = os.environ.get("DESKCUSTOM_HOST", lan_ip())
    write_manifest(folder, host)

    if not any(
        f.is_file() and f.name.lower().endswith(("-setup.exe", ".msi"))
        for f in folder.iterdir()
    ):
        print()
        print("WARNING: no Windows installer (*-setup.exe) in dist/")
        print("Download the latest artifact from GitHub Actions and copy the .exe here.")
        print()

    os.chdir(folder)
    server = ThreadingHTTPServer(("0.0.0.0", PORT), Handler)
    print(f"Serving {folder}")
    print(f"Manifest: http://{host}:{PORT}/latest.json")
    print("Press Ctrl+C to stop.")
    server.serve_forever()


if __name__ == "__main__":
    main()
