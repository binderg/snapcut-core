//! `media.*` methods: probe files and keep the project's asset table.

use std::path::PathBuf;

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
            assets.push(Asset { id, url, kind: AssetKind::Source, hash: Some(hash), provenance: None, media: Some(media) });
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
    AssetList { revision: project.revision(), assets: project.timeline.metadata.ve.assets.clone() }
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
    /// The asset whose first audio stream to extract, whole.
    pub asset: Id,
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
    let src = ve_render::local_path(&a.url)
        .ok_or_else(|| RpcError::invalid_params(format!("{}: only file:// media can be read ({})", a.id, a.url)))?;
    let hash = a.hash.as_deref().ok_or_else(|| RpcError::invalid_params(format!("{} has no content hash; re-import it", a.id)))?;
    let ext = match p.format {
        AudioFormat::Wav => "wav",
        AudioFormat::Flac => "flac",
    };
    let dir = snap.cache_dir().join("audio");
    let path = dir.join(format!("{}-{}-{}.{ext}", crate::measure::cache_dir_name(hash), p.sample_rate, p.channels));
    let duration = ve_model::transcript::Seconds::new(media.duration.seconds());
    let result = |cached| ExtractAudioResult { path: path.clone(), format: p.format, sample_rate: p.sample_rate, channels: p.channels, duration, cached };
    if path.is_file() {
        return Ok(result(true));
    }
    std::fs::create_dir_all(&dir).map_err(|e| RpcError::new(ErrorCode::Internal, format!("{}: {e}", dir.display())))?;
    let tmp = dir.join(format!("{}.{}.part.{ext}", path.file_stem().unwrap_or_default().to_string_lossy(), std::process::id()));
    let inputs = ve_render::ffmpeg::Inputs::Args(vec!["-i".into(), src.to_string_lossy().into_owned(), "-map".into(), "0:a:0".into(), "-vn".into()]);
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
