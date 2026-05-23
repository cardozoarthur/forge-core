use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct ListedArtifact {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

pub fn write_json_artifact(
    base_dir: &Path,
    relative_path: &str,
    payload: &serde_json::Value,
) -> Result<(PathBuf, String)> {
    let full_path = base_dir.join(relative_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create artifact directory {}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(payload)?;
    fs::write(&full_path, &bytes)
        .with_context(|| format!("failed to write artifact {}", full_path.display()))?;
    Ok((full_path, hex_sha256(&bytes)))
}

pub fn list_workflow_artifacts(base_dir: &Path, workflow_id: &str) -> Result<Vec<ListedArtifact>> {
    let artifact_dir = base_dir.join("artifacts").join(workflow_id);
    if !artifact_dir.exists() {
        return Ok(Vec::new());
    }

    let mut artifacts = Vec::new();
    for entry in fs::read_dir(&artifact_dir)
        .with_context(|| format!("failed to list artifacts in {}", artifact_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let bytes = fs::read(&path)?;
        let relative = path
            .strip_prefix(base_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        artifacts.push(ListedArtifact {
            path: relative,
            sha256: hex_sha256(&bytes),
            bytes: bytes.len() as u64,
        });
    }
    artifacts.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(artifacts)
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
