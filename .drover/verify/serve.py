# The fixture server: the files in its folder, plus /history and /history/<id> built around the current time.
import json, math, sys, time
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

D = 86400


def history(now):
    gap = [now - 6 * D, now - 6 * D + 4 * 3600 + 1800]
    backup = {"since": now - 40 * D, "down": [
        {"down_at": now - 9 * D, "up_at": now - 9 * D + 2160, "message": "exit 1: rsync: connection reset"},
        {"down_at": now - 6 * 3600 - 1140, "up_at": None, "message": "backup.ok is 31h old"}]}
    blog = {"id": "0123456789abcdef", "group": "Web", "name": "Status page", "since": now - 40 * D, "down": [
        {"down_at": now - 12 * D, "up_at": now - 12 * D + 420, "message": "timeout after 5s"},
        {"down_at": now - 2 * D, "up_at": now - 2 * D + 1080, "message": "HTTP 502"},
        {"down_at": now - 5 * 3600, "up_at": now - 5 * 3600 + 180, "message": "timeout after 5s"}]}
    return {"version": 1, "host": "macmini", "gaps": [gap], "checks": {"backup": backup, "disk": {"since": now - 40 * D, "down": []}},
            "monitors": [blog]}


def times(now):
    last = now // 60 * 60
    t = [last - 60 * k for k in range(1439, -1, -1)]
    ms = [None if k % 487 == 300 else round(200 + 25 * math.sin(k / 90) + (60 if k % 97 == 0 else 0)) for k in range(1440)]
    h = [last // 3600 * 3600 - 3600 * k for k in range(719, -1, -1)]
    avg = [round(205 + 20 * math.sin(k / 30)) for k in range(720)]
    return {"version": 1, "id": "0123456789abcdef", "every_s": 60, "recent": {"t": t, "ms": ms},
            "hourly": {"t": h, "avg_ms": avg, "max_ms": [a + 120 + (k % 7) * 20 for k, a in enumerate(avg)]}}


class Handler(SimpleHTTPRequestHandler):
    def do_GET(self):
        body = history if self.path == "/history" else times if self.path.startswith("/history/") else None
        if body is None:
            return super().do_GET()
        data = json.dumps(body(int(time.time()))).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), partial(Handler, directory=sys.argv[2])).serve_forever()
