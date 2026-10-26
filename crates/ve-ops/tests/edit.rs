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
            video: Some(VideoInfo { codec: "h264".into(), width: 1920, height: 1080, rate: Rate::integer(FPS), rotation: 0, vfr: false, still: false, color: None, alpha: None, see_through_percent: None }),
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
    // The pushed video clip is linked, so its audio moves with it.
    assert_eq!(track_end(&tl2, TrackKind::Audio), sec(305), "linked audio is pushed too");
}

fn insert_video(at: Q, len: i64) -> Insert {
    Insert {
        asset: Id::from_raw("asset_a"),
        track: None,
        audio_track: None,
        at: Some(at),
        source_in: Some(sec(0)),
        source_out: None,
        duration: Some(sec(len)),
        video: true,
        audio: false,
        overwrite: false,
    }
}

#[test]
fn insert_inside_a_linked_clip_cuts_and_pushes_both_sides() {
    let tl = insert_all(&project());
    let (tl, _) = edit::insert(&tl, &insert_video(sec(100), 5)).unwrap();
    let r = rows(&tl);
    let v: Vec<&Row> = r.iter().filter(|x| x.kind == TrackKind::Video).collect();
    let a: Vec<&Row> = r.iter().filter(|x| x.kind == TrackKind::Audio).collect();
    assert_eq!(v.len(), 3);
    assert_eq!(a.len(), 2, "the audio is cut at the insert point too");
    assert_eq!((a[0].start, a[0].dur), (sec(0), sec(100)));
    assert_eq!((a[1].start, a[1].dur), (sec(105), sec(200)));
    // Left halves linked together, right halves linked together, the new clip alone.
    assert_eq!(v[0].link, a[0].link);
    assert_eq!(v[2].link, a[1].link);
    assert_ne!(v[0].link, v[2].link);
    assert_eq!(v[2].start, a[1].start);
    assert_eq!(v[2].media, a[1].media);
    assert_eq!(v[1].link, None);
}

#[test]
fn insert_pushes_linked_audio_from_its_own_start() {
    // Two takes, then make an L cut: the first take's audio runs 10 frames into the second shot.
    let tl = insert_all(&project());
    let tl = edit::split(&tl, &Split { at: sec(100), clips: vec![], tracks: vec![], linked: true }).unwrap().0;
    let a0 = rows(&tl).into_iter().find(|x| x.kind == TrackKind::Audio).unwrap().id;
    let tl = edit::trim(&tl, &Trim { clip: a0, in_delta: None, out_delta: Some(fr(10)), roll: true, linked: false }).unwrap().0;
    let (tl, _) = edit::insert(&tl, &insert_video(sec(100), 5)).unwrap();
    let r = rows(&tl);
    let a: Vec<&Row> = r.iter().filter(|x| x.kind == TrackKind::Audio).collect();
    let v: Vec<&Row> = r.iter().filter(|x| x.kind == TrackKind::Video).collect();
    // The outgoing audio is not under the insert point on its own track, so it stays whole;
    // the second shot's audio keeps its 10-frame offset from its picture.
    assert_eq!((a[0].start, a[0].dur), (sec(0), sec(100) + fr(10)));
    assert_eq!(v[2].start, sec(105));
    assert_eq!(a[1].start, sec(105) + fr(10));
}

#[test]
fn insert_leaves_unlinked_tracks_alone() {
    // Video-only clip on V1, music-style audio-only clip on A1: nothing links them.
    let tl = project();
    let tl = edit::insert(&tl, &insert_video(sec(0), 20)).unwrap().0;
    let mut p = insert_video(sec(0), 20);
    p.video = false;
    p.audio = true;
    let tl = edit::insert(&tl, &p).unwrap().0;
    let (tl, _) = edit::insert(&tl, &insert_video(sec(10), 5)).unwrap();
    assert_eq!(track_end(&tl, TrackKind::Video), sec(25));
    assert_eq!(track_end(&tl, TrackKind::Audio), sec(20));
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
    let (tl, _) = edit::move_clip(&tl, &Move { clip: right.clone(), to: sec(15), track: None, linked: true, push: false, reorder: false }).unwrap();
    let Composable::Track(v1) = &tl.tracks.children[0] else { panic!() };
    assert!(matches!(v1.children[1], Composable::Gap(_)));
    assert_eq!(track_end(&tl, TrackKind::Audio), sec(305));
    // Moving it back over the start overwrites the left clip's tail.
    let (tl, _) = edit::move_clip(&tl, &Move { clip: right, to: sec(5), track: None, linked: true, push: false, reorder: false }).unwrap();
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

/// Three 10 s A/V shots back to back: (video id, audio id) per shot.
fn three_shots() -> (Timeline, Vec<(Id, Id)>) {
    let mut tl = project();
    for i in 0..3 {
        let mut p = insert_video(sec(10 * i), 10);
        p.audio = true;
        p.source_in = Some(sec(10 * i));
        tl = edit::insert(&tl, &p).unwrap().0;
    }
    let r = rows(&tl);
    let shots = (0..3)
        .map(|i| {
            let v = r.iter().find(|x| x.kind == TrackKind::Video && x.start == sec(10 * i)).unwrap();
            let a = r.iter().find(|x| x.kind == TrackKind::Audio && x.link == v.link).unwrap();
            (v.id.clone(), a.id.clone())
        })
        .collect();
    (tl, shots)
}

fn start_of(tl: &Timeline, id: &Id) -> Q {
    rows(tl).into_iter().find(|r| &r.id == id).unwrap().start
}

#[test]
fn push_move_puts_a_clip_before_another_and_moves_the_rest() {
    let (tl, s) = three_shots();
    // Shot 3 in front of shot 2: shot 2 and its sound move right by 10 s; shot 3's old place is a gap.
    let mv = Move { clip: s[2].0.clone(), to: sec(10), track: None, linked: true, push: true, reorder: false };
    let (tl, _) = edit::move_clip(&tl, &mv).unwrap();
    assert_eq!(start_of(&tl, &s[2].0), sec(10));
    assert_eq!(start_of(&tl, &s[2].1), sec(10));
    assert_eq!(start_of(&tl, &s[1].0), sec(20));
    assert_eq!(start_of(&tl, &s[1].1), sec(20));
    assert_eq!(start_of(&tl, &s[0].0), sec(0));
    assert_eq!(rows(&tl).len(), 6, "nothing was cut or overwritten");
}

#[test]
fn push_move_only_pushes_as_far_as_needed() {
    let (tl, s) = three_shots();
    // Lift shot 2 so there is a 10 s hole, then nudge shot 1 five seconds into it: nothing needs to move.
    let tl = edit::delete(&tl, &edit::Delete { clips: vec![s[1].0.clone()], linked: true }).unwrap().0;
    let (tl, _) = edit::move_clip(&tl, &Move { clip: s[0].0.clone(), to: sec(5), track: None, linked: true, push: true, reorder: false }).unwrap();
    assert_eq!(start_of(&tl, &s[2].0), sec(20));
    // Twelve seconds in overlaps shot 3 by 2 s, so it (and its sound) moves 2 s.
    let (tl, _) = edit::move_clip(&tl, &Move { clip: s[0].0.clone(), to: sec(12), track: None, linked: true, push: true, reorder: false }).unwrap();
    assert_eq!(start_of(&tl, &s[2].0), sec(22));
    assert_eq!(start_of(&tl, &s[2].1), sec(22));
}

#[test]
fn push_move_of_video_only_drags_linked_audio_along() {
    let (tl, s) = three_shots();
    // A video-only B-roll clip at the end, moved in front of shot 2.
    let tl = edit::insert(&tl, &insert_video(sec(30), 4)).unwrap().0;
    let broll = rows(&tl).into_iter().find(|r| r.link.is_none()).unwrap().id;
    let (tl, _) = edit::move_clip(&tl, &Move { clip: broll.clone(), to: sec(10), track: None, linked: true, push: true, reorder: false }).unwrap();
    assert_eq!(start_of(&tl, &broll), sec(10));
    for (v, a) in &s[1..] {
        assert_eq!(start_of(&tl, v), start_of(&tl, a));
    }
    assert_eq!(start_of(&tl, &s[1].0), sec(14));
}

#[test]
fn reorder_swaps_neighbours_both_ways() {
    let (tl, s) = three_shots();
    let mv = |clip: &Id, to: Q| Move { clip: clip.clone(), to, track: None, linked: true, push: false, reorder: true };
    // Shot 2 left past shot 1: they trade places, shot 3 stays.
    let (a, _) = edit::move_clip(&tl, &mv(&s[1].0, sec(0))).unwrap();
    assert_eq!([start_of(&a, &s[1].0), start_of(&a, &s[0].0), start_of(&a, &s[2].0)], [sec(0), sec(10), sec(20)]);
    assert_eq!([start_of(&a, &s[1].1), start_of(&a, &s[0].1)], [sec(0), sec(10)]);
    // Shot 1 right past shot 2: same result, no hole left behind.
    let (b, _) = edit::move_clip(&tl, &mv(&s[0].0, sec(10))).unwrap();
    assert_eq!(rows(&a), rows(&b));
    // Shot 1 to the end, past both.
    let (c, _) = edit::move_clip(&tl, &mv(&s[0].0, sec(20))).unwrap();
    assert_eq!([start_of(&c, &s[1].0), start_of(&c, &s[2].0), start_of(&c, &s[0].0)], [sec(0), sec(10), sec(20)]);
    assert_eq!(start_of(&c, &s[0].1), sec(20));
}

#[test]
fn reorder_moves_video_only_broll_with_linked_shots_in_sync() {
    let (tl, s) = three_shots();
    // B-roll (no sound) inserted between shots 1 and 2 leaves a hole on A1 under it.
    let tl = edit::insert(&tl, &insert_video(sec(10), 4)).unwrap().0;
    let broll = rows(&tl).into_iter().find(|r| r.link.is_none()).unwrap().id;
    let (tl, _) = edit::move_clip(&tl, &Move { clip: broll.clone(), to: sec(0), track: None, linked: true, push: false, reorder: true }).unwrap();
    assert_eq!(start_of(&tl, &broll), sec(0));
    for (v, a) in &s {
        assert_eq!(start_of(&tl, v), start_of(&tl, a));
    }
    assert_eq!(start_of(&tl, &s[0].0), sec(4));
    assert_eq!(start_of(&tl, &s[1].0), sec(14));
}

#[test]
fn reorder_refuses_to_pull_links_apart() {
    let (tl, s) = three_shots();
    // Lengthen shot 1's audio 10 frames into shot 2 (L cut); swapping shot 2 past shot 1 now cuts through it.
    let tl = edit::trim(&tl, &Trim { clip: s[0].1.clone(), in_delta: None, out_delta: Some(fr(10)), roll: true, linked: false }).unwrap().0;
    let err = edit::move_clip(&tl, &Move { clip: s[2].0.clone(), to: sec(10), track: None, linked: true, push: false, reorder: true });
    assert!(err.is_err());
}

/// Three A/V shots with a 4 s video-only B-roll clip between shots 1 and 2
/// (A1 has a 4 s hole under it).
fn shots_with_broll() -> (Timeline, Vec<(Id, Id)>, Id) {
    let (tl, s) = three_shots();
    let tl = edit::insert(&tl, &insert_video(sec(10), 4)).unwrap().0;
    let broll = rows(&tl).into_iter().find(|r| r.link.is_none()).unwrap().id;
    (tl, s, broll)
}

fn assert_pairs_in_sync(tl: &Timeline, s: &[(Id, Id)]) {
    for (v, a) in s {
        assert_eq!(start_of(tl, v), start_of(tl, a), "{v} and {a} out of sync");
    }
}

#[test]
fn ripple_trim_of_unlinked_clip_moves_later_pairs_on_both_tracks() {
    let (tl, s, broll) = shots_with_broll();
    let trim = |tl: &Timeline, out: Q| {
        edit::trim(tl, &Trim { clip: broll.clone(), in_delta: None, out_delta: Some(out), roll: false, linked: false })
    };
    // Longer: shots 2 and 3 move right 2 s with their sound.
    let (longer, _) = trim(&tl, sec(2)).unwrap();
    assert_pairs_in_sync(&longer, &s);
    assert_eq!(start_of(&longer, &s[1].1), sec(16));
    // Shorter: the hole under the B-roll on A1 gives the sound room to close up.
    let (shorter, _) = trim(&tl, sec(-2)).unwrap();
    assert_pairs_in_sync(&shorter, &s);
    assert_eq!(start_of(&shorter, &s[2].1), sec(22));
}

#[test]
fn speed_and_ripple_delete_on_one_track_keep_pairs_in_sync() {
    let (tl, s, broll) = shots_with_broll();
    let (fast, _) = edit::set_speed(&tl, &SetSpeed { clip: broll.clone(), speed: sec(2), linked: false, ripple: true }).unwrap();
    assert_pairs_in_sync(&fast, &s);
    assert_eq!(start_of(&fast, &s[1].0), sec(12));
    let del = RippleDelete { ranges: vec![(sec(10), sec(14))], markers: None, tracks: vec!["V1".into()], clips: vec![], linked: true };
    let (cut, _) = edit::ripple_delete(&tl, &del).unwrap();
    assert_pairs_in_sync(&cut, &s);
    assert_eq!(start_of(&cut, &s[1].0), sec(10));
}

#[test]
fn ripple_on_one_side_is_refused_when_the_partner_track_has_no_room() {
    let (tl, s) = three_shots();
    // Shorten shot 1's picture only: shot 2's sound would have to slide under shot 1's sound.
    let err = edit::trim(&tl, &Trim { clip: s[0].0.clone(), in_delta: None, out_delta: Some(sec(-2)), roll: false, linked: false })
        .unwrap_err();
    assert!(err.0.contains("no room"), "{err}");
    // Trimming both sides together is fine: everything after closes up on both tracks.
    let (tl, _) =
        edit::trim(&tl, &Trim { clip: s[0].0.clone(), in_delta: None, out_delta: Some(sec(-2)), roll: false, linked: true }).unwrap();
    assert_pairs_in_sync(&tl, &s[1..]);
}

// --- property tests ---

#[derive(Debug, Clone)]
enum Op {
    Split(i64),
    Ripple(i64, i64),
    Trim { pick: usize, din: i64, dout: i64, roll: bool },
    Speed { pick: usize, num: i64, den: i64 },
    Move { pick: usize, by: i64, push: bool },
    Reorder { pick: usize, to: usize },
    Insert { at: i64, len: i64, audio: bool },
}

fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0i64..9000).prop_map(Op::Split),
        (0i64..9000, 1i64..600).prop_map(|(a, l)| Op::Ripple(a, l)),
        (0usize..8, -60i64..60, -60i64..60, any::<bool>()).prop_map(|(pick, din, dout, roll)| Op::Trim { pick, din, dout, roll }),
        (0usize..8, 1i64..5, 1i64..5).prop_map(|(pick, num, den)| Op::Speed { pick, num, den }),
        (0usize..8, -300i64..300, any::<bool>()).prop_map(|(pick, by, push)| Op::Move { pick, by, push }),
        // Only A/V inserts here: the checks below expect every clip to have a twin.
        // `unlinked_ripples_keep_links_in_sync` covers video-only clips.
        (0i64..9000, 1i64..300).prop_map(|(at, len)| Op::Insert { at, len, audio: true }),
        (0usize..8, 0usize..8).prop_map(|(pick, to)| Op::Reorder { pick, to }),
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
        Op::Move { pick: i, by, push } => {
            let Some(clip) = pick(*i) else { return Ok(tl.clone()) };
            let start = rows(tl).iter().find(|r| r.id == clip).unwrap().start;
            edit::move_clip(tl, &Move { clip, to: start + fr(*by), track: None, linked: true, push: *push, reorder: false })?
        }
        Op::Reorder { pick: i, to } => {
            // Land at another video clip's start (moving left) or end (moving right), as the UI does.
            let Some(clip) = pick(*i) else { return Ok(tl.clone()) };
            let r = rows(tl);
            let me = r.iter().find(|x| x.id == clip).unwrap().clone();
            let Some(other) = pick(*to).and_then(|o| r.iter().find(|x| x.id == o).cloned()) else { return Ok(tl.clone()) };
            let dest = if other.start < me.start { other.start } else { other.start + other.dur - me.dur };
            edit::move_clip(tl, &Move { clip, to: dest, track: None, linked: true, push: false, reorder: true })?
        }
        Op::Insert { at, len, audio } => {
            let mut p = insert_video(fr(*at), 0);
            p.duration = Some(fr(*len));
            p.audio = *audio;
            edit::insert(tl, &p)?
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
    fn inserts_keep_linked_clips_in_sync(ops in prop::collection::vec((0i64..9000, 1i64..300, any::<bool>()), 1..10)) {
        let mut tl = insert_all(&project());
        for (at, len, audio) in ops {
            let mut p = insert_video(fr(at), 0);
            p.duration = Some(fr(len));
            p.audio = audio;
            let Ok((next, _)) = edit::insert(&tl, &p) else { continue };
            let r = rows(&next);
            for row in r.iter().filter(|r| r.link.is_some()) {
                let twins: Vec<&Row> = r.iter().filter(|o| o.link == row.link).collect();
                prop_assert_eq!(twins.len(), 2, "{:?}", r);
                prop_assert_eq!(twins[0].start, twins[1].start);
                prop_assert_eq!(twins[0].media, twins[1].media);
            }
            tl = next;
        }
    }

    /// Video-only clips, then ripples on one track (unlinked trims and speed
    /// changes, ripple deletes on V1): every link keeps its members' offsets,
    /// or the edit is refused.
    #[test]
    fn unlinked_ripples_keep_links_in_sync(ops in prop::collection::vec((0u8..4, 0i64..3000, -90i64..90, 1usize..8), 1..12)) {
        let (mut tl, _) = three_shots();
        for (kind, a, b, pick) in ops {
            let vids = ids_of(&rows(&tl), TrackKind::Video);
            let clip = vids[pick % vids.len()].clone();
            let next = match kind {
                0 => edit::insert(&tl, &{ let mut p = insert_video(fr(a % 900), 0); p.duration = Some(fr(b.abs() + 1)); p }),
                1 => edit::trim(&tl, &Trim { clip, in_delta: (b % 2 == 0).then(|| fr(b / 3)), out_delta: Some(fr(b)), roll: false, linked: false }),
                2 => edit::set_speed(&tl, &SetSpeed { clip, speed: Q::new(pick as i64, 1 + (a % 4)), linked: false, ripple: true }),
                _ => edit::ripple_delete(&tl, &RippleDelete {
                    ranges: vec![(fr(a % 900), fr(a % 900 + b.abs() + 1))], markers: None, tracks: vec!["V1".into()], clips: vec![], linked: true,
                }),
            };
            let Ok((next, _)) = next else { continue };
            let before = rows(&tl);
            let after = rows(&next);
            let start = |r: &[Row], id: &Id| r.iter().find(|x| &x.id == id).map(|x| x.start);
            for x in after.iter().filter(|x| x.link.is_some()) {
                for y in after.iter().filter(|y| y.link == x.link && y.id != x.id) {
                    if let (Some(bx), Some(by)) = (start(&before, &x.id), start(&before, &y.id)) {
                        prop_assert_eq!(x.start - y.start, bx - by, "{} and {} drifted: {:?}", x.id, y.id, after);
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

fn video_clips(tl: &Timeline) -> Vec<Id> {
    let Composable::Track(v1) = &tl.tracks.children[0] else { panic!() };
    v1.children
        .iter()
        .filter_map(|c| match c {
            Composable::Clip(c) => clip_id(c).cloned(),
            _ => None,
        })
        .collect()
}

#[test]
fn effects_stack_on_video_clips_and_undo() {
    use serde_json::json;
    use ve_ops::effects;
    let tl = insert_all(&project());
    let v = video_clips(&tl)[0].clone();
    let params = |v: serde_json::Value| v.as_object().cloned().unwrap();
    let (tl1, r) = effects::add(&tl, &v, "core.color/desaturate@1", params(json!({"amount": 0.5})), None).unwrap();
    let fx = r.effect.clone();
    let (tl2, r) = effects::add(&tl1, &v, "core.look/blur@1", params(json!({})), Some(0)).unwrap();
    assert_eq!(r.effects.iter().map(|e| e.effect.as_str()).collect::<Vec<_>>(), ["core.look/blur@1", "core.color/desaturate@1"]);
    let blur = r.effects[0].id.clone();
    assert!(tl2.ids().contains(&fx) && tl2.ids().contains(&blur));
    // Merge params; null removes one.
    let (tl3, r) = effects::set(&tl2, &fx, Some(&params(json!({"amount": null}))), None, Some(0)).unwrap();
    assert!(r.effects[0].params.is_empty());
    assert_eq!(r.effects[0].id, fx, "moved to the front");
    // Keyframes, sorted by time.
    let rate = tl.output_rate();
    let mut keys = indexmap::IndexMap::new();
    keys.insert("amount".to_owned(), vec![effects::keyframe(sec(2), json!(1.0), None, rate), effects::keyframe(sec(0), json!(0.0), None, rate)]);
    let (tl4, r) = effects::set(&tl3, &fx, None, Some(&keys), None).unwrap();
    assert_eq!(r.effects[0].keyframed, ["amount"]);
    let stored = effects::get(&tl4, &fx).unwrap();
    assert!(stored.keyframes["amount"][0].t < stored.keyframes["amount"][1].t);
    // Remove, and undo restores everything.
    let (tl5, _) = effects::remove(&tl4, &blur).unwrap();
    assert!(effects::get(&tl5, &blur).is_none());
    let applied = ve_ops::commit_patches(&tl, &tl5).unwrap();
    assert_eq!(ve_ops::apply_patch(&tl5, &applied.inverse).unwrap(), tl);
    // Audio clips are refused.
    let Composable::Track(a1) = &tl.tracks.children[1] else { panic!() };
    let Composable::Clip(ac) = &a1.children[0] else { panic!() };
    assert!(effects::add(&tl, clip_id(ac).unwrap(), "core.color/desaturate", params(json!({})), None).is_err());
}

#[test]
fn transitions_sit_on_cuts_and_need_handles() {
    use ve_ops::effects::{self, Align, NewTransition};
    let tl = insert_all(&project());
    // Cut at 5 s: both sides have media to spare.
    let tl = edit::split(&tl, &Split { at: sec(5), clips: vec![], tracks: vec![], linked: true }).unwrap().0;
    let new = |at: Q, align: Align| NewTransition {
        clip: None,
        at: Some(at),
        track: None,
        duration: sec(1),
        align,
        transition: "core.transition/dissolve@1".into(),
        params: serde_json::Map::new(),
    };
    let (with, r) = effects::add_transition(&tl, &new(sec(5), Align::Center)).unwrap();
    assert_eq!((r.start.as_str(), r.cut.as_str(), r.end.as_str()), ("00:00:04:15", "00:00:05:00", "00:00:05:15"));
    assert_eq!(r.track, "V1");
    assert!(effects::add_transition(&with, &new(sec(5), Align::Center)).is_err(), "one per cut");
    // Lengthen it and make it start at the cut.
    let (with2, r) = effects::set_transition(&with, &r.id, Some(sec(2)), Some(Align::Start), None, None).unwrap();
    assert_eq!((r.start.as_str(), r.end.as_str()), ("00:00:05:00", "00:00:07:00"));
    let (gone, _) = effects::remove_transition(&with2, &r.id).unwrap();
    assert_eq!(gone, tl);
    // Not a cut, and audio is not for transitions.
    assert!(effects::add_transition(&tl, &new(sec(3), Align::Center)).is_err());
    // A second copy of the file inside the first.
    let mut p = Insert {
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
    p.at = Some(sec(10));
    let tl = edit::insert(&tl, &p).unwrap().0;
    // The fixture file is 300 s. The copy inserted at 10 s starts at media 0
    // (no pre-roll) and ends at the file's end, at 310 s (no media after).
    let e = effects::add_transition(&tl, &new(sec(10), Align::Center)).unwrap_err();
    assert!(e.0.contains("before its in point"), "{e}");
    assert!(effects::add_transition(&tl, &new(sec(10), Align::Start)).is_ok(), "starting at the cut needs no pre-roll");
    let e = effects::add_transition(&tl, &new(sec(310), Align::Start)).unwrap_err();
    assert!(e.0.contains("after its out point"), "{e}");
    assert!(effects::add_transition(&tl, &new(sec(310), Align::End)).is_ok(), "ending at the cut needs no media after it");
}

#[test]
fn still_images_get_a_default_length_and_no_media_limit() {
    use ve_model::meta::{Asset, AssetKind, MediaInfo, VideoInfo};
    let mut tl = project();
    tl.metadata.ve.assets.push(Asset {
        id: Id::from_raw("asset_logo"),
        url: "file:///media/logo.png".into(),
        kind: AssetKind::Source,
        hash: None,
        provenance: None,
        media: Some(MediaInfo {
            duration: RationalTime::from_frames(0, Rate::integer(25)),
            video: Some(VideoInfo { codec: "png".into(), width: 400, height: 300, rate: Rate::integer(25), rotation: 0, vfr: false, still: true, color: None, alpha: None, see_through_percent: None }),
            audio: None,
            size_bytes: 0,
        }),
    });
    let mut p = insert_video(sec(2), 0);
    p.asset = Id::from_raw("asset_logo");
    p.duration = None;
    p.source_in = None;
    let (tl, rep) = edit::insert(&tl, &p).unwrap();
    assert_eq!((rep.created[0].start.as_str(), rep.created[0].end.as_str()), ("00:00:02:00", "00:00:07:00"), "5 s by default");
    assert_eq!(rows(&tl).len(), 1, "no audio clip for an image");
    let id = rows(&tl)[0].id.clone();
    // Any length: a still never runs out of media.
    let (tl, _) = edit::trim(&tl, &Trim { clip: id.clone(), in_delta: None, out_delta: Some(sec(600)), roll: false, linked: true }).unwrap();
    assert_eq!(rows(&tl)[0].dur, sec(605));
    // Timed at the project rate, not the 25 fps ffprobe reports for images.
    let mut p2 = insert_video(sec(0), 0);
    p2.asset = Id::from_raw("asset_logo");
    p2.duration = Some(fr(7));
    let (tl, _) = edit::insert(&tl, &p2).unwrap();
    assert_eq!(rows(&tl).iter().find(|r| r.id != id).unwrap().dur, fr(7));
}
