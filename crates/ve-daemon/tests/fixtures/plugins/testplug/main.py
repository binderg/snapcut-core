"""Test plugin for crates/ve-daemon/tests/plugins.rs. Standard library only.

Speaks newline-delimited JSON-RPC on stdin/stdout. Requests from the engine
run on worker threads, so this loop keeps reading the engine's answers to
the plugin's own calls (and $/cancel) while one runs.
"""

import json
import os
import sys
import threading
import time

_out = threading.Lock()
_pending = {}
_pending_lock = threading.Lock()
_next = [0]
_cancelled = set()
_init = {}


def send(msg):
    with _out:
        sys.stdout.write(json.dumps(msg) + "\n")
        sys.stdout.flush()


class EngineError(Exception):
    def __init__(self, err):
        super().__init__(err.get("message"))
        self.err = err


def engine(method, params=None):
    """Call an engine method and wait for its result."""
    with _pending_lock:
        _next[0] += 1
        rid = "p%d" % _next[0]
        ev = threading.Event()
        _pending[rid] = [ev, None]
    send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params or {}})
    ev.wait()
    with _pending_lock:
        resp = _pending.pop(rid)[1]
    if "error" in resp:
        raise EngineError(resp["error"])
    return resp["result"]


def progress(call_id, fraction):
    send({"jsonrpc": "2.0", "method": "$/progress", "params": {"id": call_id, "progress": fraction}})


def handle(call_id, method, params):
    if method == "initialize":
        _init.update(params)
        return {}
    if method == "testplug.echo":
        return {"text": params.get("text"), "init": _init, "env_id": os.environ.get("VE_PLUGIN_ID")}
    if method == "testplug.transcribe":
        audio = engine("media.extract_audio", {"asset": params["asset"]})
        size = os.path.getsize(audio["path"])
        # One fake word per half second of audio: "w0", "w1", ...
        secs = float(audio["duration"].rstrip("s"))
        words = []
        t = 0.0
        i = 0
        while t + 0.4 <= secs:
            words.append({"start": "%.3fs" % t, "end": "%.3fs" % (t + 0.4), "text": "w%d" % i,
                          "speaker": "S%d" % (i // 4 % 2), "confidence": 0.9})
            t += 0.5
            i += 1
        stored = engine("transcript.set", {"asset": params["asset"], "words": words, "language": "en",
                                           "provenance": {"plugin": "testplug", "model": "fake"}})
        return {"bytes": size, "words": stored["words"], "sample_rate": audio["sample_rate"]}
    if method == "testplug.mark":
        rev = params["base_rev"]
        for name in ("one", "two"):
            r = engine("marker.add", {"at": "00:00:00:10", "name": name, "base_rev": rev})
            rev = r["revision"]
        return {"revision": rev}
    if method == "testplug.slow":
        steps = int(params.get("steps", 100))
        for i in range(steps):
            if call_id in _cancelled:
                raise EngineError({"code": -31004, "message": "cancelled"})
            progress(call_id, (i + 1) / steps)
            time.sleep(0.05)
        return {"done": True}
    if method == "testplug.stubborn":
        # Ignores $/cancel; the engine has to kill it.
        time.sleep(60)
        return {}
    if method == "testplug.crash":
        sys.stderr.write("about to crash on purpose\n")
        sys.stderr.flush()
        os._exit(3)
    raise EngineError({"code": -32601, "message": "unknown method " + method})


def run(msg):
    call_id = msg["id"]
    try:
        result = handle(call_id, msg["method"], msg.get("params") or {})
        send({"jsonrpc": "2.0", "id": call_id, "result": result})
    except EngineError as e:
        send({"jsonrpc": "2.0", "id": call_id, "error": e.err})
    except Exception as e:  # noqa: BLE001
        send({"jsonrpc": "2.0", "id": call_id, "error": {"code": -32603, "message": repr(e)}})


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        msg = json.loads(line)
        method = msg.get("method")
        if method is None:
            with _pending_lock:
                slot = _pending.get(msg.get("id"))
                if slot:
                    slot[1] = msg
                    slot[0].set()
        elif method == "$/cancel":
            _cancelled.add(msg["params"]["id"])
        elif method == "shutdown":
            break
        elif "id" in msg:
            threading.Thread(target=run, args=(msg,), daemon=True).start()
    os._exit(0)


if __name__ == "__main__":
    main()
