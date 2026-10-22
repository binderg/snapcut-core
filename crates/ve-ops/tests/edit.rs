//! Edit commands on a small in-memory project, plus property tests over
//! random edit sequences: patches invert, linked clips stay in sync, clips
//! never play media they do not have.

use proptest::prelude::*;
use ve_model::meta::{Asset, AssetKind, AudioInfo, MediaInfo, OutputSettings, VideoInfo};
use ve_model::otio::{Composable, Timeline, TrackKind};
use ve_model::time::{Q, Rate, RationalTime};
use ve_model::Id;
use ve_ops::edit::{self, Insert, Move, RippleDelete, SetSpeed, Split, Trim};
use ve_ops::lane::{Lane, clip_id, clip_link, clip_speed, media_bounds};

const FPS: i64 = 30;

fn fr(n: i64) -> Q {
    Q::new(n, FPS)
}

fn sec(n: i64) -> Q {
    Q::from_integer(n)
}

fn project() -> Timeline {
    let output = OutputSettings {
        width: 1920,
        height: 1080,
        rate: Rate::integer(FPS),
        sample_rate: 48_000,
        color_space: "bt709".into(),
    };
    let mut tl = ve_model::doc::new_project("t", output);
    tl.metadata.ve.assets.push(Asset {
        id: Id::from_raw("asset_a"),
        url: "file:///media/talk%20take.mov".into(),
        kind: AssetKind::Source,
        hash: None,
        provenance: None,
        media: Some(MediaInfo {
            duration: RationalTime::from_frames(300 * FPS, Rate::integer(FPS)),
            video: Some(VideoInfo { codec: "h264".into(), width: 1920, height: 1080, rate: Rate::integer(FPS), rotation: 0, vfr: false, color: None }),
            audio: Some(AudioInfo { codec: "aac".into(), sample_rate: 48_000, channels: 2 }),
            size_bytes: 0,
        }),
    });
    tl
}

fn insert_all(tl: &Timeline) -> Timeline {
    let p = Insert {
        asset: Id::from_raw("asset_a"),
        track: None,
        audio_track: None,
        at: None,
        source_in: None,
        source_out: None,
        duration: None,
        video: true,
        audio: true,
        overwrite: false,
    };
    edit::insert(tl, &p).unwrap().0
}

/// (track kind, start, duration, media start, id, link) for every clip.
#[derive(Debug, Clone, PartialEq)]
struct Row {
    kind: TrackKind,
    start: Q,
    dur: Q,
    media: Q,
    id: Id,
    link: Option<Id>,
}

fn rows(tl: &Timeline) -> Vec<Row> {
    let mut out = Vec::new();
    for c in &tl.tracks.children {
        let Composable::Track(t) = c else { continue };
        for p in Lane::from_track(t).unwrap().items {
            if let Some(c) = p.clip() {
                out.push(Row {
                    kind: t.kind,
                    start: p.start,
                    dur: p.duration(),
                    media: c.source_range.unwrap().start_time.seconds(),
                    id: clip_id(c).unwrap().clone(),
                    link: clip_link(c).cloned(),
                });
            }
        }
    }
    out
}

fn ids_of(rows: &[Row], kind: TrackKind) -> Vec<Id> {
    rows.iter().filter(|r| r.kind == kind).map(|r| r.id.clone()).collect()
}

fn track_end(tl: &Timeline, kind: TrackKind) -> Q {
    rows(tl).iter().filter(|r| r.kind == kind).map(|r| r.start + r.dur).max().unwrap_or_default()
}

#[test]
fn insert_puts_linked_video_and_audio_down() {
    let tl = insert_all(&project());
    let r = rows(&tl);
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].kind, TrackKind::Video);
    assert_eq!(r[1].kind, TrackKind::Audio);
    assert_eq!(r[0].dur, sec(300));
    assert!(r[0].link.is_some() && r[0].link == r[1].link);
    let Composable::Track(v1) = &tl.tracks.children[0] else { panic!() };
    let Composable::Clip(c) = &v1.children[0] else { panic!() };
    assert_eq!(c.name, "talk take.mov");
    assert_eq!(media_bounds(c), Some((sec(0), sec(300))));

    // A second insert appends after the first by default; insert mode at 0 pushes it right.
    let p = Insert {
        asset: Id::from_raw("asset_a"),
        track: None,
        audio_track: None,
        at: Some(sec(0)),
        source_in: Some(sec(10)),
        source_out: Some(sec(15)),
        duration: None,
        video: true,
        audio: false,
        overwrite: false,
    };
    let (tl2, rep) = edit::insert(&tl, &p).unwrap();
    assert_eq!(rep.created.len(), 1);
    assert_eq!(rep.created[0].start, "00:00:00:00");
    assert_eq!(rep.created[0].end, "00:00:05:00");
    assert_eq!(track_end(&tl2, TrackKind::Video), sec(305));
    assert_eq!(track_end(&tl2, TrackKind::Audio), sec(300), "only target tracks ripple");
}

#[test]
fn split_ripple_delete_and_speed() {
    let tl = insert_all(&project());
    let v = ids_of(&rows(&tl), TrackKind::Video)[0].clone();
    let (tl, rep) = edit::split(&tl, &Split { at: sec(10), clips: vec![v.clone()], tracks: vec![], linked: true }).unwrap();
    assert_eq!(rep.created.len(), 2, "video and its audio both split");
    let r = rows(&tl);
    assert_eq!(r.len(), 4);
    assert_eq!(r[1].link, r[3].link, "right halves share a new link");
    assert_ne!(r[0].link, r[1].link);

    let (tl, rep) = edit::ripple_delete(
        &tl,
        &RippleDelete { ranges: vec![(sec(10), sec(12))], markers: None, tracks: vec![], clips: vec![], linked: true },
    )
    .unwrap();
    assert_eq!(rep.duration, "00:04:58:00");
    let r = rows(&tl);
    assert_eq!(r[1].start, sec(10));
    assert_eq!(r[1].media, sec(12), "content after the cut follows directly");

    let right = r[1].id.clone();
    let (tl, _) = edit::set_speed(&tl, &SetSpeed { clip: right.clone(), speed: Q::from_integer(2), linked: true, ripple: true }).unwrap();
    let r = rows(&tl);
    assert_eq!(r[1].dur, sec(144));
    assert_eq!(r[3].dur, sec(144));
    let Composable::Track(v1) = &tl.tracks.children[0] else { panic!() };
    let Composable::Clip(c) = &v1.children[1] else { panic!() };
    assert_eq!(c.effects[0]["OTIO_SCHEMA"], "LinearTimeWarp.1");
    assert_eq!(c.effects[0]["time_scalar"], 2.0);
    assert_eq!(clip_speed(c), Q::from_integer(2));

    // Back to 1x removes the time warp.
    let (tl, _) = edit::set_speed(&tl, &SetSpeed { clip: right, speed: Q::from_integer(1), linked: true, ripple: true }).unwrap();
    let Composable::Track(v1) = &tl.tracks.children[0] else { panic!() };
    let Composable::Clip(c) = &v1.children[1] else { panic!() };
    assert!(c.effects.is_empty());
    assert_eq!(track_end(&tl, TrackKind::Video), sec(298));
}

#[test]
fn roll_on_audio_only_makes_a_j_cut() {
    let tl = insert_all(&project());
    let v = ids_of(&rows(&tl), TrackKind::Video)[0].clone();
    let (tl, _) = edit::split(&tl, &Split { at: sec(10), clips: vec![v], tracks: vec![], linked: true }).unwrap();
    let a_left = ids_of(&rows(&tl), TrackKind::Audio)[0].clone();
    // Pull the next shot's audio 15 frames early (J cut): the left audio ends earlier.
    let (tl, rep) =
        edit::trim(&tl, &Trim { clip: a_left, in_delta: None, out_delta: Some(fr(-15)), roll: true, linked: false }).unwrap();
    assert_eq!(rep.modified.len(), 2);
    let r = rows(&tl);
    let (v, a): (Vec<&Row>, Vec<&Row>) = r.iter().partition(|r| r.kind == TrackKind::Video);
    assert_eq!(v[1].start, sec(10), "video cut stays");
    assert_eq!(a[1].start, sec(10) - fr(15), "audio cut moves");
    assert_eq!(a[1].media, sec(10) - fr(15), "the incoming audio starts earlier in its media");
    assert_eq!(track_end(&tl, TrackKind::Audio), track_end(&tl, TrackKind::Video));
}

#[test]
fn trims_past_the_media_fail() {
    let tl = insert_all(&project());
    let v = ids_of(&rows(&tl), TrackKind::Video)[0].clone();
    let err = edit::trim(&tl, &Trim { clip: v.clone(), in_delta: Some(fr(-1)), out_delta: None, roll: false, linked: true })
        .unwrap_err();
    assert!(err.0.contains("media"), "{err}");
    let err = edit::trim(&tl, &Trim { clip: v, in_delta: None, out_delta: Some(fr(1)), roll: false, linked: true }).unwrap_err();
    assert!(err.0.contains("media"), "{err}");
}

#[test]
fn move_overwrites_and_leaves_a_gap() {
    let tl = insert_all(&project());
    let v = ids_of(&rows(&tl), TrackKind::Video)[0].clone();
    let (tl, _) = edit::split(&tl, &Split { at: sec(10), clips: vec![v], tracks: vec![], linked: true }).unwrap();
    let right = ids_of(&rows(&tl), TrackKind::Video)[1].clone();
    // Move the right part 5s later: a 5s gap opens on both tracks.
    let (tl, _) = edit::move_clip(&tl, &Move { clip: right.clone(), to: sec(15), track: None, linked: true }).unwrap();
    let Composable::Track(v1) = &tl.tracks.children[0] else { panic!() };
    assert!(matches!(v1.children[1], Composable::Gap(_)));
    assert_eq!(track_end(&tl, TrackKind::Audio), sec(305));
    // Moving it back over the start overwrites the left clip's tail.
    let (tl, _) = edit::move_clip(&tl, &Move { clip: right, to: sec(5), track: None, linked: true }).unwrap();
    let r = rows(&tl);
    assert_eq!(r[0].dur, sec(5));
    assert_eq!(track_end(&tl, TrackKind::Video), sec(295));
}

#[test]
fn ripple_delete_by_marker_name_and_markers_ripple() {
    use ve_model::meta::MarkerSource;
    use ve_ops::markers::{self, NewMarker};
    let tl = insert_all(&project());
    let mk = |s: i64, d: i64, name: &str| NewMarker {
        start: sec(s),
        duration: sec(d),
        name: name.into(),
        color: None,
        comment: String::new(),
        source: MarkerSource::Analyzer,
        payload: serde_json::json!({"analyzer": "silence"}),
    };
    let (tl, _) = markers::add(&tl, vec![mk(10, 2, "silence"), mk(20, 3, "silence"), mk(30, 0, "chapter")]).unwrap();
    let (tl, rep) = edit::ripple_delete(
        &tl,
        &RippleDelete { ranges: vec![], markers: Some("silence".into()), tracks: vec![], clips: vec![], linked: true },
    )
    .unwrap();
    assert_eq!(rep.duration, "00:04:55:00");
    let names: Vec<(&str, Q)> = tl.tracks.markers.iter().map(|m| (m.name.as_str(), m.marked_range.start_time.seconds())).collect();
    assert_eq!(names, [("chapter", sec(25))]);
    assert_eq!(rows(&tl).len(), 6, "two cuts make three pieces per track");
}

#[test]
fn split_everything_under_the_playhead() {
    let tl = insert_all(&project());
    let (tl, rep) = edit::split(&tl, &Split { at: fr(45), clips: vec![], tracks: vec![], linked: true }).unwrap();
    assert_eq!(rep.created.len(), 2);
    assert_eq!(rep.created[0].start, "00:00:01:15");
    assert!(edit::split(&tl, &Split { at: sec(400), clips: vec![], tracks: vec![], linked: true }).is_err());
}

// --- property tests ---

#[derive(Debug, Clone)]
enum Op {
    Split(i64),
    Ripple(i64, i64),
    Trim { pick: usize, din: i64, dout: i64, roll: bool },
    Speed { pick: usize, num: i64, den: i64 },
    Move { pick: usize, by: i64 },
}

fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0i64..9000).prop_map(Op::Split),
        (0i64..9000, 1i64..600).prop_map(|(a, l)| Op::Ripple(a, l)),
        (0usize..8, -60i64..60, -60i64..60, any::<bool>()).prop_map(|(pick, din, dout, roll)| Op::Trim { pick, din, dout, roll }),
        (0usize..8, 1i64..5, 1i64..5).prop_map(|(pick, num, den)| Op::Speed { pick, num, den }),
        (0usize..8, -300i64..300).prop_map(|(pick, by)| Op::Move { pick, by }),
    ]
}

fn apply(tl: &Timeline, op: &Op) -> Result<Timeline, ve_ops::EditError> {
    let vids = ids_of(&rows(tl), TrackKind::Video);
    let pick = |i: usize| vids.get(i % vids.len().max(1)).cloned();
    let out = match op {
        Op::Split(f) => edit::split(tl, &Split { at: fr(*f), clips: vec![], tracks: vec![], linked: true })?,
        Op::Ripple(a, l) => edit::ripple_delete(
            tl,
            &RippleDelete { ranges: vec![(fr(*a), fr(a + l))], markers: None, tracks: vec![], clips: vec![], linked: true },
        )?,
        Op::Trim { pick: i, din, dout, roll } => {
            let Some(clip) = pick(*i) else { return Ok(tl.clone()) };
            edit::trim(tl, &Trim { clip, in_delta: Some(fr(*din)), out_delta: Some(fr(*dout)), roll: *roll, linked: true })?
        }
        Op::Speed { pick: i, num, den } => {
            let Some(clip) = pick(*i) else { return Ok(tl.clone()) };
            edit::set_speed(tl, &SetSpeed { clip, speed: Q::new(*num, *den), linked: true, ripple: true })?
        }
        Op::Move { pick: i, by } => {
            let Some(clip) = pick(*i) else { return Ok(tl.clone()) };
            let start = rows(tl).iter().find(|r| r.id == clip).unwrap().start;
            edit::move_clip(tl, &Move { clip, to: start + fr(*by), track: None, linked: true })?
        }
    };
    Ok(out.0)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn random_edits_keep_invariants(ops in prop::collection::vec(arb_op(), 1..12)) {
        let mut tl = insert_all(&project());
        for op in &ops {
            let Ok(next) = apply(&tl, op) else { continue };

            // Patches: forward turns old into new, inverse restores old.
            let p = ve_ops::commit_patches(&tl, &next).unwrap();
            prop_assert_eq!(&ve_ops::apply_patch(&tl, &p.patch).unwrap(), &next);
            prop_assert_eq!(&ve_ops::apply_patch(&next, &p.inverse).unwrap(), &tl);

            // The saved file reads back as the same document.
            let text = serde_json::to_string(&next).unwrap();
            prop_assert_eq!(&serde_json::from_str::<Timeline>(&text).unwrap(), &next);

            let r = rows(&next);
            for row in &r {
                // Every edit was linked, so each video clip has an audio twin in lockstep.
                let twins: Vec<&Row> = r.iter().filter(|o| o.link == row.link).collect();
                prop_assert_eq!(twins.len(), 2, "{:?}", r);
                prop_assert_eq!(twins[0].start, twins[1].start);
                prop_assert_eq!(twins[0].dur, twins[1].dur);
                prop_assert_eq!(twins[0].media, twins[1].media);
                // Whole project frames on the timeline.
                prop_assert!((row.start * Q::from_integer(FPS)).is_integer());
                prop_assert!((row.dur * Q::from_integer(FPS)).is_integer());
                prop_assert!(row.dur > Q::from_integer(0));
            }
            // No clip reads outside its media.
            for c in &next.tracks.children {
                let Composable::Track(t) = c else { continue };
                for p in Lane::from_track(t).unwrap().items {
                    if let Some(c) = p.clip() {
                        prop_assert!(ve_ops::lane::check_media(c).is_ok());
                    }
                }
            }
            tl = next;
        }
    }

    #[test]
    fn ripple_delete_shortens_by_the_deleted_length(a in 0i64..8000, len in 1i64..1000) {
        let tl = insert_all(&project());
        let before = track_end(&tl, TrackKind::Video);
        let (next, _) = edit::ripple_delete(&tl, &RippleDelete {
            ranges: vec![(fr(a), fr(a + len))], markers: None, tracks: vec![], clips: vec![], linked: true,
        }).unwrap();
        let cut = (fr(a + len).min(before) - fr(a)).max(Q::from_integer(0));
        prop_assert_eq!(track_end(&next, TrackKind::Video), before - cut);
        prop_assert_eq!(track_end(&next, TrackKind::Audio), before - cut);
    }
}

#[test]
fn clips_go_on_added_tracks_and_leave_with_forced_removal() {
    use ve_ops::tracks;
    let tl = project();
    let (tl, v2) = tracks::add(&tl, TrackKind::Video, None, None).unwrap();
    let (tl, a2) = tracks::add(&tl, TrackKind::Audio, None, None).unwrap();
    let p = Insert {
        asset: Id::from_raw("asset_a"),
        track: Some("V2".into()),
        audio_track: Some(a2.track.id.to_string()),
        at: Some(sec(2)),
        source_in: None,
        source_out: None,
        duration: Some(sec(3)),
        video: true,
        audio: true,
        overwrite: true,
    };
    let (with_clips, r) = edit::insert(&tl, &p).unwrap();
    assert_eq!(r.created.iter().map(|c| c.track.as_str()).collect::<Vec<_>>(), ["V2", "A2"]);
    assert!(tracks::remove(&with_clips, "V2", false).is_err());
    let (after, rep) = tracks::remove(&with_clips, &v2.track.id.to_string(), true).unwrap();
    assert_eq!(rep.removed.len(), 1);
    assert_eq!(rep.tracks.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["V1", "A1", "A2"]);
    // Undo restores the track and its clip exactly.
    let applied = ve_ops::commit_patches(&with_clips, &after).unwrap();
    assert_eq!(ve_ops::apply_patch(&after, &applied.inverse).unwrap(), with_clips);
}
