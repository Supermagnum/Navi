#!/usr/bin/env python3
"""Local pass-through proxy for the navi-server origin, used by the Phase 7 device test of
"a new CoMaps release is published while a migration runs".

  phase7-origin-proxy.py [PORT]          (default 18080; the device reaches it with `adb reverse`)

Everything is forwarded to https://navigate-me.duckdns.org unchanged. After the file
/tmp/phase7-flip exists, GET /mwm/current.json answers with the real catalog whose top-level
"version" is replaced by 261106 (nothing else is touched, the leaves of 261105 stay downloadable).
The log line `FLIP` is written when the first flipped catalog is served.
"""
import http.server
import json
import os
import shutil
import ssl
import sys
import urllib.error
import urllib.request

UPSTREAM = "https://navigate-me.duckdns.org"
FLIP_FILE = "/tmp/phase7-flip"
FLIP_VERSION = "261106"
flipped_logged = False


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        sys.stderr.write("%s %s\n" % (self.address_string(), fmt % args))

    def do_GET(self):
        global flipped_logged
        req = urllib.request.Request(UPSTREAM + self.path, headers={k: v for k, v in self.headers.items() if k.lower() not in ("host", "connection")})
        try:
            resp = urllib.request.urlopen(req, timeout=120, context=ssl.create_default_context())
        except urllib.error.HTTPError as e:
            self.send_response(e.code)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        body = None
        if self.path.split("?")[0] == "/mwm/current.json" and os.path.exists(FLIP_FILE):
            doc = json.loads(resp.read())
            doc["version"] = FLIP_VERSION
            body = json.dumps(doc).encode()
            if not flipped_logged:
                sys.stderr.write("FLIP: serving catalog version %s\n" % FLIP_VERSION)
                flipped_logged = True
        self.send_response(resp.status)
        for k, v in resp.getheaders():
            if k.lower() in ("transfer-encoding", "connection", "content-length"):
                continue
            self.send_header(k, v)
        if body is not None:
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            data = resp.read()
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 18080
    http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
