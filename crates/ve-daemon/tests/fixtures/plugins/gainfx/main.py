"""Plugin audio effect for crates/ve-daemon/tests/plugins.rs. Standard library only.

`gainfx.process` reads `input` (raw little-endian f32, interleaved), scales
it by `params.db`, and writes exactly `frames` frames to `output`. Every
call is logged to calls.log next to this file, so tests can count them.
"""

import array
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


def process(p):
    gain = 10 ** (float(p["params"]["db"]) / 20)
    samples = array.array("f")
    with open(p["input"], "rb") as f:
        samples.frombytes(f.read())
    if sys.byteorder != "little":
        samples.byteswap()
    want = p["frames"] * p["channels"]
    if len(samples) != want:
        raise ValueError("expected %d samples, got %d" % (want, len(samples)))
    out = array.array("f", (x * gain for x in samples))
    if sys.byteorder != "little":
        out.byteswap()
    with open(p["output"], "wb") as f:
        f.write(out.tobytes())
    with open(os.path.join(HERE, "calls.log"), "a") as log:
        log.write("%s %s %d %d\n" % (p["effect"], p["params"]["db"], p["frames"], p["start_sample"]))
    return {}


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        msg = json.loads(line)
        method = msg.get("method")
        if method == "shutdown":
            break
        if "id" not in msg or method is None:
            continue
        try:
            if method == "initialize":
                result = {}
            elif method == "gainfx.process":
                result = process(msg.get("params") or {})
            else:
                raise ValueError("unknown method " + method)
            send({"jsonrpc": "2.0", "id": msg["id"], "result": result})
        except Exception as e:  # noqa: BLE001
            send({"jsonrpc": "2.0", "id": msg["id"], "error": {"code": -32603, "message": repr(e)}})
    os._exit(0)


if __name__ == "__main__":
    main()
