//! Versioned compiled artifacts under `<project>/.topos/compiled/`.
//!
//! Store paths and the rollback destination are resolved with
//! [`crate::paths::resolve_path_within`]. Rollback writes a sibling temp
//! file, `sync_all`s, restores mode, `rename`s over the destination, then
//! **reads the destination back** before `verified: true`.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::resolve_path_within;

const STORE_DIR: &str = ".topos/compiled";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineManifest {
    pub bytes: u64,
    pub mode: u32,
    #[serde(default)]
    pub dest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackResult {
    pub bytes_restored: u64,
    pub verified: bool,
}

#[derive(Debug, Clone)]
pub struct ArtifactStore {
    project_root: PathBuf,
    store_root: PathBuf,
}

impl ArtifactStore {
    pub fn open(project_root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let project_root = project_root.as_ref();
        fs::create_dir_all(project_root)?;
        let project_root = fs::canonicalize(project_root)?;
        let store_root = project_root.join(STORE_DIR);
        fs::create_dir_all(&store_root)?;
        let store_root = fs::canonicalize(&store_root)?;
        Ok(Self {
            project_root,
            store_root,
        })
    }

    pub fn store_root(&self) -> &Path {
        &self.store_root
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn run_dir(&self, run_id: &str) -> Result<PathBuf, StoreError> {
        self.contained_store_path(&format!("runs/{run_id}"))
    }

    pub fn create_run(&self, run_id: &str) -> Result<PathBuf, StoreError> {
        let dir = self.run_dir(run_id)?;
        fs::create_dir_all(dir.join("baseline"))?;
        fs::create_dir_all(dir.join("candidates"))?;
        Ok(dir)
    }

    pub fn candidate_dir(&self, run_id: &str, slot: usize) -> Result<PathBuf, StoreError> {
        self.contained_store_path(&format!("runs/{run_id}/candidates/c{slot:02}"))
    }

    pub fn save_baseline(
        &self,
        run_id: &str,
        bytes: &[u8],
        mode: u32,
    ) -> Result<PathBuf, StoreError> {
        let dir = self.run_dir(run_id)?.join("baseline");
        let dir = self.contain(&dir)?;
        fs::create_dir_all(&dir)?;
        let binary = dir.join("binary");
        fs::write(&binary, bytes)?;
        set_mode(&binary, mode)?;
        let manifest = BaselineManifest {
            bytes: bytes.len() as u64,
            mode,
            dest: None,
        };
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).map_err(|e| StoreError::Json(e.to_string()))?,
        )?;
        Ok(binary)
    }

    pub fn load_manifest(&self, run_id: &str) -> Result<BaselineManifest, StoreError> {
        let path = self.contain(&self.run_dir(run_id)?.join("baseline/manifest.json"))?;
        serde_json::from_slice(&fs::read(path)?).map_err(|e| StoreError::Json(e.to_string()))
    }

    /// Record (and optionally replace) the rollback snapshot for this run.
    pub fn set_rollback_snapshot(
        &self,
        run_id: &str,
        dest: &Path,
        bytes: Option<&[u8]>,
        mode: Option<u32>,
    ) -> Result<(), StoreError> {
        let dest = resolve_path_within(&dest.to_string_lossy(), &self.project_root)
            .map_err(StoreError::Path)?;
        if let Some(bytes) = bytes {
            self.save_baseline(run_id, bytes, mode.unwrap_or(0o755))?;
        }
        let mut manifest = self.load_manifest(run_id)?;
        if let Some(mode) = mode {
            manifest.mode = mode;
        }
        if let Some(bytes) = bytes {
            manifest.bytes = bytes.len() as u64;
        }
        manifest.dest = Some(dest.display().to_string());
        let path = self.contain(&self.run_dir(run_id)?.join("baseline/manifest.json"))?;
        fs::write(
            path,
            serde_json::to_vec_pretty(&manifest).map_err(|e| StoreError::Json(e.to_string()))?,
        )?;
        Ok(())
    }

    pub fn rollback_recorded(&self, run_id: Option<&str>) -> Result<RollbackResult, StoreError> {
        let run_id = match run_id {
            Some(id) => {
                reject_unsafe_id(id)?;
                id.to_string()
            }
            None => self.current_run_id()?.ok_or(StoreError::NoCurrentRun)?,
        };
        let manifest = self.load_manifest(&run_id)?;
        let dest = manifest.dest.ok_or(StoreError::NoDest)?;
        self.rollback(Some(&run_id), Path::new(&dest))
    }

    pub fn set_current(&self, run_id: &str) -> Result<(), StoreError> {
        reject_unsafe_id(run_id)?;
        let pointer = self.contain(&self.store_root.join("current"))?;
        fs::write(pointer, run_id.as_bytes())?;
        Ok(())
    }

    pub fn current_run_id(&self) -> Result<Option<String>, StoreError> {
        let pointer = self.store_root.join("current");
        if !pointer.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(pointer)?;
        let id = text.trim();
        if id.is_empty() {
            return Ok(None);
        }
        reject_unsafe_id(id)?;
        Ok(Some(id.to_string()))
    }

    /// Restore the stored baseline over `dest`. `dest` must stay inside the
    /// project root. Length mismatch with the manifest is `Corrupt`.
    pub fn rollback(
        &self,
        run_id: Option<&str>,
        dest: &Path,
    ) -> Result<RollbackResult, StoreError> {
        let run_id = match run_id {
            Some(id) => {
                reject_unsafe_id(id)?;
                id.to_string()
            }
            None => self.current_run_id()?.ok_or(StoreError::NoCurrentRun)?,
        };
        let dest = resolve_path_within(&dest.to_string_lossy(), &self.project_root)
            .map_err(StoreError::Path)?;
        let baseline_dir = self.contain(&self.run_dir(&run_id)?.join("baseline"))?;
        let binary_path = self.contain(&baseline_dir.join("binary"))?;
        let manifest_path = self.contain(&baseline_dir.join("manifest.json"))?;
        let manifest: BaselineManifest = serde_json::from_slice(&fs::read(&manifest_path)?)
            .map_err(|e| StoreError::Json(e.to_string()))?;
        let stored = fs::read(&binary_path)?;
        if stored.len() as u64 != manifest.bytes {
            return Err(StoreError::Corrupt {
                expected: manifest.bytes,
                actual: stored.len() as u64,
            });
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = dest.with_file_name(format!(
            "{}.topos-rollback-{}",
            dest.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id()
        ));
        {
            let mut file = File::create(&tmp)?;
            file.write_all(&stored)?;
            file.sync_all()?;
        }
        set_mode(&tmp, manifest.mode)?;
        fs::rename(&tmp, &dest)?;
        let read_back = fs::read(&dest)?;
        let verified = read_back == stored;
        Ok(RollbackResult {
            bytes_restored: if verified { stored.len() as u64 } else { 0 },
            verified,
        })
    }

    fn contained_store_path(&self, rel: &str) -> Result<PathBuf, StoreError> {
        reject_unsafe_rel(rel)?;
        self.contain(&self.store_root.join(rel))
    }

    fn contain(&self, path: &Path) -> Result<PathBuf, StoreError> {
        resolve_path_within(&path.to_string_lossy(), &self.store_root).map_err(StoreError::Path)
    }
}

fn reject_unsafe_id(id: &str) -> Result<(), StoreError> {
    if id.is_empty()
        || id.contains("..")
        || id.contains('/')
        || id.contains('\\')
        || id.contains('\0')
    {
        return Err(StoreError::Path(format!(
            "Access denied: run id must be a single path segment, got {id}"
        )));
    }
    Ok(())
}

fn reject_unsafe_rel(rel: &str) -> Result<(), StoreError> {
    if rel.contains('\0') {
        return Err(StoreError::Path("Access denied: NUL in path".into()));
    }
    Ok(())
}

fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    let _ = (path, mode);
    Ok(())
}

#[derive(Debug)]
pub enum StoreError {
    Path(String),
    Corrupt { expected: u64, actual: u64 },
    NoCurrentRun,
    NoDest,
    Json(String),
    Io(io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Path(msg) => write!(f, "{msg}"),
            StoreError::Corrupt { expected, actual } => write!(
                f,
                "baseline corrupt: manifest length {expected}, file length {actual}"
            ),
            StoreError::NoCurrentRun => write!(f, "no current compiled run to roll back"),
            StoreError::NoDest => write!(f, "no rollback destination recorded for this run"),
            StoreError::Json(err) => write!(f, "artifact JSON: {err}"),
            StoreError::Io(err) => write!(f, "artifact I/O: {err}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(err: io::Error) -> Self {
        StoreError::Io(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "topos_artifacts_{label}_{}_{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rollback_restores_the_exact_baseline_bytes() {
        let root = temp_project("rollback");
        let store = ArtifactStore::open(&root).unwrap();
        store.create_run("run1").unwrap();
        store.save_baseline("run1", b"BASELINE-v1", 0o755).unwrap();
        store.set_current("run1").unwrap();

        let dest = root.join("app.bin");
        fs::write(&dest, b"CANDIDATE-v2-longer").unwrap();

        let result = store.rollback(None, &dest).unwrap();
        assert!(result.verified);
        assert_eq!(result.bytes_restored, 11);
        assert_eq!(fs::read(&dest).unwrap(), b"BASELINE-v1");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn artifact_paths_escaping_the_store_are_rejected() {
        let root = temp_project("escape");
        let store = ArtifactStore::open(&root).unwrap();
        let err = store.run_dir("../../etc/passwd").unwrap_err();
        assert!(
            matches!(err, StoreError::Path(ref msg) if msg.contains("Access denied") || msg.contains("single path segment")),
            "{err}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_escaping_the_store_is_rejected() {
        let root = temp_project("symlink");
        let store = ArtifactStore::open(&root).unwrap();
        store.create_run("run1").unwrap();
        let link = store.store_root().join("runs/run1/baseline/binary");
        if link.exists() {
            fs::remove_file(&link).ok();
        }
        std::os::unix::fs::symlink("/etc/passwd", &link).unwrap();
        let err = store
            .contain(&store.store_root().join("runs/run1/baseline/binary"))
            .unwrap_err();
        assert!(
            matches!(err, StoreError::Path(ref msg) if msg.contains("Access denied")),
            "{err}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn length_mismatch_is_corrupt_and_refuses_to_write() {
        let root = temp_project("corrupt");
        let store = ArtifactStore::open(&root).unwrap();
        store.create_run("run1").unwrap();
        store.save_baseline("run1", b"BASELINE-v1", 0o755).unwrap();
        let dest = root.join("app.bin");
        fs::write(&dest, b"keep-me").unwrap();
        fs::write(
            store.run_dir("run1").unwrap().join("baseline/binary"),
            b"short",
        )
        .unwrap();
        let err = store.rollback(Some("run1"), &dest).unwrap_err();
        assert!(matches!(err, StoreError::Corrupt { .. }));
        assert_eq!(fs::read(&dest).unwrap(), b"keep-me");
        let _ = fs::remove_dir_all(&root);
    }
}
