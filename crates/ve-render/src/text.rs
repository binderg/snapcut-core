//! Text clips to pictures: lay out with `cosmic-text` (system fonts,
//! shaping, wrapping, right-to-left), rasterize glyphs with swash, then
//! build background box, shadow, outline and fill on the CPU. The result is
//! a straight-alpha sRGB RGBA picture the compositor draws like a decoded
//! frame ([`crate::compose`]).
//!
//! The picture is the text box (layout box plus padding) with the same
//! margin on every side for outline, shadow and ink that leaves the box, so
//! its center is the text box's center and alignment never shifts it.
//!
//! Fonts: the system's, plus every font file in the folders given to
//! [`use_fonts_dir`] (a project's `fonts/`). Those folders are rescanned at
//! most once a second, so fonts added or replaced while the engine runs
//! are picked up; [`fonts_signature`] changes with them, and frame keys
//! include it so cached frames never show an old font.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use cosmic_text::fontdb;

use cosmic_text::{Align as CtAlign, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight};
use ve_model::text::{Align, TextClip, rgba};

use crate::RenderError;

/// A rendered text clip.
#[derive(Debug)]
pub struct TextImage {
    /// RGBA8, straight alpha, sRGB.
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

struct Fonts {
    system: FontSystem,
    swash: SwashCache,
}

/// Fonts load once per process (reading every installed font takes a moment).
fn fonts() -> &'static Mutex<Fonts> {
    static F: OnceLock<Mutex<Fonts>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(Fonts { system: FontSystem::new(), swash: SwashCache::new() }))
}

/// Font folders added with [`use_fonts_dir`] and what was loaded from them.
#[derive(Default)]
struct Dirs {
    dirs: Vec<PathBuf>,
    /// File -> (size, modified time, faces loaded from it).
    loaded: HashMap<PathBuf, (u64, Option<SystemTime>, Vec<fontdb::ID>)>,
    scanned: Option<Instant>,
    signature: String,
}

fn dirs() -> &'static Mutex<Dirs> {
    static D: OnceLock<Mutex<Dirs>> = OnceLock::new();
    D.get_or_init(Default::default)
}

/// How often font folders are looked at again.
const RESCAN: Duration = Duration::from_secs(1);

/// File extensions read as fonts.
pub const FONT_EXTENSIONS: [&str; 4] = ["ttf", "otf", "ttc", "otc"];

fn is_font_file(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| FONT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Use the fonts in `dir` (and keep watching it) besides the system's.
pub fn use_fonts_dir(dir: &Path) {
    if let Ok(mut d) = dirs().lock() {
        if !d.dirs.iter().any(|x| x == dir) {
            d.dirs.push(dir.to_owned());
        }
        d.scanned = None;
    }
}

/// Load new or changed font files from the folders, drop removed ones.
fn refresh(system: &mut FontSystem) {
    let Ok(mut d) = dirs().lock() else { return };
    if d.scanned.is_some_and(|t| t.elapsed() < RESCAN) {
        return;
    }
    d.scanned = Some(Instant::now());
    let mut seen: Vec<(PathBuf, u64, Option<SystemTime>)> = Vec::new();
    for dir in d.dirs.clone() {
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = e.path();
            if !is_font_file(&path) {
                continue;
            }
            if let Ok(m) = e.metadata() {
                seen.push((path, m.len(), m.modified().ok()));
            }
        }
    }
    seen.sort();
    let changed: Vec<PathBuf> = d
        .loaded
        .iter()
        .filter(|(p, (len, time, _))| !seen.iter().any(|(q, l, t)| q == *p && l == len && t == time))
        .map(|(p, _)| p.clone())
        .collect();
    for p in changed {
        if let Some((_, _, ids)) = d.loaded.remove(&p) {
            for id in ids {
                system.db_mut().remove_face(id);
            }
        }
    }
    for (path, len, time) in &seen {
        if d.loaded.contains_key(path) {
            continue;
        }
        let ids = system.db_mut().load_font_source(fontdb::Source::File(path.clone()));
        d.loaded.insert(path.clone(), (*len, *time, ids.to_vec()));
    }
    d.signature = seen.iter().map(|(p, l, t)| format!("{}:{l}:{t:?}", p.display())).collect::<Vec<_>>().join("|");
}

/// Changes whenever a font folder's files change ([`use_fonts_dir`]).
pub fn fonts_signature() -> String {
    if let Ok(mut f) = fonts().lock() {
        refresh(&mut f.system);
    }
    dirs().lock().map(|d| blake3::hash(d.signature.as_bytes()).to_hex()[..16].to_owned()).unwrap_or_default()
}

/// One face in a font file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    pub family: String,
    /// 100 to 900.
    pub weight: u16,
    pub italic: bool,
}

/// The faces in font file bytes; empty when they are not a font.
pub fn faces_in(bytes: &[u8]) -> Vec<Face> {
    let mut db = fontdb::Database::new();
    db.load_font_data(bytes.to_vec());
    db.faces()
        .map(|f| Face {
            family: f.families.first().map(|(n, _)| n.clone()).unwrap_or_default(),
            weight: f.weight.0,
            italic: f.style != fontdb::Style::Normal,
        })
        .collect()
}

/// Every font family installed on the system, sorted.
pub fn system_families() -> Vec<String> {
    let Ok(f) = fonts().lock() else { return Vec::new() };
    let mut out: Vec<String> = f
        .system
        .db()
        .faces()
        .filter(|face| matches!(face.source, fontdb::Source::File(ref p) | fontdb::Source::SharedFile(ref p, _) if !dirs().lock().is_ok_and(|d| d.loaded.contains_key(p))))
        .filter_map(|face| face.families.first().map(|(n, _)| n.clone()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Rendered text by everything that decides its pixels. Small: text
/// changes rarely within a render, and each entry is one picture.
fn cache() -> &'static Mutex<HashMap<String, Arc<TextImage>>> {
    static C: OnceLock<Mutex<HashMap<String, Arc<TextImage>>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

const CACHE_ENTRIES: usize = 64;

/// `t` drawn for a `canvas_w x canvas_h` canvas, `k` times larger (so a
/// clip scaled up by its transform stays sharp; it is drawn `1/k` size).
pub fn render(t: &TextClip, canvas_w: u32, canvas_h: u32, k: f32) -> Result<Arc<TextImage>, RenderError> {
    let key = format!("{} {canvas_w}x{canvas_h} k{k} fonts {}", serde_json::to_string(t).unwrap_or_default(), fonts_signature());
    if let Some(hit) = cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Ok(hit);
    }
    t.style.check().map_err(|e| RenderError::Invalid(format!("text clip: {e}")))?;
    let img = Arc::new(draw(t, canvas_w as f32 * k, canvas_h as f32 * k)?);
    if let Ok(mut c) = cache().lock() {
        if c.len() >= CACHE_ENTRIES {
            c.clear();
        }
        c.insert(key, Arc::clone(&img));
    }
    Ok(img)
}

/// Coverage of the glyphs (0 to 1) on a `w x h` grid.
struct Mask {
    w: usize,
    h: usize,
    a: Vec<f32>,
}

impl Mask {
    fn new(w: usize, h: usize) -> Self {
        Self { w, h, a: vec![0.0; w * h] }
    }

    fn get(&self, x: isize, y: isize) -> f32 {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h { 0.0 } else { self.a[y as usize * self.w + x as usize] }
    }

    /// Grown by `r` pixels with a round brush (the outline's shape).
    fn dilate(&self, r: f32) -> Mask {
        if r <= 0.0 {
            return Mask { w: self.w, h: self.h, a: self.a.clone() };
        }
        let ri = r.ceil() as isize;
        // Offsets inside the circle, with soft coverage on its rim.
        let mut brush = Vec::new();
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                let cover = (r + 0.5 - d).clamp(0.0, 1.0);
                if cover > 0.0 {
                    brush.push((dx, dy, cover));
                }
            }
        }
        let mut out = Mask::new(self.w, self.h);
        for y in 0..self.h as isize {
            for x in 0..self.w as isize {
                let mut m: f32 = 0.0;
                for &(dx, dy, cover) in &brush {
                    let v = self.get(x + dx, y + dy) * cover;
                    if v > m {
                        m = v;
                        if m >= 1.0 {
                            break;
                        }
                    }
                }
                out.a[y as usize * self.w + x as usize] = m;
            }
        }
        out
    }
}

fn draw(t: &TextClip, cw: f32, ch: f32) -> Result<TextImage, RenderError> {
    let s = &t.style;
    let px = (s.size as f32 * ch).max(1.0);
    let wrap = (s.width as f32 * cw).max(px);
    let mut guard = fonts().lock().map_err(|_| RenderError::Invalid("text: font system poisoned".into()))?;
    let Fonts { system, swash } = &mut *guard;
    refresh(system);
    if system.db().is_empty() {
        return Err(RenderError::Invalid("text clips need fonts, and this system has none installed".into()));
    }
    let family = match s.font.trim() {
        "sans-serif" | "sans" => Family::SansSerif,
        "serif" => Family::Serif,
        "monospace" | "mono" => Family::Monospace,
        name => Family::Name(name),
    };
    let attrs = Attrs::new()
        .family(family)
        .weight(Weight(s.weight))
        .style(if s.italic { Style::Italic } else { Style::Normal });
    let align = match s.align {
        Align::Left => CtAlign::Left,
        Align::Center => CtAlign::Center,
        Align::Right => CtAlign::Right,
    };
    let mut buf = Buffer::new(system, Metrics::new(px, px * s.line_height as f32));
    buf.set_size(Some(wrap), None);
    buf.set_text(&t.text, &attrs, Shaping::Advanced, Some(align));
    buf.shape_until_scroll(system, false);

    // The layout box: the widest line (where alignment put it) and all lines.
    let (mut x0, mut x1, mut height) = (f32::MAX, f32::MIN, 0.0f32);
    for run in buf.layout_runs() {
        let (l, r) = match s.align {
            Align::Left => (0.0, run.line_w),
            Align::Center => ((wrap - run.line_w) / 2.0, (wrap + run.line_w) / 2.0),
            Align::Right => (wrap - run.line_w, wrap),
        };
        x0 = x0.min(l);
        x1 = x1.max(r);
        height = height.max(run.line_top + run.line_height);
    }
    if x1 <= x0 {
        // Empty text: one blank line, so the clip still has a size.
        (x0, x1, height) = (wrap / 2.0, wrap / 2.0, px * s.line_height as f32);
    }

    let pad = s.padding as f32 * px;
    let outline = s.outline as f32 * px;
    let shadow = s.shadow as f32 * px;
    // Room for ink past the line box (accents, descenders, italics), outline and shadow.
    let margin = (px * 0.35 + outline + shadow).ceil();
    let box_w = (x1 - x0) + 2.0 * pad;
    let box_h = height + 2.0 * pad;
    let w = (box_w + 2.0 * margin).ceil().max(1.0) as usize;
    let h = (box_h + 2.0 * margin).ceil().max(1.0) as usize;
    // Where layout point (0, 0) lands in the picture.
    let (ox, oy) = (margin + pad - x0, margin + pad);

    let mut mask = Mask::new(w, h);
    buf.draw(system, swash, Color::rgba(255, 255, 255, 255), |gx, gy, gw, gh, c| {
        let cover = f32::from(c.a()) / 255.0;
        if cover <= 0.0 {
            return;
        }
        for yy in 0..gh as i32 {
            for xx in 0..gw as i32 {
                let (x, y) = ((gx + xx) as f32 + ox, (gy + yy) as f32 + oy);
                if x >= 0.0 && y >= 0.0 && (x as usize) < w && (y as usize) < h {
                    let i = y as usize * w + x as usize;
                    mask.a[i] = mask.a[i].max(cover);
                }
            }
        }
    });
    drop(guard);

    // Straight-alpha layers, bottom to top: box, shadow, outline, fill.
    let mut out = vec![[0.0f32; 4]; w * h];
    let bg = rgba(&s.background);
    if bg[3] > 0.0 {
        let (bx0, by0) = (margin, margin);
        let (bx1, by1) = (margin + box_w, margin + box_h);
        for y in 0..h {
            for x in 0..w {
                // Soft edges: how much of the pixel the box covers.
                let cx = ((x as f32 + 1.0).min(bx1) - (x as f32).max(bx0)).clamp(0.0, 1.0);
                let cy = ((y as f32 + 1.0).min(by1) - (y as f32).max(by0)).clamp(0.0, 1.0);
                over(&mut out[y * w + x], bg, cx * cy);
            }
        }
    }
    let shape = mask.dilate(outline);
    let sc = rgba(&s.shadow_color);
    if shadow > 0.0 && sc[3] > 0.0 {
        let (dx, dy) = (shadow.round() as isize, shadow.round() as isize);
        for y in 0..h {
            for x in 0..w {
                let a = shape.get(x as isize - dx, y as isize - dy);
                over(&mut out[y * w + x], sc, a);
            }
        }
    }
    if outline > 0.0 {
        let oc = rgba(&s.outline_color);
        for (o, a) in out.iter_mut().zip(&shape.a) {
            over(o, oc, *a);
        }
    }
    let fill = rgba(&s.color);
    for (o, a) in out.iter_mut().zip(&mask.a) {
        over(o, fill, *a);
    }
    let rgba8 = out.iter().flat_map(|p| [byte(p[0]), byte(p[1]), byte(p[2]), byte(p[3])]).collect();
    Ok(TextImage { rgba: rgba8, width: w as u32, height: h as u32 })
}

/// Lay `color` (straight alpha, times `cover`) over `dst` (straight alpha).
fn over(dst: &mut [f32; 4], color: [f64; 4], cover: f32) {
    let a = color[3] as f32 * cover;
    if a <= 0.0 {
        return;
    }
    let out_a = a + dst[3] * (1.0 - a);
    for i in 0..3 {
        dst[i] = (color[i] as f32 * a + dst[i] * dst[3] * (1.0 - a)) / out_a;
    }
    dst[3] = out_a;
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use ve_model::text::TextStyle;

    fn clip(text: &str, style: TextStyle) -> TextClip {
        TextClip { text: text.into(), style }
    }

    fn alpha(img: &TextImage, x: u32, y: u32) -> u8 {
        img.rgba[((y * img.width + x) * 4 + 3) as usize]
    }

    #[test]
    fn draws_white_text_on_clear() {
        let img = render(&clip("Hello", TextStyle::default()), 640, 360, 1.0).unwrap();
        // About 7% of 360 tall, and wider than tall.
        assert!(img.width > img.height && img.height > 25 && img.height < 120, "{}x{}", img.width, img.height);
        assert_eq!(alpha(&img, 0, 0), 0, "corners are clear");
        let inked = img.rgba.chunks(4).filter(|p| p[3] > 200).count();
        assert!(inked > 100, "letters are drawn: {inked} solid pixels");
        assert!(img.rgba.chunks(4).filter(|p| p[3] > 200).all(|p| p[0] > 200 && p[1] > 200 && p[2] > 200), "white");
    }

    #[test]
    fn background_outline_and_wrapping() {
        let boxed = TextStyle { background: vec![0.0, 0.0, 1.0, 1.0], ..TextStyle::default() };
        let img = render(&clip("Box", boxed), 640, 360, 1.0).unwrap();
        // The box fills the middle of the picture's edge rows inside the margin.
        let mid_y = img.height / 2;
        let blue_px = (0..img.width).filter(|&x| {
            let i = ((mid_y * img.width + x) * 4) as usize;
            img.rgba[i + 2] > 200 && img.rgba[i + 3] > 200
        });
        assert!(blue_px.count() > 10, "the box is drawn");

        let plain = render(&clip("O", TextStyle::default()), 640, 360, 1.0).unwrap();
        let outlined = render(&clip("O", TextStyle { outline: 0.15, ..TextStyle::default() }), 640, 360, 1.0).unwrap();
        let dark = outlined.rgba.chunks(4).filter(|p| p[3] > 200 && p[0] < 60).count();
        assert!(dark > 50, "a black outline: {dark}");
        assert!(outlined.width > plain.width, "the margin grows with the outline");

        let long = "word ".repeat(30);
        let one = render(&clip("word", TextStyle::default()), 640, 360, 1.0).unwrap();
        let wrapped = render(&clip(&long, TextStyle::default()), 640, 360, 1.0).unwrap();
        assert!(wrapped.width as f32 <= 640.0 * 0.8 + 100.0, "wraps at the width: {}", wrapped.width);
        assert!(wrapped.height > one.height * 2, "onto several lines");
    }

    #[test]
    fn scale_draws_larger_and_bad_styles_fail() {
        let a = render(&clip("Hi", TextStyle::default()), 640, 360, 1.0).unwrap();
        let b = render(&clip("Hi", TextStyle::default()), 640, 360, 2.0).unwrap();
        assert!((b.height as f32 / a.height as f32 - 2.0).abs() < 0.2, "{} vs {}", a.height, b.height);
        assert!(render(&clip("x", TextStyle { size: 0.0, ..TextStyle::default() }), 640, 360, 1.0).is_err());
    }
}
