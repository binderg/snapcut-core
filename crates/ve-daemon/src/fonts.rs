//! `font.*` methods: the project's own fonts, in `<project>/fonts/`, so a
//! project renders the same on any machine (installed system fonts work
//! too). Fonts come from a file, a URL, or Google Fonts by family name.
//! The renderer watches the folder (`ve_render::text::use_fonts_dir`), so
//! new fonts show up in the next frame.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_protocol::{ErrorCode, RpcError};
use ve_render::text::{FONT_EXTENSIONS, faces_in};

use crate::project::OpenProject;

pub const FOLDER: &str = "fonts";

/// Largest font file accepted.
const MAX_BYTES: u64 = 50 << 20;

/// The project's font folder; the renderer is told to use it.
pub(crate) fn dir(root: &Path) -> PathBuf {
    let d = root.join(FOLDER);
    ve_render::text::use_fonts_dir(&d);
    d
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Face {
    pub family: String,
    /// 100 to 900 (400 regular, 700 bold).
    pub weight: u16,
    pub italic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FontFile {
    /// File name in the project's `fonts/` folder.
    pub file: String,
    pub faces: Vec<Face>,
    /// The same file was already there.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub existing: bool,
}

fn faces(bytes: &[u8]) -> Vec<Face> {
    faces_in(bytes).into_iter().map(|f| Face { family: f.family, weight: f.weight, italic: f.italic }).collect()
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddParams {
    /// A font file on this machine (`.ttf`, `.otf`, `.ttc`, `.otc`).
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// A font file to download (`https://...`).
    #[serde(default)]
    pub url: Option<String>,
    /// A Google Fonts family by name (`"Inter"`, `"Bebas Neue"`): every weight and italic it has.
    #[serde(default)]
    pub google: Option<String>,
    /// Replace a file of the same name with different content (else refused).
    #[serde(default)]
    pub replace: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AddResult {
    pub files: Vec<FontFile>,
    /// Families now usable as a text style's `font`.
    pub families: Vec<String>,
}

fn invalid(m: impl Into<String>) -> RpcError {
    RpcError::invalid_params(m.into())
}

/// A file name safe to put in the fonts folder.
fn clean_name(name: &str) -> String {
    let n: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '-' }).collect();
    n.trim_matches(['-', '.']).to_owned()
}

fn download(url: &str) -> Result<Vec<u8>, RpcError> {
    let mut resp = ureq::get(url).header("User-Agent", concat!("ve/", env!("CARGO_PKG_VERSION"))).call().map_err(|e| invalid(format!("downloading {url}: {e}")))?;
    resp.body_mut().with_config().limit(MAX_BYTES).read_to_vec().map_err(|e| invalid(format!("downloading {url}: {e}")))
}

fn fetch_text(url: &str) -> Result<String, ureq::Error> {
    // A plain user agent gets TrueType files, which the renderer reads (browsers get WOFF2).
    ureq::get(url).header("User-Agent", concat!("ve/", env!("CARGO_PKG_VERSION"))).call()?.body_mut().read_to_string()
}

/// One `@font-face` from Google's CSS: weight, italic, file URL.
#[derive(Debug, PartialEq)]
struct GoogleFace {
    weight: u16,
    italic: bool,
    url: String,
}

fn parse_css(css: &str) -> Vec<GoogleFace> {
    css.split("@font-face")
        .filter_map(|block| {
            let field = |name: &str| {
                let i = block.find(&format!("{name}:"))? + name.len() + 1;
                Some(block[i..].split(';').next()?.trim().to_owned())
            };
            let url = block.split("url(").nth(1)?.split(')').next()?.trim_matches(['\'', '"']).to_owned();
            Some(GoogleFace { weight: field("font-weight")?.parse().ok()?, italic: field("font-style").is_some_and(|s| s == "italic"), url })
        })
        .collect()
}

/// Every weight and italic of a Google Fonts family, as TrueType files.
fn google(family: &str) -> Result<Vec<(String, Vec<u8>)>, RpcError> {
    let name = family.trim();
    if name.is_empty() {
        return Err(invalid("google: give a family name like \"Inter\""));
    }
    let q = name.replace(' ', "+");
    // One request per weight and style: Google refuses a list or range that
    // includes a weight the family lacks, and families differ widely.
    let mut css = String::new();
    for ital in [0, 1] {
        for w in (100..=900).step_by(100) {
            match fetch_text(&format!("https://fonts.googleapis.com/css2?family={q}:ital,wght@{ital},{w}")) {
                Ok(text) => css.push_str(&text),
                Err(ureq::Error::StatusCode(400)) => {}
                Err(e) => return Err(invalid(format!("Google Fonts: {e}"))),
            }
        }
    }
    if css.is_empty() {
        // Families with no weight axis at all answer only to the plain name.
        css = match fetch_text(&format!("https://fonts.googleapis.com/css2?family={q}")) {
            Ok(text) => text,
            Err(ureq::Error::StatusCode(400)) => {
                return Err(invalid(format!("Google Fonts has no family \"{name}\" (names are as on fonts.google.com, e.g. \"Bebas Neue\")")));
            }
            Err(e) => return Err(invalid(format!("Google Fonts: {e}"))),
        };
    }
    let mut faces = parse_css(&css);
    faces.dedup_by(|a, b| a.url == b.url);
    if faces.is_empty() {
        return Err(invalid(format!("Google Fonts sent no font files for \"{name}\"")));
    }
    let base = clean_name(&name.replace(' ', ""));
    faces
        .iter()
        .map(|f| Ok((format!("{base}-{}{}.ttf", f.weight, if f.italic { "-italic" } else { "" }), download(&f.url)?)))
        .collect()
}

pub fn add(p: &mut OpenProject, a: AddParams) -> Result<AddResult, RpcError> {
    let sources = [a.path.is_some(), a.url.is_some(), a.google.is_some()].iter().filter(|x| **x).count();
    if sources != 1 {
        return Err(invalid("give one of `path`, `url` or `google`"));
    }
    let files: Vec<(String, Vec<u8>)> = if let Some(path) = &a.path {
        let bytes = std::fs::read(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        vec![(name, bytes)]
    } else if let Some(url) = &a.url {
        let bytes = download(url)?;
        let last = url.split(['?', '#']).next().unwrap_or("").rsplit('/').next().unwrap_or("").to_owned();
        vec![(last, bytes)]
    } else {
        google(a.google.as_deref().unwrap_or_default())?
    };

    // Check every file before writing any.
    let mut checked = Vec::new();
    for (name, bytes) in files {
        let found = faces(&bytes);
        if found.is_empty() {
            return Err(invalid(format!("{name} is not a font file (TrueType or OpenType)")));
        }
        let mut file = clean_name(&name);
        let has_ext = Path::new(&file).extension().and_then(|e| e.to_str()).is_some_and(|e| FONT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
        if !has_ext {
            let f = &found[0];
            file = format!("{}-{}{}.ttf", clean_name(&f.family.replace(' ', "")), f.weight, if f.italic { "-italic" } else { "" });
        }
        checked.push((file, bytes, found));
    }
    let folder = dir(p.root());
    std::fs::create_dir_all(&folder).map_err(|e| RpcError::new(ErrorCode::Internal, format!("{}: {e}", folder.display())))?;
    let mut out = Vec::new();
    for (file, bytes, found) in &checked {
        let path = folder.join(file);
        if let Ok(old) = std::fs::read(&path) {
            if old == *bytes {
                out.push(FontFile { file: file.clone(), faces: found.clone(), existing: true });
                continue;
            }
            if !a.replace {
                return Err(invalid(format!("fonts/{file} already exists with other content; pass replace to overwrite it")));
            }
        }
        crate::project::write_file_atomic(&path, bytes).map_err(crate::rpc_error)?;
        out.push(FontFile { file: file.clone(), faces: found.clone(), existing: false });
    }
    let mut families: Vec<String> = out.iter().flat_map(|f| f.faces.iter().map(|x| x.family.clone())).collect();
    families.sort();
    families.dedup();
    ve_render::text::use_fonts_dir(&folder);
    Ok(AddResult { files: out, families })
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListParams {
    /// Also list the font families installed on this machine.
    #[serde(default)]
    pub system: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ListResult {
    /// Fonts in the project's `fonts/` folder (travel with the project).
    pub project: Vec<FontFile>,
    /// Installed families (with `system: true`). A project using these may look different elsewhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<String>>,
}

/// The fonts in a project's folder.
pub(crate) fn project_fonts(root: &Path) -> Vec<FontFile> {
    let mut out: Vec<FontFile> = std::fs::read_dir(dir(root))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            if !FONT_EXTENSIONS.contains(&ext.as_str()) {
                return None;
            }
            let bytes = std::fs::read(&path).ok()?;
            Some(FontFile { file: e.file_name().to_string_lossy().into_owned(), faces: faces(&bytes), existing: false })
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

pub fn list(p: &mut OpenProject, a: ListParams) -> ListResult {
    ListResult { project: project_fonts(p.root()), system: a.system.then(ve_render::text::system_families) }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveParams {
    /// File name in the project's `fonts/` folder (`font.list` shows them).
    pub file: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RemoveResult {
    pub file: String,
    pub removed: bool,
}

pub fn remove(p: &mut OpenProject, a: RemoveParams) -> Result<RemoveResult, RpcError> {
    if a.file.contains(['/', '\\']) || a.file.starts_with('.') {
        return Err(invalid("give a file name from `font.list`, not a path"));
    }
    let path = dir(p.root()).join(&a.file);
    let removed = match std::fs::remove_file(&path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(RpcError::new(ErrorCode::Internal, format!("{}: {e}", path.display()))),
    };
    Ok(RemoveResult { file: a.file, removed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_google_css() {
        let css = "@font-face {\n  font-family: 'Inter';\n  font-style: italic;\n  font-weight: 400;\n  src: url(https://fonts.gstatic.com/s/inter/a.ttf) format('truetype');\n}\n\
                   @font-face {\n  font-family: 'Inter';\n  font-style: normal;\n  font-weight: 700;\n  src: url(https://fonts.gstatic.com/s/inter/b.ttf) format('truetype');\n}\n";
        assert_eq!(
            parse_css(css),
            [
                GoogleFace { weight: 400, italic: true, url: "https://fonts.gstatic.com/s/inter/a.ttf".into() },
                GoogleFace { weight: 700, italic: false, url: "https://fonts.gstatic.com/s/inter/b.ttf".into() },
            ]
        );
        assert_eq!(clean_name("My Font (Bold).TTF"), "My-Font--Bold-.TTF");
    }
}
