//! Versioned artifact storage for bitcode, profiles, LLVM assembly (.ll), binaries, and rollback.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Versioned record of optimization build artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactVersion {
    pub version_id: String,
    pub created_at: String,
    pub bitcode_path: Option<PathBuf>,
    pub ll_path: Option<PathBuf>,
    pub profile_path: Option<PathBuf>,
    pub binary_path: Option<PathBuf>,
    pub metadata: HashMap<String, String>,
}

/// Disk-backed versioned store for compiler artifacts and binaries.
#[derive(Debug, Clone)]
pub struct ArtifactStore {
    root_dir: PathBuf,
}

impl ArtifactStore {
    pub fn new(root_dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let root_dir = root_dir.as_ref().to_path_buf();
        fs::create_dir_all(&root_dir)?;
        Ok(Self { root_dir })
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// Save a new version of compiler artifacts into a version directory.
    pub fn store_version(
        &self,
        version_id: &str,
        bitcode: Option<&[u8]>,
        ll: Option<&str>,
        profile: Option<&[u8]>,
        binary: Option<&[u8]>,
        metadata: HashMap<String, String>,
    ) -> std::io::Result<ArtifactVersion> {
        let version_dir = self.root_dir.join(version_id);
        fs::create_dir_all(&version_dir)?;

        let mut bitcode_path = None;
        let mut ll_path = None;
        let mut profile_path = None;
        let mut binary_path = None;

        if let Some(bc_bytes) = bitcode {
            let p = version_dir.join("module.bc");
            fs::write(&p, bc_bytes)?;
            bitcode_path = Some(p);
        }

        if let Some(ll_text) = ll {
            let p = version_dir.join("assembly.ll");
            fs::write(&p, ll_text)?;
            ll_path = Some(p);
        }

        if let Some(prof_bytes) = profile {
            let p = version_dir.join("default.profdata");
            fs::write(&p, prof_bytes)?;
            profile_path = Some(p);
        }

        if let Some(bin_bytes) = binary {
            let p = version_dir.join("binary");
            fs::write(&p, bin_bytes)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o755));
            }
            binary_path = Some(p);
        }

        let timestamp = "2026-08-20T19:00:00Z".to_string();
        let artifact_ver = ArtifactVersion {
            version_id: version_id.to_string(),
            created_at: timestamp,
            bitcode_path,
            ll_path,
            profile_path,
            binary_path,
            metadata,
        };

        let manifest_path = version_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&artifact_ver)?;
        fs::write(manifest_path, manifest_json)?;

        Ok(artifact_ver)
    }

    /// Retrieve metadata for a stored artifact version.
    pub fn get_version(&self, version_id: &str) -> std::io::Result<Option<ArtifactVersion>> {
        let manifest_path = self.root_dir.join(version_id).join("manifest.json");
        if !manifest_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(manifest_path)?;
        let artifact_ver: ArtifactVersion = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        Ok(Some(artifact_ver))
    }

    /// List all artifact versions present in the store directory.
    pub fn list_versions(&self) -> std::io::Result<Vec<ArtifactVersion>> {
        let mut versions = Vec::new();

        if !self.root_dir.exists() {
            return Ok(versions);
        }

        for entry in fs::read_dir(&self.root_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(version_id) = entry.file_name().to_str() {
                    if let Some(ver) = self.get_version(version_id)? {
                        versions.push(ver);
                    }
                }
            }
        }

        versions.sort_by(|a, b| a.version_id.cmp(&b.version_id));
        Ok(versions)
    }

    /// Roll back active binary to the specified stored target version.
    pub fn rollback(
        &self,
        target_version_id: &str,
        active_binary_dest: &Path,
    ) -> std::io::Result<ArtifactVersion> {
        let ver = self.get_version(target_version_id)?.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Target version '{target_version_id}' not found in artifact store"),
            )
        })?;

        let bin_path = ver.binary_path.as_ref().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Target version '{target_version_id}' does not carry a binary artifact"),
            )
        })?;

        if let Some(parent) = active_binary_dest.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::copy(bin_path, active_binary_dest)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(active_binary_dest, fs::Permissions::from_mode(0o755));
        }

        Ok(ver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_and_retrieve_version() {
        let temp_dir = std::env::temp_dir().join("topos_store_test");
        let store = ArtifactStore::new(&temp_dir).unwrap();

        let mut meta = HashMap::new();
        meta.insert("plan_id".into(), "p1".into());

        let ver = store
            .store_version(
                "v1.0.0",
                Some(b"bc content"),
                Some("; LLVM IR"),
                Some(b"pgo prof"),
                Some(b"binary exe"),
                meta,
            )
            .unwrap();

        assert_eq!(ver.version_id, "v1.0.0");
        assert!(ver.binary_path.as_ref().unwrap().exists());

        let retrieved = store.get_version("v1.0.0").unwrap().unwrap();
        assert_eq!(retrieved.version_id, "v1.0.0");
        assert_eq!(retrieved.metadata.get("plan_id"), Some(&"p1".to_string()));

        let versions = store.list_versions().unwrap();
        assert_eq!(versions.len(), 1);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_rollback_binary() {
        let temp_dir = std::env::temp_dir().join("topos_store_rollback_test");
        let store = ArtifactStore::new(temp_dir.join("store")).unwrap();

        store
            .store_version(
                "v1",
                None,
                None,
                None,
                Some(b"version 1 binary"),
                HashMap::new(),
            )
            .unwrap();

        store
            .store_version(
                "v2",
                None,
                None,
                None,
                Some(b"version 2 binary"),
                HashMap::new(),
            )
            .unwrap();

        let active_dest = temp_dir.join("active_app");
        fs::write(&active_dest, b"version 2 binary").unwrap();

        // Perform rollback to v1
        let rolled_back = store.rollback("v1", &active_dest).unwrap();
        assert_eq!(rolled_back.version_id, "v1");

        let active_content = fs::read_to_string(&active_dest).unwrap();
        assert_eq!(active_content, "version 1 binary");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
