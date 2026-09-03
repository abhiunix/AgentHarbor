use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

/// Resolve the Codex state directory in the same order as Codex itself.
/// `CODEX_HOME` allows separate profiles and must take precedence over the
/// default `~/.codex` location.
pub fn codex_home() -> Result<PathBuf, String> {
    codex_home_from(std::env::var_os("CODEX_HOME"), dirs::home_dir())
}

fn codex_home_from(
    codex_home: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(value) = codex_home {
        if !value.is_empty() {
            let path = PathBuf::from(&value);
            let metadata = fs::metadata(&path).map_err(|error| {
                format!(
                    "CODEX_HOME points to {}, but that path could not be read: {}",
                    path.display(),
                    error
                )
            })?;
            if !metadata.is_dir() {
                return Err(format!(
                    "CODEX_HOME points to {}, but that path is not a directory",
                    path.display()
                ));
            }
            return dunce::canonicalize(&path).map_err(|error| {
                format!("Failed to resolve CODEX_HOME {}: {}", path.display(), error)
            });
        }
    }

    let home = home_dir.ok_or_else(|| "Could not determine the home directory".to_string())?;
    if !home.is_absolute() {
        return Err("The home directory must be an absolute path".to_string());
    }
    Ok(home.join(".codex"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_codex_home_wins() {
        let profile = tempfile::tempdir().unwrap();
        let resolved = codex_home_from(
            Some(profile.path().as_os_str().to_owned()),
            Some(PathBuf::from("/Users/example")),
        )
        .unwrap();
        assert_eq!(resolved, dunce::canonicalize(profile.path()).unwrap());
    }

    #[test]
    fn empty_codex_home_uses_default() {
        let resolved =
            codex_home_from(Some(OsString::new()), Some(PathBuf::from("/Users/example"))).unwrap();
        assert_eq!(resolved, PathBuf::from("/Users/example/.codex"));
    }

    #[test]
    fn missing_home_is_an_error() {
        let error = codex_home_from(None, None).unwrap_err();
        assert!(error.contains("home directory"));
    }

    #[test]
    fn explicit_missing_codex_home_is_an_error() {
        let parent = tempfile::tempdir().unwrap();
        let missing = parent.path().join("missing");
        let error = codex_home_from(Some(missing.into_os_string()), None).unwrap_err();
        assert!(error.contains("CODEX_HOME"));
        assert!(error.contains("could not be read"));
    }

    #[test]
    fn explicit_file_codex_home_is_an_error() {
        let parent = tempfile::tempdir().unwrap();
        let file = parent.path().join("not-a-directory");
        fs::write(&file, "content").unwrap();
        let error = codex_home_from(Some(file.into_os_string()), None).unwrap_err();
        assert!(error.contains("not a directory"));
    }

    #[cfg(unix)]
    #[test]
    fn explicit_codex_home_symlink_is_canonicalized() {
        use std::os::unix::fs::symlink;

        let parent = tempfile::tempdir().unwrap();
        let target = parent.path().join("profile");
        let link = parent.path().join("profile-link");
        fs::create_dir(&target).unwrap();
        symlink(&target, &link).unwrap();

        let resolved = codex_home_from(Some(link.into_os_string()), None).unwrap();
        assert_eq!(resolved, dunce::canonicalize(target).unwrap());
    }

    #[test]
    fn relative_default_home_is_an_error() {
        let error = codex_home_from(None, Some(PathBuf::from("relative-home"))).unwrap_err();
        assert!(error.contains("absolute path"));
    }
}
