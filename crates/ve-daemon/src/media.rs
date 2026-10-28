//! `media.*` methods: probe files and keep the project's asset table.

use std::path::{Component, Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_media::MediaError;
use ve_model::Id;
use ve_model::meta::{Asset, AssetKind, MediaInfo};
use ve_protocol::{ErrorCode, RpcError};

use crate::commit::Committed;
use crate::project::OpenProject;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProbeParams {
    /// Local file. Relative paths resolve against the engine's working directory.
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProbeResult {
    pub url: String,
    /// The compact summary that `media.import` stores in the asset table.
    pub media: MediaInfo,
    /// ffprobe's own `-show_format -show_streams` JSON.
    pub ffprobe: serde_json::Value,
}

crate::commit::mutation_params! {
    /// `media.import`: probe, hash and add files to the asset table.
    pub struct ImportParams {
        /// Local files to add to the asset table.
        pub paths: Vec<PathBuf>,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImportResult {
    /// One entry per input path, in order.
    pub assets: Vec<ImportedAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImportedAsset {
    /// On a dry run, new assets show the ID they would have had; a real run assigns fresh ones.
    pub id: Id,
    pub url: String,
    /// The same content (by blake3 hash) was already in the asset table.
    pub existing: bool,
    pub media: Option<MediaInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AssetList {
    pub revision: u64,
    pub assets: Vec<Asset>,
    /// Assets whose local file is not there (moved, renamed, on an unplugged
    /// drive). Fix with `media.relink`.
    #[serde(default)]
    pub missing: Vec<Id>,
}

pub fn probe(p: ProbeParams) -> Result<ProbeResult, RpcError> {
    let probed = ve_media::probe(&p.path).map_err(media_error)?;
    let url = ve_media::file_url(&p.path).map_err(media_error)?;
    Ok(ProbeResult { url, media: probed.media, ffprobe: probed.ffprobe })
}

/// Probe and hash every file first, so one bad file fails the whole call
/// and the project never holds half an import.
pub fn import(project: &mut OpenProject, p: ImportParams) -> Result<Committed<ImportResult>, RpcError> {
    if p.paths.is_empty() {
        return Err(RpcError::invalid_params("paths is empty"));
    }
    let root = project.root().to_owned();
    crate::commit::commit(project, p.mutation(), "media.import", |tl| {
        let mut next = tl.clone();
        let mut ids = next.ids();
        let mut out = Vec::with_capacity(p.paths.len());
        for path in &p.paths {
            let media = ve_media::probe(path).map_err(media_error)?.media;
            let hash = ve_media::hash_file(path).map_err(media_error)?;
            let url = ve_media::file_url(path).map_err(media_error)?;
            let assets = &mut next.metadata.ve.assets;
            if let Some(a) = assets.iter().find(|a| a.hash.as_deref() == Some(hash.as_str())) {
                out.push(ImportedAsset { id: a.id.clone(), url: a.url.clone(), existing: true, media: a.media.clone() });
                continue;
            }
            let id = loop {
                let id = Id::new("asset");
                if ids.insert(id.clone()) {
                    break id;
                }
            };
            out.push(ImportedAsset { id: id.clone(), url: url.clone(), existing: false, media: Some(media.clone()) });
            let rel = rel_path(&root, &url);
            assets.push(Asset { id, url, kind: AssetKind::Source, hash: Some(hash), provenance: None, rel, media: Some(media) });
        }
        Ok((next, ImportResult { assets: out }))
    })
}

crate::commit::mutation_params! {
    /// `media.remove`: drop assets from the asset table.
    pub struct RemoveParams {
        /// Asset IDs to remove.
        pub assets: Vec<Id>,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RemoveResult {
    /// The removed asset IDs, in request order.
    pub removed: Vec<Id>,
}

/// All-or-nothing: an unknown ID, or an asset any clip still uses, fails the
/// whole call. The media file itself is never touched.
pub fn remove(project: &mut OpenProject, p: RemoveParams) -> Result<Committed<RemoveResult>, RpcError> {
    if p.assets.is_empty() {
        return Err(RpcError::invalid_params("assets is empty"));
    }
    crate::commit::commit(project, p.mutation(), "media.remove", |tl| {
        let mut next = tl.clone();
        for id in &p.assets {
            let Some(a) = next.metadata.ve.assets.iter().find(|a| &a.id == id) else {
                return Err(RpcError::invalid_params(format!("no asset {id}")));
            };
            let clips = next.clips_using(&a.url);
            if !clips.is_empty() {
                let list = clips.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
                return Err(RpcError::invalid_params(format!("asset {id} is used by clips {list}; delete them first")));
            }
        }
        next.metadata.ve.assets.retain(|a| !p.assets.contains(&a.id));
        Ok((next, RemoveResult { removed: p.assets.clone() }))
    })
}

pub fn assets(project: &OpenProject) -> AssetList {
    let assets = &project.timeline.metadata.ve.assets;
    AssetList { revision: project.revision(), assets: assets.clone(), missing: missing(assets) }
}

// --- relink ---

/// Assets with a local file that does not exist. Other URL schemes are not checked.
fn missing(assets: &[Asset]) -> Vec<Id> {
    assets.iter().filter(|a| ve_render::plan::local_path(&a.url).is_some_and(|p| !p.exists())).map(|a| a.id.clone()).collect()
}

/// A canonical Windows path without its `\\?\` prefix, so it compares with
/// paths read from `file://` URLs.
fn plain(p: &Path) -> PathBuf {
    match p.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with(r"UNC\") => PathBuf::from(rest),
        _ => p.to_owned(),
    }
}

/// Where the file at `url` is relative to the project folder `root`, with
/// `/` separators; `None` for other drives and non-file URLs.
pub(crate) fn rel_path(root: &Path, url: &str) -> Option<String> {
    let file = ve_render::plan::local_path(url)?;
    let root = plain(root);
    let (r, f): (Vec<Component<'_>>, Vec<Component<'_>>) = (root.components().collect(), file.components().collect());
    let common = r.iter().zip(&f).take_while(|(a, b)| a == b).count();
    // Both must share at least the drive and its root (`C:` + `\`, or `/`).
    let needed = if matches!(r.first(), Some(Component::Prefix(_))) { 2 } else { 1 };
    if common < needed {
        return None;
    }
    let mut parts: Vec<String> = vec!["..".into(); r.len() - common];
    parts.extend(f[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    Some(parts.join("/"))
}

fn from_rel(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(plain(root), |p, part| p.join(part))
}

/// Point asset `i` at `path`: its URL, every clip using the old URL, and
/// the relative hint. Returns the new URL.
fn retarget(tl: &mut ve_model::Timeline, root: &Path, i: usize, path: &Path) -> Result<String, RpcError> {
    let url = ve_media::file_url(path).map_err(media_error)?;
    let old = tl.metadata.ve.assets[i].url.clone();
    tl.retarget(&old, &url);
    let a = &mut tl.metadata.ve.assets[i];
    a.url = url.clone();
    a.rel = rel_path(root, &url);
    Ok(url)
}

/// On open: an asset whose file is gone but sits at its relative place (the
/// project folder moved with its media) is pointed there, if the size
/// matches. Relative hints of present files are brought up to date. Returns
/// (assets relinked, hints changed).
pub(crate) fn follow_moves(tl: &mut ve_model::Timeline, root: &Path) -> (usize, usize) {
    let (mut relinked, mut hints) = (0, 0);
    for i in 0..tl.metadata.ve.assets.len() {
        let a = &tl.metadata.ve.assets[i];
        let Some(path) = ve_render::plan::local_path(&a.url) else { continue };
        if path.exists() {
            let rel = rel_path(root, &a.url);
            // Assets probed before every audio stream was recorded.
            let unlisted = a.media.as_ref().is_some_and(|m| m.audio.is_some() && m.audio_streams.is_empty());
            if rel != a.rel {
                tl.metadata.ve.assets[i].rel = rel;
                hints += 1;
            }
            if unlisted
                && let Ok(p) = ve_media::probe(&path)
                && let Some(m) = tl.metadata.ve.assets[i].media.as_mut()
            {
                m.audio_streams = p.media.audio_streams;
                hints += 1;
            }
            continue;
        }
        let Some(candidate) = a.rel.as_deref().map(|r| from_rel(root, r)) else { continue };
        let size = std::fs::metadata(&candidate).ok().filter(std::fs::Metadata::is_file).map(|m| m.len());
        let fits = size.is_some() && a.media.as_ref().is_none_or(|m| Some(m.size_bytes) == size);
        if fits && retarget(tl, root, i, &candidate).is_ok() {
            relinked += 1;
        }
    }
    (relinked, hints)
}

crate::commit::mutation_params! {
    /// `media.relink`: point assets at files that moved or were renamed.
    pub struct RelinkParams {
        /// The asset to relink, with `path`. Without it, `folder` is searched for every missing asset.
        #[serde(default)]
        pub asset: Option<Id>,
        /// The asset's file in its new place.
        #[serde(default)]
        pub path: Option<PathBuf>,
        /// Search this folder and its subfolders for missing assets: a file with the same content
        /// (blake3), looked for by name and size first, then by size alone (renamed files).
        #[serde(default)]
        pub folder: Option<PathBuf>,
        /// With `asset` and `path`: use the file even if its content differs (it is probed again).
        #[serde(default)]
        pub force: bool,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RelinkResult {
    pub relinked: Vec<Relinked>,
    /// Assets still missing afterwards.
    pub missing: Vec<Id>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Relinked {
    pub asset: Id,
    pub from: String,
    pub url: String,
    /// The file has different content than the asset had (`force`); it was probed again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub changed_content: bool,
}

/// Most files a folder search looks at.
const SEARCH_LIMIT: usize = 200_000;

/// Every file under `dir` with its size, skipping hidden folders.
fn walk(dir: &Path, out: &mut Vec<(PathBuf, u64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if out.len() >= SEARCH_LIMIT {
            return;
        }
        let path = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            if !e.file_name().to_string_lossy().starts_with('.') {
                walk(&path, out);
            }
        } else if t.is_file()
            && let Ok(m) = e.metadata()
        {
            out.push((path, m.len()));
        }
    }
}

type Found = (PathBuf, u64);

/// A file in `files` with asset `a`'s content: same name and size first,
/// then any file of the same size (a rename), checked by hash.
fn find_in<'a>(files: &'a [Found], a: &Asset) -> Option<&'a Found> {
    let name = ve_render::plan::local_path(&a.url).and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    let size = a.media.as_ref().map(|m| m.size_bytes);
    let sized = |f: &&Found| size.is_none_or(|s| s == f.1);
    let named = |f: &&Found| name.as_deref().is_some_and(|n| f.0.file_name().is_some_and(|x| x.to_string_lossy().to_lowercase() == n));
    let mut candidates: Vec<&Found> = files.iter().filter(sized).filter(named).collect();
    if a.hash.is_some() && size.is_some() {
        candidates.extend(files.iter().filter(sized).filter(|f| !named(f)));
    }
    candidates.into_iter().find(|f| a.hash.as_ref().is_none_or(|h| ve_media::hash_file(&f.0).is_ok_and(|x| x == *h)))
}

pub fn relink(project: &mut OpenProject, p: RelinkParams) -> Result<Committed<RelinkResult>, RpcError> {
    let root = project.root().to_owned();
    match (&p.asset, &p.path, &p.folder) {
        (Some(_), Some(_), None) | (None, None, Some(_)) => {}
        _ => return Err(RpcError::invalid_params("pass `asset` with `path`, or `folder` to search for every missing asset")),
    }
    if p.force && p.folder.is_some() {
        return Err(RpcError::invalid_params("`force` only goes with `asset` and `path`"));
    }
    crate::commit::commit(project, p.mutation(), "media.relink", |tl| {
        let mut next = tl.clone();
        let mut done = Vec::new();
        if let (Some(id), Some(path)) = (&p.asset, &p.path) {
            let i = next.metadata.ve.assets.iter().position(|a| &a.id == id).ok_or_else(|| RpcError::invalid_params(format!("no asset {id}")))?;
            let hash = ve_media::hash_file(path).map_err(media_error)?;
            let same = next.metadata.ve.assets[i].hash.as_ref().is_none_or(|h| *h == hash);
            if !same && !p.force {
                return Err(RpcError::invalid_params(format!(
                    "{} has different content than {id} was imported from; pass force to use it anyway (it is probed again, and clips keep their in and out points)",
                    path.display()
                )));
            }
            let from = next.metadata.ve.assets[i].url.clone();
            let url = retarget(&mut next, &root, i, path)?;
            if !same {
                let media = ve_media::probe(path).map_err(media_error)?.media;
                let a = &mut next.metadata.ve.assets[i];
                a.hash = Some(hash);
                a.media = Some(media);
            }
            done.push(Relinked { asset: id.clone(), from, url, changed_content: !same });
        } else if let Some(folder) = &p.folder {
            if !folder.is_dir() {
                return Err(RpcError::invalid_params(format!("{} is not a folder", folder.display())));
            }
            let mut files = Vec::new();
            walk(folder, &mut files);
            for id in missing(&next.metadata.ve.assets) {
                let i = next.metadata.ve.assets.iter().position(|a| a.id == id).expect("listed from the same table");
                let Some((path, _)) = find_in(&files, &next.metadata.ve.assets[i]).cloned() else { continue };
                let from = next.metadata.ve.assets[i].url.clone();
                let url = retarget(&mut next, &root, i, &path)?;
                done.push(Relinked { asset: id, from, url, changed_content: false });
            }
        }
        let still = missing(&next.metadata.ve.assets);
        Ok((next, RelinkResult { relinked: done, missing: still }))
    })
}

fn media_error(e: MediaError) -> RpcError {
    let code = match &e {
        MediaError::FfprobeMissing(_) => ErrorCode::Internal,
        MediaError::Io { source, .. } if source.kind() != std::io::ErrorKind::NotFound => ErrorCode::Internal,
        _ => ErrorCode::InvalidParams,
    };
    RpcError::new(code, e.to_string())
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractAudioParams {
    /// The asset whose audio to extract, whole.
    pub asset: Id,
    /// Which audio stream (from the asset's `audio_streams`). Default 0, the first.
    #[serde(default)]
    pub stream: u32,
    /// Samples per second. Default 16000, what speech models expect.
    #[serde(default = "default_extract_rate")]
    pub sample_rate: u32,
    /// 1 (default, a mono downmix) or 2.
    #[serde(default = "default_extract_channels")]
    pub channels: u32,
    /// `wav` (16-bit PCM, default) or `flac` (lossless, about a third the size, for uploads).
    #[serde(default)]
    pub format: AudioFormat,
}

fn default_extract_rate() -> u32 {
    16_000
}

fn default_extract_channels() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    #[default]
    Wav,
    Flac,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExtractAudioResult {
    /// The file, in the engine's cache. Read it, do not move or change it.
    pub path: PathBuf,
    pub format: AudioFormat,
    pub sample_rate: u32,
    pub channels: u32,
    /// The audio stream extracted.
    pub stream: u32,
    /// The asset's duration (`"12.345s"`).
    pub duration: ve_model::transcript::Seconds,
    /// The file was already in the cache.
    pub cached: bool,
}

/// Decode an asset's audio to a plain file for a plugin (speech to text,
/// beat detection), so plugins never need their own decoder. Files are kept
/// by content hash and settings in `<project cache>/audio/`.
pub fn extract_audio(snap: &crate::Snapshot, p: ExtractAudioParams) -> Result<ExtractAudioResult, RpcError> {
    if !(8_000..=192_000).contains(&p.sample_rate) {
        return Err(RpcError::invalid_params("sample_rate must be between 8000 and 192000"));
    }
    if !(1..=2).contains(&p.channels) {
        return Err(RpcError::invalid_params("channels must be 1 or 2"));
    }
    let a = snap
        .timeline
        .metadata
        .ve
        .assets
        .iter()
        .find(|a| a.id == p.asset)
        .ok_or_else(|| RpcError::invalid_params(format!("no asset `{}`; `ve call media.assets` lists them", p.asset)))?;
    let media = a.media.as_ref().ok_or_else(|| RpcError::invalid_params(format!("{} has no probe data; re-import it", a.id)))?;
    if media.audio.is_none() {
        return Err(RpcError::invalid_params(format!("{} has no audio", a.id)));
    }
    if media.audio_stream(p.stream).is_none() {
        return Err(RpcError::invalid_params(format!(
            "{} has {} audio stream(s) (numbered from 0); there is no stream {}",
            a.id,
            media.audio_stream_count(),
            p.stream
        )));
    }
    let src = ve_render::local_path(&a.url)
        .ok_or_else(|| RpcError::invalid_params(format!("{}: only file:// media can be read ({})", a.id, a.url)))?;
    let hash = a.hash.as_deref().ok_or_else(|| RpcError::invalid_params(format!("{} has no content hash; re-import it", a.id)))?;
    let ext = match p.format {
        AudioFormat::Wav => "wav",
        AudioFormat::Flac => "flac",
    };
    let dir = snap.cache_dir().join("audio");
    // Stream 0 keeps the name files had before streams.
    let stream = if p.stream > 0 { format!("-s{}", p.stream) } else { String::new() };
    let path = dir.join(format!("{}{stream}-{}-{}.{ext}", crate::measure::cache_dir_name(hash), p.sample_rate, p.channels));
    let duration = ve_model::transcript::Seconds::new(media.duration.seconds());
    let result = |cached| ExtractAudioResult {
        path: path.clone(),
        format: p.format,
        sample_rate: p.sample_rate,
        channels: p.channels,
        stream: p.stream,
        duration,
        cached,
    };
    if path.is_file() {
        return Ok(result(true));
    }
    std::fs::create_dir_all(&dir).map_err(|e| RpcError::new(ErrorCode::Internal, format!("{}: {e}", dir.display())))?;
    let tmp = dir.join(format!("{}.{}.part.{ext}", path.file_stem().unwrap_or_default().to_string_lossy(), std::process::id()));
    let inputs = ve_render::ffmpeg::Inputs::Args(vec!["-i".into(), src.to_string_lossy().into_owned(), "-map".into(), format!("0:a:{}", p.stream), "-vn".into()]);
    let codec = match p.format {
        AudioFormat::Wav => "pcm_s16le",
        AudioFormat::Flac => "flac",
    };
    let out = vec![
        "-ac".into(),
        p.channels.to_string(),
        "-ar".into(),
        p.sample_rate.to_string(),
        "-c:a".into(),
        codec.into(),
        tmp.to_string_lossy().into_owned(),
    ];
    let run = ve_render::ffmpeg::run_inputs(&inputs, &out, &dir, false);
    if let Err(e) = run {
        let _ = std::fs::remove_file(&tmp);
        return Err(crate::render::render_error(e));
    }
    std::fs::rename(&tmp, &path).map_err(|e| RpcError::new(ErrorCode::Internal, format!("{}: {e}", path.display())))?;
    Ok(result(false))
}
