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
