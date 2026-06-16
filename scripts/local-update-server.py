#!/usr/bin/env python3
"""Local update server for Deskcustom — run on any PC in your LAN.

Usage:
  python3 scripts/local-update-server.py /path/to/installers

Then in Deskcustom set update URL to:
  http://YOUR_IP:8765/latest.json
"""

import json
import os
import sys
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

PORT = 8765
VERSION = os.environ.get("DESKCUSTOM_VERSION", "0.1.0")


def write_manifest(folder: Path) -> None:
    manifest = {
        "version": VERSION,
        "notes": "Local LAN update",
        "platforms": {},
    }

    for file in folder.iterdir():
        if not file.is_file():
            continue
        name = file.name.lower()
        if name.endswith("-setup.exe") or name.endswith(".msi"):
            manifest["platforms"]["windows-x86_64"] = {
                "url": f"http://{{HOST}}:{PORT}/{file.name}"
            }
        elif name.endswith(".dmg"):
            manifest["platforms"]["darwin-aarch64"] = {
                "url": f"http://{{HOST}}:{PORT}/{file.name}"
            }

    text = json.dumps(manifest, indent=2)
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
    write_manifest(folder)

    os.chdir(folder)
    host = "0.0.0.0"
    server = ThreadingHTTPServer((host, PORT), Handler)
    print(f"Serving {folder} on http://0.0.0.0:{PORT}/latest.json")
    print("Replace {HOST} in latest.json with this machine's LAN IP.")
    server.serve_forever()


if __name__ == "__main__":
    main()
