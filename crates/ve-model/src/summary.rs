//! `project.summary`: the timeline as compact text, the cheapest rung of the
//! agent's perception ladder (ARCHITECTURE.md, "Perception for agents").
//!
//! One line per item with its timeline position, length in frames at the
//! project rate, stable ID, name, source timecode and asset, so an agent can
//! address anything it reads without fetching the full JSON. Positions use
//! drop-frame timecode at 29.97/59.94.

use std::fmt::Write;

use crate::meta::Asset;
use crate::otio::{Clip, Composable, Marker, MediaReference, Stack, Timeline, Track, TrackKind};
use crate::time::{Q, Rate, RationalTime};
use crate::timecode::to_smpte;

/// Render the summary. Never fails: odd input shows up in the text instead.
pub fn summary(tl: &Timeline) -> String {
    let project = tl.project();
    let rate = project.map(|p| p.output.rate).or_else(|| first_clip_rate(&tl.tracks)).unwrap_or(Rate::integer(24));
    let f = Fmt { rate, assets: &tl.metadata.ve.assets };
    let mut out = String::new();

    // Header.
    let _ = write!(out, "{:?}", tl.name);
    if let Some(p) = project {
        let o = &p.output;
        let _ = write!(
            out,
            "  rev {}  {}x{}  {}  {} Hz  {}",
            p.revision,
            o.width,
            o.height,
            f.rate_label(),
            o.sample_rate,
            o.color_space
        );
    } else {
        let _ = write!(out, "  (no ve project block)  {}", f.rate_label());
    }
    out.push('\n');

    let tracks: Vec<&Track> = tl
        .tracks
        .children
        .iter()
        .filter_map(|c| match c {
            Composable::Track(t) => Some(t),
            _ => None,
        })
        .collect();
    let clip_count: usize =
        tracks.iter().map(|t| t.children.iter().filter(|c| matches!(c, Composable::Clip(_))).count()).sum();
    let mut markers = Vec::new();
    collect_stack_markers(&tl.tracks, &mut markers);
    let _ = writeln!(
        out,
        "duration {} ({})  {} tracks  {} clips  {} markers",
        f.tc(stack_duration(&tl.tracks, rate)),
        f.frames(stack_duration(&tl.tracks, rate)),
        tracks.len(),
        clip_count,
        markers.len()
    );

    if !tl.metadata.ve.assets.is_empty() {
        out.push_str("assets:\n");
        for a in &tl.metadata.ve.assets {
            let _ = writeln!(out, "  {}  {}  {}", a.id, label(a.kind), file_name(&a.url));
        }
    }

    for c in &tl.tracks.children {
        match c {
            Composable::Track(t) => f.track(&mut out, t),
            other => {
                let _ = writeln!(out, "(top-level {}, not a track)", kind_name(other));
            }
        }
    }

    if !markers.is_empty() {
        markers.sort_by_key(|m| m.at);
        out.push_str("markers:\n");
        for m in &markers {
            f.marker(&mut out, m);
        }
    }
    out
}

struct Fmt<'a> {
    rate: Rate,
    assets: &'a [Asset],
}

impl Fmt<'_> {
    fn rate_label(&self) -> String {
        let q = self.rate.as_ratio();
        if q.is_integer() {
            format!("{}fps", q.numer())
        } else {
            let approx = *q.numer() as f64 / *q.denom() as f64;
            let df = if self.rate.has_drop_frame() { ", drop-frame" } else { "" };
            let approx = format!("{approx:.3}");
            format!("{}fps ({}{df})", approx.trim_end_matches('0'), self.rate)
        }
    }

    fn tc(&self, t: RationalTime) -> String {
        let t = t.rescaled_to(self.rate);
        to_smpte(t, self.rate.has_drop_frame()).unwrap_or_else(|_| self.frames(t))
    }

    fn frames(&self, d: RationalTime) -> String {
        let v = d.rescaled_to(self.rate).value;
        if v.is_integer() { format!("{}f", v.numer()) } else { format!("{}/{}f", v.numer(), v.denom()) }
    }

    fn track(&self, out: &mut String, t: &Track) {
        let id = t.metadata.ve.track.as_ref().map_or("-".to_owned(), |m| m.id.to_string());
        let kind = match t.kind {
            TrackKind::Video => "video",
            TrackKind::Audio => "audio",
        };
        let off = if t.enabled { "" } else { "  [off]" };
        let dur = track_duration(t, self.rate);
        let _ = writeln!(out, "{:?} {id} {kind}  ends {}{off}", t.name, self.tc(dur));
        let mut at = RationalTime::zero(self.rate);
        for c in &t.children {
            let len = item_duration(c, self.rate);
            let _ = write!(out, "  {}  ", self.tc(at));
            match c {
                Composable::Clip(clip) => self.clip(out, clip, len),
                Composable::Gap(_) => {
                    let _ = write!(out, "{:>6}  gap", self.frames(len));
                }
                Composable::Transition(tr) => {
                    let id = tr.metadata.ve.transition.as_ref().map_or("-".to_owned(), |m| m.id.to_string());
                    let plugin = tr.metadata.ve.transition.as_ref().map_or("", |m| m.transition.as_str());
                    let _ = write!(
                        out,
                        "{:>6}  {id} transition {:?} {plugin} in {} out {}",
                        "",
                        tr.name,
                        self.frames(tr.in_offset),
                        self.frames(tr.out_offset)
                    );
                }
                Composable::Track(_) | Composable::Stack(_) => {
                    let _ = write!(out, "{:>6}  nested {}", self.frames(len), kind_name(c));
                }
            }
            out.push('\n');
            at = at + len;
        }
    }

    fn clip(&self, out: &mut String, c: &Clip, len: RationalTime) {
        let meta = c.metadata.ve.clip.as_ref();
        let id = meta.map_or("-".to_owned(), |m| m.id.to_string());
        let _ = write!(out, "{:>6}  {id} {:?}", self.frames(len), c.name);
        if let Some(src) = c.source_range {
            let r = src.start_time.rate;
            let tc = to_smpte(src.start_time, r.has_drop_frame()).unwrap_or_else(|_| format!("{}", src.start_time.value));
            let _ = write!(out, "  src {tc}");
        }
        match c.active_media() {
            Some(MediaReference::External(e)) => match self.assets.iter().find(|a| a.url == e.target_url) {
                Some(a) => {
                    let _ = write!(out, " {}", a.id);
                }
                None => {
                    let _ = write!(out, " {}", file_name(&e.target_url));
                }
            },
            Some(MediaReference::Missing(_)) | None => out.push_str("  MEDIA MISSING"),
        }
        if let Some(m) = meta {
            if m.speed != Q::from_integer(1) {
                let _ = write!(out, "  speed {}/{}", m.speed.numer(), m.speed.denom());
            }
            if m.volume_db != 0.0 {
                let _ = write!(out, "  vol {:+}dB", m.volume_db);
            }
        }
        for fx in &c.metadata.ve.effects {
            let _ = write!(out, "  {} {}", fx.id, fx.effect);
        }
        if !c.enabled {
            out.push_str("  [off]");
        }
    }

    fn marker(&self, out: &mut String, m: &PlacedMarker<'_>) {
        let meta = m.marker.metadata.ve.marker.as_ref();
        let id = meta.map_or("-".to_owned(), |x| x.id.to_string());
        let _ = write!(
            out,
            "  {}  {:>6}  {id} {:?} {}",
            self.tc(m.at),
            self.frames(m.marker.marked_range.duration),
            m.marker.name,
            m.marker.color.to_lowercase()
        );
        if let Some(x) = meta {
            let _ = write!(out, " {}", label(x.source));
        }
        if !m.marker.comment.is_empty() {
            let _ = write!(out, "  {:?}", m.marker.comment);
        }
        if let Some(on) = &m.on {
            let _ = write!(out, "  on {on}");
        }
        out.push('\n');
    }
}

struct PlacedMarker<'a> {
    at: RationalTime,
    marker: &'a Marker,
    /// ID of the clip the marker belongs to, if any.
    on: Option<String>,
}

fn collect_stack_markers<'a>(s: &'a Stack, out: &mut Vec<PlacedMarker<'a>>) {
    // Stack and track markers are already in timeline time.
    out.extend(s.markers.iter().map(|m| PlacedMarker { at: m.marked_range.start_time, marker: m, on: None }));
    for c in &s.children {
        if let Composable::Track(t) = c {
            out.extend(t.markers.iter().map(|m| PlacedMarker { at: m.marked_range.start_time, marker: m, on: None }));
            let rate = first_clip_rate_in(t).unwrap_or(Rate::integer(24));
            let mut at = RationalTime::zero(rate);
            for item in &t.children {
                // Item markers are in the item's source time.
                let (markers, src_start, on) = match item {
                    Composable::Clip(c) => {
                        (&c.markers, c.source_range.map(|r| r.start_time), c.metadata.ve.clip.as_ref().map(|m| m.id.to_string()))
                    }
                    Composable::Gap(g) => (&g.markers, g.source_range.map(|r| r.start_time), None),
                    _ => (&EMPTY, None, None),
                };
                for m in markers {
                    let offset = src_start.map_or(m.marked_range.start_time, |s| m.marked_range.start_time - s);
                    out.push(PlacedMarker { at: at + offset, marker: m, on: on.clone() });
                }
                at = at + item_duration(item, rate);
            }
        }
    }
}

static EMPTY: Vec<Marker> = Vec::new();

fn item_duration(c: &Composable, rate: Rate) -> RationalTime {
    match c {
        Composable::Clip(c) => c.source_range.map_or(RationalTime::zero(rate), |r| r.duration),
        Composable::Gap(g) => g.source_range.map_or(RationalTime::zero(rate), |r| r.duration),
        Composable::Transition(_) => RationalTime::zero(rate),
        Composable::Track(t) => track_duration(t, rate),
        Composable::Stack(s) => stack_duration(s, rate),
    }
}

fn track_duration(t: &Track, rate: Rate) -> RationalTime {
    if let Some(r) = t.source_range {
        return r.duration;
    }
    t.children.iter().fold(RationalTime::zero(rate), |acc, c| acc + item_duration(c, rate))
}

fn stack_duration(s: &Stack, rate: Rate) -> RationalTime {
    if let Some(r) = s.source_range {
        return r.duration;
    }
    s.children.iter().map(|c| item_duration(c, rate)).max().unwrap_or(RationalTime::zero(rate))
}

fn first_clip_rate(s: &Stack) -> Option<Rate> {
    s.children.iter().find_map(|c| match c {
        Composable::Track(t) => first_clip_rate_in(t),
        Composable::Clip(c) => c.source_range.map(|r| r.duration.rate),
        _ => None,
    })
}

fn first_clip_rate_in(t: &Track) -> Option<Rate> {
    t.children.iter().find_map(|c| match c {
        Composable::Clip(c) => c.source_range.map(|r| r.duration.rate),
        _ => None,
    })
}

fn kind_name(c: &Composable) -> &'static str {
    match c {
        Composable::Clip(_) => "clip",
        Composable::Gap(_) => "gap",
        Composable::Transition(_) => "transition",
        Composable::Track(_) => "track",
        Composable::Stack(_) => "stack",
    }
}

/// The serde name of a unit enum (`source`, `analyzer`), as the JSON shows it.
fn label(v: impl serde::Serialize) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

fn file_name(url: &str) -> &str {
    url.rsplit(['/', '\\']).next().unwrap_or(url)
}
