use std::fs;
use std::path::{Path, PathBuf};

/// Returns the cross-platform application data directory.
/// - macOS: ~/Library/Application Support/com.agentharbor.app
/// - Windows: %APPDATA%/com.agentharbor.app
/// - Linux: ~/.local/share/com.agentharbor.app (or $XDG_DATA_HOME)
pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("com.agentharbor.app")
}

/// Normalize line endings to LF (\n) for consistent hashing and diff comparison.
/// Windows IDEs may write \r\n which would cause false drift detection and noisy diffs.
pub fn normalize_line_endings(content: &str) -> String {
    content.replace("\r\n", "\n")
}

/// Cross-platform atomic file write.
/// On Unix: write to .tmp then rename (atomic).
/// On Windows: try rename; if destination locked, remove then rename.
pub fn atomic_write(path: &Path, content: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
    }

    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, content)
        .map_err(|e| format!("Failed to write temp file {}: {}", temp_path.display(), e))?;

    #[cfg(unix)]
    {
        fs::rename(&temp_path, path).map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            format!("Failed to rename {} to {}: {}", temp_path.display(), path.display(), e)
        })?;
    }

    #[cfg(windows)]
    {
        if fs::rename(&temp_path, path).is_err() {
            // On Windows, rename fails if destination is open by another process.
            // Try removing the destination first, then rename.
            let _ = fs::remove_file(path);
            fs::rename(&temp_path, path).map_err(|e| {
                let _ = fs::remove_file(&temp_path);
                format!("Failed to rename {} to {}: {}", temp_path.display(), path.display(), e)
            })?;
        }
    }

    Ok(())
}

/// Atomic write from a string (convenience wrapper).
pub fn atomic_write_str(path: &Path, content: &str) -> Result<(), String> {
    atomic_write(path, content.as_bytes())
}

/// Read a file with retry logic for Windows file locking.
/// On non-Windows platforms, this is a simple read.
pub fn read_with_sharing(path: &Path) -> Result<String, String> {
    #[cfg(not(target_os = "windows"))]
    {
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))
    }

    #[cfg(target_os = "windows")]
    {
        let max_retries = 3;
        for i in 0..max_retries {
            match fs::read_to_string(path) {
                Ok(content) => return Ok(content),
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && i < max_retries - 1 => {
                    std::thread::sleep(std::time::Duration::from_millis(50 * (i as u64 + 1)));
                }
                Err(e) => return Err(format!("Failed to read {}: {}", path.display(), e)),
            }
        }
        Err(format!("Failed to read {} after {} retries", path.display(), max_retries))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_data_dir_not_empty() {
        let dir = app_data_dir();
        assert!(dir.to_string_lossy().contains("com.agentharbor.app"));
    }

    #[test]
    fn test_normalize_line_endings() {
        assert_eq!(normalize_line_endings("a\r\nb\r\nc"), "a\nb\nc");
        assert_eq!(normalize_line_endings("a\nb\nc"), "a\nb\nc");
        assert_eq!(normalize_line_endings("a\r\nb\nc\r\n"), "a\nb\nc\n");
    }

    #[test]
    fn test_atomic_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        atomic_write(&path, b"hello world").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[test]
    fn test_atomic_write_str() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        atomic_write_str(&path, "hello world").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[test]
    fn test_atomic_write_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        atomic_write_str(&path, "first").unwrap();
        atomic_write_str(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
    }

    #[test]
    fn test_atomic_write_creates_parents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("dir").join("test.txt");
        atomic_write_str(&path, "nested").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "nested");
    }
}
