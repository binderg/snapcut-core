"""Writes fixtures/otio/talking_head.otio with Python opentimelineio.

The Rust tests in crates/ve-model/tests/otio_fixtures.rs compare against
this file, so regenerate it only on purpose:

    python scripts/make_otio_fixtures.py fixtures/otio/talking_head.otio
"""
import sys
import opentimelineio as otio
R = 30000 / 1001
RT = otio.opentime.RationalTime
TR = otio.opentime.TimeRange
def rng(a, n): return TR(RT(a, R), RT(n, R))
SRC = "file:///C:/footage/IMG_4410.MOV"
avail = rng(0, 18000)

tl = otio.schema.Timeline(name="talking_head")
tl.global_start_time = RT(0, R)
tl.metadata["ve"] = {
    "project": {"schema_version": "0.1.0", "revision": 7,
                "output": {"width": 1920, "height": 1080, "rate": [30000, 1001],
                           "sample_rate": 48000, "color_space": "bt709"}},
    "assets": [{"id": "asset_a1b2c3", "url": SRC, "kind": "source",
                "hash": "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"}],
}

def clip(name, cid, a, n, **extra):
    c = otio.schema.Clip(name=name, media_reference=otio.schema.ExternalReference(target_url=SRC, available_range=avail), source_range=rng(a, n))
    c.metadata["ve"] = {"clip": {"id": cid, "speed": [1, 1], "volume_db": 0.0}, **extra}
    return c

v = otio.schema.Track(name="V1", kind=otio.schema.TrackKind.Video)
v.metadata["ve"] = {"track": {"id": "trk_v1a2b3"}}
intro = clip("intro_take3", "clip_7f3a01", 312, 450, effects=[{
    "id": "fx_0200aa", "effect": "core.color/exposure@1", "params": {"exposure": 0.3},
    "keyframes": {"exposure": [{"t": RT(0, R), "v": 0.0}, {"t": RT(30, R), "v": 0.3, "ease": "ease-out"}]}}])
intro.metadata["resolve"] = {"clip_color": "Orange"}
m = otio.schema.Marker(name="um", marked_range=rng(120, 18), color="YELLOW", comment="filler word")
m.metadata["ve"] = {"marker": {"id": "mk_9c0d1e", "source": "analyzer", "payload": {"kind": "silence", "db": -42.5}}}
intro.markers.append(m)
v.append(intro)
v.append(otio.schema.Gap(source_range=rng(0, 15)))
v.append(clip("point_one", "clip_44be02", 1200, 900))
t = otio.schema.Transition(name="dissolve", transition_type="SMPTE_Dissolve", in_offset=RT(6, R), out_offset=RT(6, R))
t.metadata["ve"] = {"transition": {"id": "tr_5e6f70", "transition": "core.dissolve@1"}}
v.append(t)
v.append(clip("point_two", "clip_b07c03", 2400, 600))

a = otio.schema.Track(name="A1", kind=otio.schema.TrackKind.Audio)
a.metadata["ve"] = {"track": {"id": "trk_a1c4d5"}}
a.append(clip("intro_take3", "clip_7f3a91", 312, 450))
a.append(otio.schema.Gap(source_range=rng(0, 15)))
a.append(clip("point_one", "clip_44be92", 1200, 900))
off = otio.schema.Clip(name="broll_missing", media_reference=otio.schema.MissingReference(), source_range=rng(0, 600))
a.append(off)

tl.tracks.append(v); tl.tracks.append(a)
tl.tracks.markers.append(otio.schema.Marker(name="chapter", marked_range=rng(0, 0)))
open(sys.argv[1], "w", newline="\n").write(tl.to_json_string(indent=2) + "\n")
