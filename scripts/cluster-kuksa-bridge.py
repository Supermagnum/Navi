#!/usr/bin/env python3
"""Forward Navi `navi.vss.batch.v1` TCP lines into a local file or stdout.

Wire format from Navi's Kuksa live sink (one JSON object per line)::

    {"schema":"navi.vss.batch.v1","updates":[{"path":"...","value":"..."}]}

Default listen: 127.0.0.1:55556. Point a Kuksa.val feeder / databroker client
at the printed path/value pairs, or pipe stdout into your own adapter.

This script does not speak Kuksa gRPC itself — it is the documented bridge
endpoint Navi writes to so the phone never embeds a gRPC stack.
"""

from __future__ import annotations

import argparse
import json
import socket
import sys


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=55556)
    args = p.parse_args()

    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((args.host, args.port))
    srv.listen(8)
    print(f"listening on {args.host}:{args.port}", file=sys.stderr)

    while True:
        conn, addr = srv.accept()
        print(f"client {addr}", file=sys.stderr)
        with conn, conn.makefile("r", encoding="utf-8", newline="\n") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    doc = json.loads(line)
                except json.JSONDecodeError as e:
                    print(f"bad json: {e}", file=sys.stderr)
                    continue
                for upd in doc.get("updates") or []:
                    path = upd.get("path")
                    value = upd.get("value")
                    print(f"{path}={value}")
                    sys.stdout.flush()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
