//! Files with several audio streams and odd channel layouts: probe lists
//! every stream, insert brings each in on its own linked track, and a clip's
//! stream and channel mapping decide what the mix hears. Needs ffmpeg on PATH.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ve_model::Id;
use ve_model::meta::{Asset, AssetKind, ChannelMode, Channels, OutputSettings};
use ve_model::otio::{Composable, Timeline};
use ve_model::time::{Q, Rate};
use ve_ops::edit::{self, Insert, SetAudio};
use ve_render::Span;
use ve_render::fx::Registry;
use ve_render::source::AudioSource;

const FPS: i64 = 25;
const SR: u32 = 48_000;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("ve-render-tests").join(name);
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn ffmpeg(args: &[&str]) {
    let st = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).status().expect("ffmpeg on PATH");
    assert!(st.success(), "ffmpeg {args:?}");
}

/// Three audio streams, 2 s each (lavfi sines peak at 1/8):
/// - a0: stereo, a 440 Hz tone on the left only (a lav on one side),
/// - a1: mono, a 1 kHz tone at a quarter of that level,
/// - a2: quad, a 2 kHz tone on channel 2 only.
fn source(work: &Path) -> PathBuf {
    let out = work.join("streams.mka");
    let z = format!("anullsrc=r={SR}:cl=mono:d=2");
    let graph = format!(
        "sine=f=440:r={SR}:d=2[l];{z}[r];[l][r]amerge=inputs=2[st];\
         sine=f=1000:r={SR}:d=2,volume=0.25[m];\
         {z}[z0];{z}[z1];sine=f=2000:r={SR}:d=2[t];{z}[z3];[z0][z1][t][z3]amerge=inputs=4,channelmap=map=0|1|2|3:channel_layout=quad[q]"
    );
    ffmpeg(&["-filter_complex", &graph, "-map", "[st]", "-map", "[m]", "-map", "[q]", "-c:a", "flac", out.to_str().unwrap()]);
    out
}

fn project(path: &Path) -> Timeline {
    let output = OutputSettings { width: 64, height: 64, rate: Rate::integer(FPS), sample_rate: SR, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("streams", output);
    tl.metadata.ve.assets.push(Asset {
        id: Id::from_raw("asset_rec"),
        url: ve_media::file_url(path).unwrap(),
        rel: None,
        kind: AssetKind::Source,
        hash: None,
        provenance: None,
        media: Some(ve_media::probe(path).unwrap().media),
    });
    tl
}

fn insert(streams: Option<Vec<u32>>) -> Insert {
    Insert {
        asset: Id::from_raw("asset_rec"),
        track: None,
        audio_track: Some("A1".into()),
        at: None,
        source_in: None,
        source_out: None,
        duration: None,
        video: false,
        audio: true,
        streams,
        overwrite: false,
    }
}

/// (track name, clip id, stream, link) of every clip on audio tracks.
fn audio_clips(tl: &Timeline) -> Vec<(String, Id, u32, Option<Id>)> {
    let mut out = Vec::new();
    for c in &tl.tracks.children {
        let Composable::Track(t) = c else { continue };
        for item in &t.children {
            if let Composable::Clip(clip) = item {
                let m = clip.metadata.ve.clip.as_ref().unwrap();
                out.push((t.name.clone(), m.id.clone(), m.audio.as_ref().map_or(0, |a| a.stream), m.link.clone()));
            }
        }
    }
    out
}

/// RMS in dBFS of the left and right side of the mix over 0.5 to 1.5 s.
fn sides_db(tl: &Timeline, work: &Path) -> (f64, f64) {
    let fx = Registry::for_project(None);
    let (mut l, mut r, mut n) = (0.0f64, 0.0f64, 0usize);
    let span = Span::new(Q::new(1, 2), Q::new(3, 2));
    ve_render::decode_audio(&AudioSource::Timeline { tl, span, fx: &fx }, SR, 2, work, &mut |s| {
        for f in s.chunks_exact(2) {
            l += f64::from(f[0]) * f64::from(f[0]);
            r += f64::from(f[1]) * f64::from(f[1]);
            n += 1;
        }
    })
    .unwrap();
    let db = |e: f64| 10.0 * (e / n.max(1) as f64).max(1e-12).log10();
    (db(l), db(r))
}

/// A sine at 1/8 of full scale is -21.1 dBFS RMS.
const TONE_DB: f64 = -21.07;

fn near(got: f64, want: f64, what: &str) {
    assert!((got - want).abs() < 0.6, "{what}: {got:.2} dB, expected {want:.2} dB");
}

fn silent(got: f64, what: &str) {
    assert!(got < -80.0, "{what} should be silent: {got:.2} dB");
}

#[test]
fn probe_lists_every_stream() {
    let work = dir("streams-probe");
    let m = ve_media::probe(&source(&work)).unwrap().media;
    let got: Vec<(u32, u32, Option<&str>)> = m.audio_streams.iter().map(|s| (s.index, s.channels, s.layout.as_deref())).collect();
    assert_eq!(got, [(0, 2, Some("stereo")), (1, 1, Some("mono")), (2, 4, Some("quad"))]);
    assert_eq!(m.audio.as_ref().map(|a| a.channels), Some(2), "`audio` stays the first stream");
    assert_eq!(m.audio_stream_count(), 3);
}

#[test]
fn insert_puts_each_stream_on_its_own_linked_track() {
    let work = dir("streams-insert");
    let tl = project(&source(&work));
    let (all, _) = edit::insert(&tl, &insert(None)).unwrap();
    let clips = audio_clips(&all);
    let tracks: Vec<(&str, u32)> = clips.iter().map(|c| (c.0.as_str(), c.2)).collect();
    assert_eq!(tracks, [("A1", 0), ("A2", 1), ("A3", 2)], "A2 and A3 are made for the extra streams");
    assert!(clips[0].3.is_some() && clips.iter().all(|c| c.3 == clips[0].3), "all streams share one link");

    // One chosen stream: no link, no new tracks.
    let (one, _) = edit::insert(&tl, &insert(Some(vec![2]))).unwrap();
    let clips = audio_clips(&one);
    assert_eq!(clips.len(), 1);
    assert_eq!((clips[0].0.as_str(), clips[0].2, clips[0].3.clone()), ("A1", 2, None));

    let err = edit::insert(&tl, &insert(Some(vec![3]))).unwrap_err().to_string();
    assert!(err.contains("there is no stream 3"), "{err}");
}

#[test]
fn stream_and_channels_decide_what_the_mix_hears() {
    let work = dir("streams-mix");
    let path = source(&work);
    let (tl, _) = edit::insert(&project(&path), &insert(Some(vec![0]))).unwrap();
    let clip = audio_clips(&tl)[0].1.clone();
    let set = |stream: Option<u32>, channels: Option<Channels>| {
        edit::set_audio(&tl, &SetAudio { clip: clip.clone(), stream, channels }).map(|r| r.0)
    };

    // Default: the left-only recording plays on the left only.
    let (l, r) = sides_db(&tl, &work);
    near(l, TONE_DB, "auto, left");
    silent(r, "auto, right");

    // `left` puts it on both sides; `mono` averages the two sides (-6 dB).
    let (l, r) = sides_db(&set(None, Some(Channels::Mode(ChannelMode::Left))).unwrap(), &work);
    near(l, TONE_DB, "left, left");
    near(r, TONE_DB, "left, right");
    let (l, r) = sides_db(&set(None, Some(Channels::Mode(ChannelMode::Mono))).unwrap(), &work);
    near(l, TONE_DB - 6.02, "mono, left");
    near(r, TONE_DB - 6.02, "mono, right");

    // Stream 1 (mono, a quarter of the level) on both sides.
    let (l, r) = sides_db(&set(Some(1), Some(Channels::Pick(vec![0]))).unwrap(), &work);
    near(l, TONE_DB - 12.04, "stream 1, left");
    near(r, TONE_DB - 12.04, "stream 1, right");

    // Stream 2: channel 2 has the tone, channel 3 is silent.
    let (l, r) = sides_db(&set(Some(2), Some(Channels::Pick(vec![2, 3]))).unwrap(), &work);
    near(l, TONE_DB, "quad [2, 3], left");
    silent(r, "quad [2, 3], right");

    // What a stream does not have is refused.
    let err = set(None, Some(Channels::Pick(vec![0, 2]))).unwrap_err().to_string();
    assert!(err.contains("channel 2 does not exist"), "{err}");
    let err = set(Some(1), Some(Channels::Mode(ChannelMode::Right))).unwrap_err().to_string();
    assert!(err.contains("mono"), "{err}");
    let err = set(Some(7), None).unwrap_err().to_string();
    assert!(err.contains("no stream 7"), "{err}");

    // Measuring one stream of the file.
    let mut e = (0.0f64, 0usize);
    ve_render::decode_audio(&AudioSource::File { path: &path, stream: 1 }, SR, 1, &work, &mut |s| {
        for x in s {
            e.0 += f64::from(*x) * f64::from(*x);
            e.1 += 1;
        }
    })
    .unwrap();
    near(10.0 * (e.0 / e.1 as f64).log10(), TONE_DB - 12.04, "stream 1 measured alone");

    // The layer inspector says which stream and mapping play.
    let picked = set(Some(2), Some(Channels::Pick(vec![2]))).unwrap();
    let f = ve_render::layers::at(&picked, Q::from_integer(1), &Registry::for_project(None)).unwrap();
    assert_eq!((f.audio[0].stream, f.audio[0].channels.clone()), (2, Channels::Pick(vec![2])));
}

#[test]
fn a_video_clip_with_several_streams_names_its_audio_clip() {
    let work = dir("streams-link");
    let path = work.join("av.mkv");
    let z = format!("sine=f=440:r={SR}:d=1");
    ffmpeg(&[
        "-f", "lavfi", "-i", "color=c=gray:s=64x64:r=25:d=1",
        "-filter_complex", &format!("{z}[a];{z}[b]"),
        "-map", "0:v", "-map", "[a]", "-map", "[b]",
        "-c:v", "mpeg4", "-c:a", "flac", path.to_str().unwrap(),
    ]);
    let tl = project(&path);
    let mut ins = insert(None);
    ins.video = true;
    let (tl, _) = edit::insert(&tl, &ins).unwrap();
    let video = tl
        .tracks
        .children
        .iter()
        .find_map(|c| match c {
            Composable::Track(t) if t.name == "V1" => t.children.iter().find_map(|c| match c {
                Composable::Clip(c) => ve_ops::lane::clip_id(c).cloned(),
                _ => None,
            }),
            _ => None,
        })
        .unwrap();
    let err = edit::set_audio(&tl, &SetAudio { clip: video, stream: None, channels: Some(Channels::Mode(ChannelMode::Left)) })
        .unwrap_err()
        .to_string();
    assert!(err.contains("several audio clips"), "{err}");
}

#[test]
fn channels_with_no_layout_play_the_first_two_with_a_warning() {
    let work = dir("streams-unknown");
    // Four discrete channels with no layout, as many recorders write them:
    // a tone on channel 1 only. Raw PCM read back as `4c` keeps no layout.
    let raw = work.join("rec.pcm");
    let z = format!("anullsrc=r={SR}:cl=mono:d=2");
    let graph = format!("{z}[a];sine=f=440:r={SR}:d=2[b];{z}[c];{z}[d];[a][b][c][d]amerge=inputs=4[q]");
    ffmpeg(&["-filter_complex", &graph, "-map", "[q]", "-f", "s16le", raw.to_str().unwrap()]);
    let path = work.join("rec.mka");
    ffmpeg(&["-f", "s16le", "-ch_layout", "4c", "-ar", &SR.to_string(), "-i", raw.to_str().unwrap(), "-c:a", "pcm_s16le", path.to_str().unwrap()]);
    let tl = project(&path);
    let s = tl.metadata.ve.assets[0].media.as_ref().unwrap().audio_stream(0).unwrap();
    assert_eq!((s.channels, s.layout.as_deref()), (4, None));

    let (tl, _) = edit::insert(&tl, &insert(None)).unwrap();
    let (l, r) = sides_db(&tl, &work);
    silent(l, "no layout, channel 0 on the left");
    near(r, TONE_DB, "no layout, channel 1 on the right");
    let p = ve_render::plan::plan(&tl, Span::new(Q::from_integer(0), Q::from_integer(1))).unwrap();
    assert!(p.warnings.iter().any(|w| w.contains("no standard layout")), "{:?}", p.warnings);
}
