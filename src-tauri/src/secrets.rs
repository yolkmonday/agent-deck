//! Provider API key storage.
//!
//! A key value never goes into `opencode.jsonc`: it lives in its own file under
//! `~/.config/opencode/secrets/` (dir 0700, file 0600) and the config only holds a
//! `{file:...}` reference to it. Nothing in this module logs a key.

use anyhow::{bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// `<home>/.config/opencode/secrets`
pub fn secrets_dir(home: &str) -> PathBuf {
    PathBuf::from(home).join(".config/opencode/secrets")
}

/// Path of a provider's key file. The id is validated so no caller can escape the
/// secrets directory with `..` or a path separator.
pub fn key_path(home: &str, provider_id: &str) -> Result<PathBuf> {
    if !is_valid_id(provider_id) {
        bail!("invalid provider id");
    }
    Ok(secrets_dir(home).join(format!("{provider_id}.key")))
}

/// Provider ids end up in a file name and in config keys, so they are restricted to a
/// conservative charset and a bounded length.
fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Writes a key to its 0600 file, creating the 0700 directory if needed. The modes are
/// set at creation time so the plaintext is never briefly world readable.
pub fn write_key(home: &str, provider_id: &str, key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        bail!("key is empty");
    }
    let path = key_path(home, provider_id)?;
    let dir = path.parent().context("key path has no parent")?;
    create_dir_0700(dir)?;

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    file.write_all(key.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn create_dir_0700(dir: &Path) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    if dir.is_dir() {
        return Ok(());
    }
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .with_context(|| format!("cannot create {}", dir.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn create_dir_0700(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))
}

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

pub fn read_key(home: &str, provider_id: &str) -> Result<String> {
    let path = key_path(home, provider_id)?;
    let text = fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    Ok(text.trim().to_string())
}

/// Removes the key file. A missing file is not an error.
pub fn clear_key(home: &str, provider_id: &str) -> Result<()> {
    let path = key_path(home, provider_id)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("cannot remove {}", path.display())),
    }
}

/// The only form of a key the frontend may see outside an explicit reveal: first 6 and
/// last 2 characters with the middle replaced by bullets.
pub fn mask(key: &str) -> String {
    let key = key.trim();
    if key.chars().count() < 12 {
        return "••••••••".to_string();
    }
    let chars: Vec<char> = key.chars().collect();
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 2..].iter().collect();
    format!("{head}••••••••{tail}")
}

/// The portable reference written into the config, always in `~` form so the file can be
/// moved between machines.
pub fn file_ref(_home: &str, provider_id: &str) -> String {
    format!("{{file:~/.config/opencode/secrets/{provider_id}.key}}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn write_key_sets_0600_and_dir_0700() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = home();
        let home = tmp.path().to_str().unwrap();
        write_key(home, "acme", "sk-abcdefghijkl").unwrap();

        let dir_mode = std::fs::metadata(secrets_dir(home))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let file_mode = std::fs::metadata(key_path(home, "acme").unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(dir_mode, 0o700);
        assert_eq!(file_mode, 0o600);
    }

    #[test]
    fn read_key_round_trips_and_trims() {
        let tmp = home();
        let home = tmp.path().to_str().unwrap();
        write_key(home, "acme", "  abc\n").unwrap();
        assert_eq!(read_key(home, "acme").unwrap(), "abc");
    }

    #[test]
    fn clear_key_is_idempotent() {
        let tmp = home();
        let home = tmp.path().to_str().unwrap();
        write_key(home, "acme", "sk-abcdefghijkl").unwrap();
        let path = key_path(home, "acme").unwrap();

        clear_key(home, "acme").unwrap();
        clear_key(home, "acme").unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn key_path_rejects_traversal() {
        let tmp = home();
        let home = tmp.path().to_str().unwrap();
        let long = "a".repeat(65);

        for id in ["../evil", "a/b", "", &long] {
            assert!(key_path(home, id).is_err(), "id {id:?} should be rejected");
        }
    }

    #[test]
    fn mask_hides_the_middle() {
        let key = "sk-6a8b9f7e862e2668-e63vy0-0f8658dc";
        let masked = mask(key);

        assert!(masked.starts_with("sk-6a8"), "got {masked}");
        assert!(masked.ends_with("dc"), "got {masked}");
        assert!(masked.contains('•'), "got {masked}");
        assert!(
            !masked.contains("9f7e862e2668"),
            "masked value leaked the key body: {masked}"
        );
    }

    #[test]
    fn mask_short_key_is_fully_hidden() {
        assert_eq!(mask("short"), "••••••••");
    }

    #[test]
    fn write_key_rejects_empty() {
        let tmp = home();
        let home = tmp.path().to_str().unwrap();

        assert!(write_key(home, "acme", "").is_err());
        assert!(write_key(home, "acme", "   ").is_err());
    }

    #[test]
    fn file_ref_uses_tilde_form() {
        let tmp = home();
        let home = tmp.path().to_str().unwrap();
        assert_eq!(
            file_ref(home, "acme"),
            "{file:~/.config/opencode/secrets/acme.key}"
        );
    }
}
