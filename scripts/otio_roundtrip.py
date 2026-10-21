"""Checks that Python opentimelineio reads our .otio files and writes them back
unchanged, which proves they are valid OTIO and that no field is lost.

    pip install opentimelineio
    python scripts/otio_roundtrip.py fixtures/otio

Exits non-zero and prints the first difference on failure.
"""

import json
import pathlib
import sys

import opentimelineio as otio


def first_diff(a, b, path="$"):
    if isinstance(a, dict) and isinstance(b, dict):
        for k in sorted(set(a) | set(b)):
            if k not in a or k not in b:
                return f"{path}.{k}: only in {'original' if k in a else 'otio output'}"
            d = first_diff(a[k], b[k], f"{path}.{k}")
            if d:
                return d
        return None
    if isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            return f"{path}: length {len(a)} != {len(b)}"
        for i, (x, y) in enumerate(zip(a, b)):
            d = first_diff(x, y, f"{path}[{i}]")
            if d:
                return d
        return None
    return None if a == b else f"{path}: {a!r} != {b!r}"


def main(root):
    files = sorted(pathlib.Path(root).rglob("*.otio"))
    if not files:
        sys.exit(f"no .otio files under {root}")
    failed = 0
    for f in files:
        original = json.loads(f.read_text(encoding="utf-8"))
        rewritten = json.loads(otio.adapters.read_from_file(str(f)).to_json_string())
        diff = first_diff(original, rewritten)
        print(f"{'FAIL' if diff else 'ok  '} {f}" + (f"\n     {diff}" if diff else ""))
        failed += bool(diff)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "fixtures/otio")
