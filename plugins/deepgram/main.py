"""Deepgram speech to text for the ve engine. Python 3.9+, standard library only.

The engine starts this process and talks newline-delimited JSON-RPC on
stdin/stdout (see skills/ve-core/guides/writing-plugins.md). Requests run on
worker threads, so the main loop keeps reading the engine's answers to our
own calls (and $/cancel) while a transcription runs.

Flow of deepgram.transcribe:
  1. media.extract_audio   (the engine decodes the asset to 16 kHz mono FLAC)
  2. POST it to Deepgram   (DEEPGRAM_API_KEY, from the OS keychain or env)
  3. transcript.set        (words with media times, speakers, confidence)
"""

import json
import os
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

API_URL = os.environ.get("DEEPGRAM_API_URL", "https://api.deepgram.com/v1/listen")
DEFAULT_MODEL = "nova-3"
PLUGIN_VERSION = "0.1.0"

_out = threading.Lock()
_pending = {}
_pending_lock = threading.Lock()
_next = [0]
_cancelled = set()


class RpcError(Exception):
    def __init__(self, message, code=-31002, data=None):
        super().__init__(message)
        self.err = {"code": code, "message": message}
        if data is not None:
            self.err["data"] = data


def send(msg):
    with _out:
        sys.stdout.write(json.dumps(msg) + "\n")
        sys.stdout.flush()


def log(message, level="info"):
    send({"jsonrpc": "2.0", "method": "$/log", "params": {"level": level, "message": message}})


def engine(method, params=None):
    """Call an engine method and wait for its result."""
    with _pending_lock:
        _next[0] += 1
        rid = "dg%d" % _next[0]
        ev = threading.Event()
        _pending[rid] = [ev, None]
    send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params or {}})
    ev.wait()
    with _pending_lock:
        resp = _pending.pop(rid)[1]
    if "error" in resp:
        e = resp["error"]
        raise RpcError(e.get("message", "engine error"), e.get("code", -31002), e.get("data"))
    return resp["result"]


def progress(call_id, fraction, message=None):
    params = {"id": call_id, "progress": fraction}
    if message:
        params["message"] = message
    send({"jsonrpc": "2.0", "method": "$/progress", "params": params})


def check_cancel(call_id):
    if call_id in _cancelled:
        raise RpcError("cancelled", -31004)


def request_url(model, language, diarize, keywords):
    q = [("model", model), ("smart_format", "true"), ("punctuate", "true")]
    if diarize:
        q.append(("diarize", "true"))
    if language:
        q.append(("language", language))
    else:
        q.append(("detect_language", "true"))
    for k in keywords or []:
        # nova-3 takes keyterm prompts; older models take keywords.
        q.append(("keyterm" if model.startswith("nova-3") else "keywords", k))
    return API_URL + "?" + urllib.parse.urlencode(q)


def post_audio(call_id, url, path, content_type, key):
    """POST the file, polling for cancel while Deepgram works."""
    with open(path, "rb") as f:
        body = f.read()
    req = urllib.request.Request(url, data=body, method="POST", headers={
        "Authorization": "Token " + key,
        "Content-Type": content_type,
        "User-Agent": "ve-deepgram-plugin/" + PLUGIN_VERSION,
    })
    box = {}

    def work():
        try:
            with urllib.request.urlopen(req, timeout=1800) as resp:
                box["body"] = resp.read()
        except urllib.error.HTTPError as e:
            detail = e.read().decode("utf-8", "replace")[:500]
            box["error"] = RpcError("Deepgram answered %d: %s" % (e.code, detail))
        except Exception as e:  # noqa: BLE001 (network errors of every kind)
            box["error"] = RpcError("could not reach Deepgram: %s" % e)

    t = threading.Thread(target=work, daemon=True)
    t.start()
    started = time.time()
    while t.is_alive():
        t.join(0.2)
        check_cancel(call_id)
        # Deepgram takes very roughly a few seconds per hour of audio, plus upload.
        elapsed = time.time() - started
        progress(call_id, min(0.2 + 0.6 * (1 - 1 / (1 + elapsed / 20)), 0.85), "waiting for Deepgram")
    if "error" in box:
        raise box["error"]
    return json.loads(box["body"])


def secs(x):
    return "%.3fs" % max(0.0, float(x))


def transcribe(call_id, params):
    asset = params["asset"]
    key = os.environ.get("DEEPGRAM_API_KEY")
    if not key:
        raise RpcError("DEEPGRAM_API_KEY is not set: run `ve plugin secret set deepgram DEEPGRAM_API_KEY`")
    model = params.get("model") or DEFAULT_MODEL
    language = params.get("language")
    diarize = params.get("diarize", True)
    keywords = params.get("keywords") or []

    progress(call_id, 0.02, "extracting audio")
    audio = engine("media.extract_audio", {"asset": asset, "format": "flac"})
    check_cancel(call_id)
    progress(call_id, 0.15, "uploading")
    url = request_url(model, language, diarize, keywords)
    result = post_audio(call_id, url, audio["path"], "audio/flac", key)
    check_cancel(call_id)
    progress(call_id, 0.9, "storing the transcript")

    channels = result.get("results", {}).get("channels", [])
    if not channels or not channels[0].get("alternatives"):
        raise RpcError("Deepgram returned no transcript", data={"response": result})
    channel = channels[0]
    alt = channel["alternatives"][0]
    words = []
    speakers = set()
    for w in alt.get("words", []):
        word = {
            "start": secs(w["start"]),
            "end": secs(w["end"]),
            "text": w.get("punctuated_word") or w.get("word", ""),
        }
        if "speaker" in w and w["speaker"] is not None:
            word["speaker"] = "S%d" % int(w["speaker"])
            speakers.add(word["speaker"])
        if "confidence" in w and w["confidence"] is not None:
            word["confidence"] = max(0.0, min(1.0, float(w["confidence"])))
        words.append(word)

    meta = result.get("metadata", {})
    model_info = next(iter(meta.get("model_info", {}).values()), {})
    detected = channel.get("detected_language")
    provenance = {
        "plugin": "deepgram",
        "plugin_version": PLUGIN_VERSION,
        "service": "deepgram",
        "model": model_info.get("name") or model,
        "model_version": model_info.get("version"),
        "request_id": meta.get("request_id"),
        "params": {"model": model, "language": language, "diarize": diarize, "keywords": keywords},
        "created": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    stored = engine("transcript.set", {
        "asset": asset,
        "words": words,
        "language": language or detected,
        "speakers": [{"id": s} for s in sorted(speakers)],
        "provenance": provenance,
    })
    lang = language or detected or "unknown language"
    return {
        "asset": asset,
        "words": stored["words"],
        "language": language or detected,
        "speakers": len(speakers),
        "text": "transcribed %s: %d words, %s, %d speaker(s). `ve transcript` shows it on the timeline."
                % (asset, stored["words"], lang, len(speakers)),
    }


def handle(call_id, method, params):
    if method == "initialize":
        return {"name": "deepgram", "version": PLUGIN_VERSION}
    if method == "deepgram.transcribe":
        return transcribe(call_id, params)
    raise RpcError("unknown method " + method, -32601)


def run(msg):
    call_id = msg["id"]
    try:
        result = handle(call_id, msg["method"], msg.get("params") or {})
        send({"jsonrpc": "2.0", "id": call_id, "result": result})
    except RpcError as e:
        send({"jsonrpc": "2.0", "id": call_id, "error": e.err})
    except Exception as e:  # noqa: BLE001
        send({"jsonrpc": "2.0", "id": call_id, "error": {"code": -31002, "message": "deepgram plugin: %r" % e}})
    finally:
        _cancelled.discard(call_id)


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except ValueError:
            continue
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
    # stdin closed or shutdown: the engine is done with us.
    os._exit(0)


if __name__ == "__main__":
    main()
