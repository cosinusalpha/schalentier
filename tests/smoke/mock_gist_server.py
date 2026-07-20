#!/usr/bin/env python3
"""Minimal in-memory mock of the GitHub Gists API for smoke tests.

Implements just enough of the API surface that src/gist.rs uses:
  - POST  /gists         -> create, returns {id, html_url, public, files}
  - GET   /gists/<id>    -> fetch, returns the stored gist
  - PATCH /gists/<id>    -> update files, returns the gist

Gists are held in memory for the life of the process. No auth is enforced (the
client sends a bearer token; we ignore it). Point schalentier at this server with
SCHALENTIER_GITHUB_API_BASE=http://127.0.0.1:<port>.

Usage: mock_gist_server.py [port]   (default port 8099)
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

GISTS = {}
_next_id = [0]


def _new_id():
    _next_id[0] += 1
    return f"mock{_next_id[0]:012d}"


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass  # keep smoke output clean

    def _send(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _read_json(self):
        length = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(length) if length else b"{}"
        return json.loads(raw or b"{}")

    def do_POST(self):
        if self.path.rstrip("/") != "/gists":
            return self._send(404, {"message": "Not Found"})
        req = self._read_json()
        gid = _new_id()
        gist = {
            "id": gid,
            "html_url": f"http://mock/{gid}",
            "public": bool(req.get("public", False)),
            "files": req.get("files", {}),
        }
        GISTS[gid] = gist
        self._send(201, gist)

    def do_GET(self):
        gid = self.path.rsplit("/", 1)[-1]
        gist = GISTS.get(gid)
        if gist is None:
            return self._send(404, {"message": "Not Found"})
        self._send(200, gist)

    def do_PATCH(self):
        gid = self.path.rsplit("/", 1)[-1]
        gist = GISTS.get(gid)
        if gist is None:
            return self._send(404, {"message": "Not Found"})
        req = self._read_json()
        # Merge/replace files by name, like the real API.
        for name, f in req.get("files", {}).items():
            gist["files"][name] = f
        if "public" in req:
            gist["public"] = bool(req["public"])
        self._send(200, gist)


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8099
    server = HTTPServer(("127.0.0.1", port), Handler)
    server.serve_forever()


if __name__ == "__main__":
    main()
