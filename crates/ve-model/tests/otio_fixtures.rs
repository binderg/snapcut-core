//! The fixtures in `fixtures/otio/` were written by Python `opentimelineio`
//! (see `scripts/make_otio_fixtures.py`). These tests prove our types read
//! them and write them back unchanged, and that a timeline built in Rust
//! serializes to the same document. `scripts/otio_roundtrip.py` checks the
//! other direction with the Python library itself.

use std::path::PathBuf;

use serde_json::{Value, json};
use ve_model::meta::{
    Asset, AssetKind, ClipMeta, EffectInstance, Keyframe, MarkerMeta, MarkerSource, OutputSettings, ProjectMeta,
    TrackMeta, TransitionMeta,
};
use ve_model::otio::{MissingReference, Transition};
use ve_model::time::Q;
use ve_model::{
    Clip, Composable, ExternalReference, Gap, Id, Marker, MediaReference, Rate, RationalTime, SCHEMA_VERSION,
    TimeRange, Timeline, Track, TrackKind,
};

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/otio").join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

fn ntsc() -> Rate {
    Rate::new(30_000, 1001)
}
const SRC: &str = "file:///C:/footage/IMG_4410.MOV";

fn rng(start: i64, dur: i64) -> TimeRange {
    TimeRange::new(RationalTime::from_frames(start, ntsc()), RationalTime::from_frames(dur, ntsc()))
}

fn id(s: &str) -> Id {
    serde_json::from_value(json!(s)).unwrap()
}

#[test]
fn fixture_round_trips_unchanged() {
    let original = fixture("talking_head.otio");
    let timeline: Timeline = serde_json::from_value(original.clone()).unwrap();
    let written = serde_json::to_value(&timeline).unwrap();
    assert_eq!(written, original);
}

#[test]
fn fixture_reads_into_typed_values() {
    let tl: Timeline = serde_json::from_value(fixture("talking_head.otio")).unwrap();

    let project = tl.metadata.ve.project.as_ref().unwrap();
    assert_eq!(project.revision, 7);
    assert_eq!(project.output.rate, Rate::new(30_000, 1001));
    assert_eq!(tl.metadata.ve.assets[0].kind, AssetKind::Source);

    let [Composable::Track(v1), Composable::Track(a1)] = tl.tracks.children.as_slice() else {
        panic!("expected two tracks");
    };
    assert_eq!(v1.kind, TrackKind::Video);
    assert_eq!(a1.kind, TrackKind::Audio);
    assert_eq!(v1.metadata.ve.track.as_ref().unwrap().id.as_str(), "trk_v1a2b3");

    let Composable::Clip(intro) = &v1.children[0] else { panic!("expected clip") };
    let meta = intro.metadata.ve.clip.as_ref().unwrap();
    assert_eq!(meta.id.as_str(), "clip_7f3a01");
    assert_eq!(meta.speed, Q::from_integer(1));
    // Exact NTSC time survives the doubles on the wire.
    assert_eq!(intro.source_range.unwrap().start_time.rate, ntsc());
    assert_eq!(intro.source_range.unwrap(), rng(312, 450));
    // Another tool's metadata is kept beside ours.
    assert_eq!(intro.metadata.other["resolve"]["clip_color"], "Orange");

    let fx = &intro.metadata.ve.effects[0];
    assert_eq!(fx.keyframes["exposure"][1].ease, "ease-out");
    assert_eq!(fx.keyframes["exposure"][0].ease, "linear");

    let marker = intro.markers[0].metadata.ve.marker.as_ref().unwrap();
    assert_eq!(marker.source, MarkerSource::Analyzer);

    let Some(MediaReference::External(ext)) = intro.active_media() else { panic!("expected external media") };
    assert_eq!(ext.target_url, SRC);

    let Composable::Clip(offline) = &a1.children[3] else { panic!("expected clip") };
    assert!(matches!(offline.active_media(), Some(MediaReference::Missing(_))));
    assert!(offline.metadata.ve.clip.is_none());
}

/// Builds the fixture's timeline from constructors. Matching the
/// Python-written file proves documents we create are canonical OTIO.
#[test]
fn timeline_built_in_rust_matches_python_output() {
    let media = || MediaReference::External(ExternalReference::new(SRC, Some(rng(0, 18_000))));
    let clip = |name: &str, cid: &str, start, dur| {
        let mut c = Clip::new(name, media(), rng(start, dur));
        c.metadata.ve.clip = Some(ClipMeta::new(id(cid)));
        c
    };
    let gap = || Composable::Gap(Gap::new(RationalTime::from_frames(15, ntsc())));

    let mut tl = Timeline::new("talking_head");
    tl.global_start_time = Some(RationalTime::zero(ntsc()));
    tl.metadata.ve.project = Some(ProjectMeta {
        schema_version: SCHEMA_VERSION,
        revision: 7,
        output: OutputSettings {
            width: 1920,
            height: 1080,
            rate: Rate::new(30_000, 1001),
            sample_rate: 48_000,
            color_space: "bt709".into(),
        },
    });
    tl.metadata.ve.assets.push(Asset {
        id: id("asset_a1b2c3"),
        url: SRC.into(),
        kind: AssetKind::Source,
        hash: Some("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262".into()),
        provenance: None,
    });

    let mut v1 = Track::new("V1", TrackKind::Video);
    v1.metadata.ve.track = Some(TrackMeta { id: id("trk_v1a2b3") });
    let mut intro = clip("intro_take3", "clip_7f3a01", 312, 450);
    intro.metadata.ve.effects.push(EffectInstance {
        id: id("fx_0200aa"),
        effect: "core.color/exposure@1".into(),
        params: json!({"exposure": 0.3}).as_object().unwrap().clone(),
        keyframes: [(
            "exposure".to_owned(),
            vec![
                Keyframe { t: RationalTime::zero(ntsc()), v: json!(0.0), ease: "linear".into() },
                Keyframe { t: RationalTime::from_frames(30, ntsc()), v: json!(0.3), ease: "ease-out".into() },
            ],
        )]
        .into(),
    });
    intro.metadata.other.insert("resolve".into(), json!({"clip_color": "Orange"}));
    let mut um = Marker::new("um", rng(432, 18));
    um.color = "YELLOW".into();
    um.comment = "filler word".into();
    um.metadata.ve.marker = Some(MarkerMeta {
        id: id("mk_9c0d1e"),
        source: MarkerSource::Analyzer,
        payload: json!({"kind": "silence", "db": -42.5}),
    });
    intro.markers.push(um);
    let mut dissolve = Transition::new(RationalTime::from_frames(6, ntsc()), RationalTime::from_frames(6, ntsc()));
    dissolve.name = "dissolve".into();
    dissolve.transition_type = "SMPTE_Dissolve".into();
    dissolve.metadata.ve.transition =
        Some(TransitionMeta { id: id("tr_5e6f70"), transition: "core.dissolve@1".into(), params: Default::default() });
    v1.children = vec![
        Composable::Clip(intro),
        gap(),
        Composable::Clip(clip("point_one", "clip_44be02", 1200, 900)),
        Composable::Transition(dissolve),
        Composable::Clip(clip("point_two", "clip_b07c03", 2400, 600)),
    ];

    let mut a1 = Track::new("A1", TrackKind::Audio);
    a1.metadata.ve.track = Some(TrackMeta { id: id("trk_a1c4d5") });
    a1.children = vec![
        Composable::Clip(clip("intro_take3", "clip_7f3a91", 312, 450)),
        gap(),
        Composable::Clip(clip("point_one", "clip_44be92", 1200, 900)),
        Composable::Clip(Clip::new(
            "broll_missing",
            MediaReference::Missing(MissingReference::default()),
            rng(0, 600),
        )),
    ];

    tl.tracks.children = vec![Composable::Track(v1), Composable::Track(a1)];
    tl.tracks.markers.push(Marker::new("chapter", rng(0, 0)));

    assert_eq!(serde_json::to_value(&tl).unwrap(), fixture("talking_head.otio"));
}

#[test]
fn summary_of_fixture() {
    let tl: Timeline = serde_json::from_value(fixture("talking_head.otio")).unwrap();
    let expected = r#""talking_head"  rev 7  1920x1080  29.97fps (30000/1001, drop-frame)  48000 Hz  bt709
duration 00:01:05;17 (1965f)  2 tracks  6 clips  2 markers
assets:
  asset_a1b2c3  source  IMG_4410.MOV
"V1" trk_v1a2b3 video  ends 00:01:05;17
  00:00:00;00    450f  clip_7f3a01 "intro_take3"  src 00:00:10;12 asset_a1b2c3  fx_0200aa core.color/exposure@1
  00:00:15;00     15f  gap
  00:00:15;15    900f  clip_44be02 "point_one"  src 00:00:40;00 asset_a1b2c3
  00:00:45;15          tr_5e6f70 transition "dissolve" core.dissolve@1 in 6f out 6f
  00:00:45;15    600f  clip_b07c03 "point_two"  src 00:01:20;02 asset_a1b2c3
"A1" trk_a1c4d5 audio  ends 00:01:05;17
  00:00:00;00    450f  clip_7f3a91 "intro_take3"  src 00:00:10;12 asset_a1b2c3
  00:00:15;00     15f  gap
  00:00:15;15    900f  clip_44be92 "point_one"  src 00:00:40;00 asset_a1b2c3
  00:00:45;15    600f  - "broll_missing"  src 00:00:00;00  MEDIA MISSING
markers:
  00:00:00;00      0f  - "chapter" red
  00:00:04;00     18f  mk_9c0d1e "um" yellow analyzer  "filler word"  on clip_7f3a01
"#;
    assert_eq!(ve_model::summary::summary(&tl), expected);
}
