//! `media.*` methods: probe files and keep the project's asset table.

use std::collections::HashSet;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_media::MediaError;
use ve_model::Id;
use ve_model::meta::{Asset, AssetKind, MediaInfo};
use ve_protocol::{ErrorCode, RpcError};

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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportParams {
    /// Local files to add to the asset table.
    pub paths: Vec<PathBuf>,
    /// The revision this call was planned against; a mismatch fails with `stale_revision`.
    pub base_rev: u64,
    /// Report what would be imported without changing the project.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImportResult {
    /// Project revision after the call (unchanged on dry runs or when nothing was new).
    pub revision: u64,
    pub dry_run: bool,
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
pub fn import(project: &mut OpenProject, p: ImportParams) -> Result<ImportResult, RpcError> {
    let current = project.revision();
    if p.base_rev != current {
        return Err(RpcError::stale_revision(current));
    }
    if p.paths.is_empty() {
        return Err(RpcError::invalid_params("paths is empty"));
    }

    let mut next = project.timeline.clone();
    let mut ids: HashSet<Id> = next.metadata.ve.assets.iter().map(|a| a.id.clone()).collect();
    let mut out = Vec::with_capacity(p.paths.len());
    let mut added = 0;
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
        assets.push(Asset {
            id,
            url,
            kind: AssetKind::Source,
            hash: Some(hash),
            provenance: None,
            media: Some(media),
        });
        added += 1;
    }

    if added == 0 || p.dry_run {
        return Ok(ImportResult { revision: current, dry_run: p.dry_run, assets: out });
    }
    let meta = next.metadata.ve.project.as_mut().expect("open projects always have a project block");
    meta.revision += 1;
    let revision = meta.revision;
    project.commit(next).map_err(crate::rpc_error)?;
    Ok(ImportResult { revision, dry_run: false, assets: out })
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
