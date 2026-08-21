//! Symlink-safe path containment.
//!
//! Resolve a path against a root and reject anything that escapes it, even
//! when the leaf does not exist yet. That last property is the point: an
//! artifact store must be able to name a destination that has not been
//! written, without falling back to a lexical normalize that ignores
//! symlinks on the existing prefix.

use std::path::{Component, Path, PathBuf};

/// Resolve symlinks incrementally, one path component at a time, matching
/// Python `Path.resolve(strict=False)`.
///
/// A plain `canonicalize().unwrap_or_else(normalize)` is unsafe: when the
/// leaf is missing, lexical normalize does not follow symlinks on the
/// existing prefix, so `/proj/link/newfile` with `link → /etc` would be
/// accepted under root `/proj`.
///
/// Walking *forwards* avoids the `..` trap of a backward walk: once a
/// component is found not to exist, nothing after it can be a symlink
/// either, so the remaining components are safe to apply lexically against
/// the already-resolved real prefix. The unresolved depth is tracked so
/// that a `..` which removes every missing component resumes symlink
/// resolution.
pub fn resolve_existing_prefix(path: &Path) -> PathBuf {
    let mut resolved = PathBuf::new();
    let mut missing_components: usize = 0;
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
                missing_components = missing_components.saturating_sub(1);
            }
            Component::Prefix(_) | Component::RootDir => resolved.push(component),
            Component::Normal(name) => {
                if missing_components > 0 {
                    resolved.push(name);
                    missing_components += 1;
                    continue;
                }
                match resolved.join(name).canonicalize() {
                    Ok(real) => resolved = real,
                    Err(_) => {
                        missing_components = 1;
                        resolved.push(name);
                    }
                }
            }
        }
    }
    resolved
}

/// Resolve `filepath` against `root` and reject paths that escape it.
pub fn resolve_path_within(filepath: &str, root: &Path) -> Result<PathBuf, String> {
    let path = Path::new(filepath);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let resolved = resolve_existing_prefix(&joined);
    if resolved.starts_with(root) {
        Ok(resolved)
    } else {
        Err(format!(
            "Access denied: path must be inside {}. Got: {}",
            root.display(),
            resolved.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "topos_paths_{label}_{}_{nanos}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    #[test]
    fn a_relative_path_inside_the_root_resolves() {
        let root = temp_root("relative");
        std::fs::create_dir_all(root.join("src")).unwrap();
        let resolved = resolve_path_within("src/main.c", &root).unwrap();
        assert_eq!(resolved, root.join("src/main.c"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dotdot_escaping_the_root_is_rejected() {
        let root = temp_root("escape");
        let err = resolve_path_within("../../etc/passwd", &root).unwrap_err();
        assert!(err.contains("Access denied"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_leaf_inside_the_root_still_resolves() {
        let root = temp_root("missing-leaf");
        let missing = root.join("does-not-exist-yet.rs");
        let resolved = resolve_path_within(missing.to_str().unwrap(), &root).unwrap();
        assert_eq!(resolved, missing);
        assert!(!missing.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_escaping_the_root_is_rejected() {
        let root = temp_root("symlink");
        let link = root.join("link");
        std::os::unix::fs::symlink("/etc", &link).unwrap();
        let request = link.join("newfile");
        let err = resolve_path_within(request.to_str().unwrap(), &root).unwrap_err();
        assert!(err.contains("Access denied"), "{err}");
        assert!(err.contains("/etc"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_via_dotdot_and_missing_intermediate_is_denied() {
        let dir = temp_root("dotdot-symlink");
        let root_path = dir.join("proj");
        std::fs::create_dir_all(&root_path).unwrap();
        let root = root_path.canonicalize().unwrap();
        let outside = dir.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let outside = outside.canonicalize().unwrap();
        let link = root.join("link");
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        let request = format!("{}/subdir/../newfile", link.display());
        let err = resolve_path_within(&request, &root).unwrap_err();
        assert!(err.contains("Access denied"), "{err}");
        assert!(
            err.contains(&outside.display().to_string()),
            "expected resolved path under {}, got: {err}",
            outside.display()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_checks_resume_after_dotdot_removes_a_missing_component() {
        let dir = temp_root("missing-dotdot");
        let root_path = dir.join("proj");
        let outside = dir.join("outside");
        std::fs::create_dir_all(&root_path).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let root = root_path.canonicalize().unwrap();
        let outside = outside.canonicalize().unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();

        let request = root.join("missing/../link/file");
        let err = resolve_path_within(request.to_str().unwrap(), &root).unwrap_err();
        assert!(err.contains("Access denied"), "{err}");
        assert!(err.contains(&outside.display().to_string()), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
