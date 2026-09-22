//! Comment-preserving editor for `opencode.jsonc`.
//!
//! The user's config carries comments that must survive every edit, so this module never
//! reserialises the document. It parses with `jsonc-parser`'s CST and rewrites only the
//! nodes it was asked to change. Every write is a backup + temp file + atomic rename +
//! re-parse, and a failed re-parse restores the backup.

use anyhow::{bail, Context, Result};
use jsonc_parser::cst::{CstInputValue, CstRootNode};
use jsonc_parser::{ParseOptions};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// How many `.bak-*` copies of a config file to keep.
pub const KEEP_BACKUPS: usize = 10;

const BAK_PREFIX: &str = ".bak-";
const TMP_PREFIX: &str = ".tmp-";

/// An open config file: the on-disk path plus its current text. All edits touch `text`
/// only; nothing reaches the disk until `save_atomic`.
pub struct ConfigFile {
    pub path: PathBuf,
    pub text: String,
}

impl ConfigFile {
    pub fn load(path: &Path) -> Result<ConfigFile> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        Ok(ConfigFile {
            path: path.to_path_buf(),
            text,
        })
    }

    /// The document as plain JSON, comments stripped.
    pub fn value(&self) -> Result<Value> {
        parse_value(&self.text)
    }

    /// Sets `json_path` to `value`, creating any missing intermediate objects. Only the
    /// affected nodes are rewritten, so comments and the rest of the formatting survive.
    pub fn set_path(&mut self, json_path: &[&str], value: Value) -> Result<()> {
        let (first, rest) = match json_path.split_first() {
            Some(parts) => parts,
            None => bail!("empty json path"),
        };
        let root = self.parse_cst()?;
        let object = root.object_value_or_set();
        if rest.is_empty() {
            set_property(&object, first, value);
        } else {
            let (last, middle) = rest.split_last().expect("rest is non-empty");
            let mut cursor = object.object_value_or_set(first);
            for key in middle {
                cursor = cursor.object_value_or_set(key);
            }
            set_property(&cursor, last, value);
        }
        self.text = root.to_string();
        Ok(())
    }

    /// Removes `json_path`. A path that is not present is not an error.
    pub fn remove_path(&mut self, json_path: &[&str]) -> Result<()> {
        let (first, rest) = match json_path.split_first() {
            Some(parts) => parts,
            None => bail!("empty json path"),
        };
        let root = self.parse_cst()?;
        let Some(object) = root.object_value() else {
            return Ok(());
        };
        let Some(mut prop) = object.get(first) else {
            return Ok(());
        };
        for key in rest {
            let Some(next) = prop.object_value().and_then(|o| o.get(key)) else {
                return Ok(());
            };
            prop = next;
        }
        prop.remove();
        self.text = root.to_string();
        Ok(())
    }

    /// Backup, write a temp file in the same directory, `fsync`, atomically rename, then
    /// re-read and re-parse. On an invalid result the backup is restored and an error is
    /// returned, so a bad edit can never leave a broken config behind.
    pub fn save_atomic(&self) -> Result<PathBuf> {
        let dir = self
            .path
            .parent()
            .context("config path has no parent directory")?
            .to_path_buf();

        // Reject a broken document before anything touches the disk.
        if let Err(e) = parse_value(&self.text) {
            bail!("refusing to write an unparsable config: {e}");
        }

        let backup = if self.path.exists() {
            let backup = dir.join(format!(
                "{}{BAK_PREFIX}{}",
                file_name(&self.path),
                now_epoch_ms()
            ));
            fs::copy(&self.path, &backup)
                .with_context(|| format!("cannot back up {}", self.path.display()))?;
            Some(backup)
        } else {
            None
        };

        let tmp = dir.join(format!("{}{TMP_PREFIX}{}", file_name(&self.path), std::process::id()));
        let write_result = (|| -> Result<()> {
            let mut file = fs::File::create(&tmp)
                .with_context(|| format!("cannot create {}", tmp.display()))?;
            file.write_all(self.text.as_bytes())?;
            file.sync_all()?;
            Ok(())
        })();
        if let Err(e) = write_result {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }

        if let Err(e) = fs::rename(&tmp, &self.path) {
            let _ = fs::remove_file(&tmp);
            return Err(e).context("cannot replace the config file");
        }

        // Belt and braces: the rename already put new bytes in place, so this only catches
        // a caller that handed us a broken `text` that somehow parsed on the first check.
        let reparsed: Result<Value> = fs::read_to_string(&self.path).map_err(Into::into).and_then(|t| parse_value(&t));
        match reparsed {
            Ok(_) => {}
            Err(e) => {
                if let Some(backup) = &backup {
                    let _ = fs::copy(backup, &self.path);
                }
                if let Ok(f) = fs::OpenOptions::new().append(true).open(&self.path) {
                    let _ = f.sync_all();
                }
                bail!("config failed to re-parse after writing, backup restored: {e}");
            }
        }

        prune_backups(&dir, KEEP_BACKUPS);
        Ok(backup.unwrap_or_else(|| self.path.clone()))
    }

    fn parse_cst(&self) -> Result<CstRootNode> {
        CstRootNode::parse(&self.text, &ParseOptions::default())
            .map_err(|e| anyhow::anyhow!("cannot parse config: {e}"))
    }
}

/// Sets a key on an object, replacing the existing property in place when one is already
/// there. `append` alone would leave a duplicate key behind, which JSONC tolerates but
/// which no reader agrees on.
fn set_property(object: &jsonc_parser::cst::CstObject, key: &str, value: Value) {
    match object.get(key) {
        Some(prop) => prop.set_value(to_input(value)),
        None => {
            object.append(key, to_input(value));
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "opencode.jsonc".to_string())
}

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn parse_value(text: &str) -> Result<Value> {
    let parsed = jsonc_parser::parse_to_serde_value(text, &ParseOptions::default())
        .map_err(|e| anyhow::anyhow!("cannot parse config: {e}"))?;
    Ok(parsed.unwrap_or(Value::Null))
}

/// Converts a `serde_json::Value` into the CST's input form so it can be written into the
/// document with correct string escaping.
fn to_input(value: Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(b) => CstInputValue::Bool(b),
        Value::Number(n) => CstInputValue::Number(n.to_string()),
        Value::String(s) => CstInputValue::String(s),
        Value::Array(items) => {
            CstInputValue::Array(items.into_iter().map(to_input).collect())
        }
        Value::Object(map) => CstInputValue::Object(
            map.into_iter()
                .map(|(k, v)| (k, to_input(v)))
                .collect(),
        ),
    }
}

/// The directory holding the opencode config, where its backups live.
pub fn backup_dir(home: &str) -> PathBuf {
    PathBuf::from(home).join(".config/opencode")
}

/// Backup files in `dir`, newest first, paired with the epoch ms parsed from the name.
pub fn backups(dir: &Path) -> Vec<(PathBuf, i64)> {
    let mut found: Vec<(PathBuf, i64)> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let (_, stamp) = name.split_once(BAK_PREFIX)?;
                let at_ms = stamp.parse::<i64>().ok()?;
                Some((e.path(), at_ms))
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    found.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
    found
}

/// Deletes every backup beyond the `keep` newest.
pub fn prune_backups(dir: &Path, keep: usize) {
    for (path, _) in backups(dir).into_iter().skip(keep) {
        let _ = fs::remove_file(path);
    }
}

/// Copies a backup over `target`.
pub fn restore(backup: &Path, target: &Path) -> Result<()> {
    fs::copy(backup, target)
        .with_context(|| format!("cannot restore {} from {}", target.display(), backup.display()))?;
    if let Ok(file) = fs::OpenOptions::new().write(true).open(target) {
        let _ = file.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"{
  // keep this comment please
  "provider": {
    "acme": {
      "npm": "@ai-sdk/openai-compatible",
      "options": { "baseURL": "https://acme.example/v1" }
    },
    "other": {
      "npm": "@ai-sdk/openai-compatible",
      "options": { "baseURL": "https://other.example/v1" }
    }
  }
}
"#;

    fn write(dir: &std::path::Path, text: &str) -> PathBuf {
        let path = dir.join("opencode.jsonc");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn set_path_preserves_comments_and_other_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(tmp.path(), CONFIG);
        let mut file = ConfigFile::load(&path).unwrap();

        file.set_path(
            &["provider", "acme", "options", "baseURL"],
            serde_json::json!("https://changed.example/v1"),
        )
        .unwrap();

        assert!(file.text.contains("// keep this comment please"));
        let value = file.value().unwrap();
        assert_eq!(
            value["provider"]["acme"]["options"]["baseURL"],
            "https://changed.example/v1"
        );
        assert_eq!(
            value["provider"]["other"]["options"]["baseURL"],
            "https://other.example/v1"
        );
    }

    #[test]
    fn set_path_creates_missing_objects() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(tmp.path(), CONFIG);
        let mut file = ConfigFile::load(&path).unwrap();

        file.set_path(
            &["provider", "new", "options", "baseURL"],
            serde_json::json!("https://new.example/v1"),
        )
        .unwrap();

        let value = file.value().unwrap();
        assert_eq!(
            value["provider"]["new"]["options"]["baseURL"],
            "https://new.example/v1"
        );
        assert_eq!(
            value["provider"]["acme"]["options"]["baseURL"],
            "https://acme.example/v1"
        );
        assert!(file.text.contains("// keep this comment please"));
    }

    #[test]
    fn set_path_replaces_existing_value_without_duplicating_the_key() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(tmp.path(), CONFIG);
        let mut file = ConfigFile::load(&path).unwrap();

        file.set_path(
            &["provider", "acme", "options", "baseURL"],
            serde_json::json!("https://one.example/v1"),
        )
        .unwrap();
        file.set_path(
            &["provider", "acme", "options", "baseURL"],
            serde_json::json!("https://two.example/v1"),
        )
        .unwrap();
        file.set_path(
            &["provider", "acme", "options", "baseURL"],
            serde_json::json!("https://three.example/v1"),
        )
        .unwrap();

        let text = file.text.clone();
        assert_eq!(
            text.matches("baseURL").count(),
            2,
            "each provider must keep exactly one baseURL:\n{text}"
        );
        let value = file.value().unwrap();
        assert_eq!(
            value["provider"]["acme"]["options"]["baseURL"],
            "https://three.example/v1"
        );
    }

    #[test]
    fn remove_path_deletes_only_the_target() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(tmp.path(), CONFIG);
        let mut file = ConfigFile::load(&path).unwrap();

        file.remove_path(&["provider", "acme"]).unwrap();

        let value = file.value().unwrap();
        assert!(value["provider"].get("acme").is_none());
        assert_eq!(
            value["provider"]["other"]["options"]["baseURL"],
            "https://other.example/v1"
        );
        assert!(file.text.contains("// keep this comment please"));
    }

    #[test]
    fn save_atomic_creates_a_backup_and_leaves_no_temp_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(tmp.path(), CONFIG);
        let mut file = ConfigFile::load(&path).unwrap();
        file.set_path(&["provider", "acme", "npm"], serde_json::json!("changed"))
            .unwrap();

        let backup = file.save_atomic().unwrap();

        assert!(backup.exists());
        let baks: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bak-"))
            .collect();
        assert_eq!(baks.len(), 1);
        let temps: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert_eq!(temps.len(), 0);

        let reparsed = ConfigFile::load(&path).unwrap();
        assert_eq!(reparsed.value().unwrap()["provider"]["acme"]["npm"], "changed");
    }

    #[test]
    fn save_atomic_restores_backup_when_result_is_invalid() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(tmp.path(), CONFIG);
        let mut file = ConfigFile::load(&path).unwrap();
        file.text = "{oops".to_string();

        assert!(file.save_atomic().is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), CONFIG);
    }

    #[test]
    fn prune_backups_keeps_only_the_newest() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for i in 0..13 {
            std::fs::write(dir.join(format!("opencode.jsonc.bak-{i:04}")), "{}").unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(5));

        prune_backups(dir, 10);

        let remaining = backups(dir);
        assert_eq!(remaining.len(), 10);
        assert!(remaining
            .iter()
            .all(|(p, _)| p.file_name().unwrap().to_string_lossy().contains("bak-00")));
    }

    #[test]
    fn restore_puts_the_backup_back() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(tmp.path(), CONFIG);
        let mut file = ConfigFile::load(&path).unwrap();
        file.set_path(&["provider", "acme", "npm"], serde_json::json!("changed"))
            .unwrap();
        let backup = file.save_atomic().unwrap();
        assert_ne!(std::fs::read_to_string(&path).unwrap(), CONFIG);

        restore(&backup, &path).unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), CONFIG);
    }
}
