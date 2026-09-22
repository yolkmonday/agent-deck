use collector::store::ProjectRow;
use serde::{Deserialize, Serialize};

pub const PROFILE_IDS: [&str; 3] = ["claude", "opencode", "codex"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInput {
    pub name: String,
    pub path: String,
    pub default_profile: Option<String>,
    pub color: Option<String>,
    pub sort_order: i64,
}

pub const MAX_NAME_LEN: usize = 60;

/// `~` and `~/x` expand; `~user/x` is left alone, since resolving another user's
/// home is not something this app may do without reading the password database.
pub fn expand_tilde(path: &str, home: &str) -> String {
    if path == "~" {
        return home.to_string();
    }
    match path.strip_prefix("~/") {
        Some(rest) => format!("{home}/{rest}"),
        None => path.to_string(),
    }
}

pub fn normalise_path(path: &str, home: &str) -> String {
    let expanded = expand_tilde(path.trim(), home);
    let trailing = expanded.len() > 1;
    let trimmed = if trailing {
        expanded.trim_end_matches('/')
    } else {
        expanded.as_str()
    };
    let collapsed = trimmed
        .split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    if expanded.starts_with('/') {
        format!("/{collapsed}")
    } else {
        collapsed
    }
}

pub fn display_name(path: &str, home: &str) -> String {
    if path == home {
        return "~".to_string();
    }
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Returns the normalised path, or the exact Indonesian error the UI shows.
/// `editing_id` is the row being updated, so it does not collide with itself.
pub fn validate(
    input: &ProjectInput,
    home: &str,
    existing: &[ProjectRow],
    editing_id: Option<&str>,
) -> Result<String, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("nama tidak boleh kosong".into());
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err("nama terlalu panjang".into());
    }

    let path = normalise_path(&input.path, home);
    if !path.starts_with('/') {
        return Err("path harus absolut".into());
    }
    if !std::path::Path::new(&path).is_dir() {
        return Err("folder tidak ditemukan".into());
    }

    let collision = existing
        .iter()
        .any(|r| r.path == path && Some(r.id.as_str()) != editing_id);
    if collision {
        return Err("project dengan folder ini sudah ada".into());
    }

    if let Some(profile) = input.default_profile.as_deref() {
        if !PROFILE_IDS.contains(&profile) {
            return Err("profil tidak dikenal".into());
        }
    }

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, path: &str) -> ProjectInput {
        ProjectInput {
            name: name.into(),
            path: path.into(),
            default_profile: None,
            color: None,
            sort_order: 0,
        }
    }

    fn existing(id: &str, path: &str) -> ProjectRow {
        ProjectRow {
            id: id.into(),
            name: "saved".into(),
            path: path.into(),
            default_profile: None,
            color: None,
            sort_order: 0,
            last_used_ms: None,
        }
    }

    #[test]
    fn expand_tilde_handles_bare_and_prefixed() {
        assert_eq!(expand_tilde("~", "/home/y"), "/home/y");
        assert_eq!(expand_tilde("~/Dev/x", "/home/y"), "/home/y/Dev/x");
        assert_eq!(expand_tilde("~other/x", "/home/y"), "~other/x");
        assert_eq!(expand_tilde("/abs", "/home/y"), "/abs");
    }

    #[test]
    fn normalise_path_drops_trailing_and_duplicate_slashes() {
        assert_eq!(normalise_path("/a/b//c/", "/home/y"), "/a/b/c");
        assert_eq!(normalise_path("/", "/home/y"), "/");
        assert_eq!(normalise_path("  /a/b  ", "/home/y"), "/a/b");
        assert_eq!(normalise_path("~/Dev//x/", "/home/y"), "/home/y/Dev/x");
    }

    #[test]
    fn display_name_uses_last_segment_and_tilde() {
        assert_eq!(display_name("/Users/yolk/Dev/kirimi", "/unused"), "kirimi");
        assert_eq!(display_name("/Users/yolk", "/Users/yolk"), "~");
    }

    #[test]
    fn validate_rejects_empty_and_long_name() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        let dir = tmp.path().join("repo");
        std::fs::create_dir(&dir).unwrap();
        let path = dir.to_str().unwrap();

        let mut blank = input("   ", path);
        assert_eq!(validate(&blank, home, &[], None), Err("nama tidak boleh kosong".into()));

        blank.name = "x".repeat(61);
        assert_eq!(validate(&blank, home, &[], None), Err("nama terlalu panjang".into()));

        blank.name = "a".repeat(60);
        assert!(validate(&blank, home, &[], None).is_ok());
    }

    #[test]
    fn validate_rejects_relative_path() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        assert_eq!(
            validate(&input("x", "Dev/x"), home, &[], None),
            Err("path harus absolut".into())
        );
    }

    #[test]
    fn validate_rejects_missing_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        let missing = tmp.path().join("gone");
        assert_eq!(
            validate(&input("x", missing.to_str().unwrap()), home, &[], None),
            Err("folder tidak ditemukan".into())
        );
    }

    #[test]
    fn validate_rejects_a_file_path() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        let file = tmp.path().join("a.txt");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(
            validate(&input("x", file.to_str().unwrap()), home, &[], None),
            Err("folder tidak ditemukan".into())
        );
    }

    #[test]
    fn validate_rejects_duplicate_path() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        let dir = tmp.path().join("repo");
        std::fs::create_dir(&dir).unwrap();
        let path = dir.to_str().unwrap();

        let rows = vec![existing("other", path)];
        assert_eq!(
            validate(&input("x", path), home, &rows, None),
            Err("project dengan folder ini sudah ada".into())
        );
        // Trailing slash and a tilde-free spelling normalise to the same path.
        let sloppy = format!("{path}//");
        assert_eq!(
            validate(&input("x", &sloppy), home, &rows, None),
            Err("project dengan folder ini sudah ada".into())
        );
    }

    #[test]
    fn validate_allows_same_path_when_editing_that_row() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        let dir = tmp.path().join("repo");
        std::fs::create_dir(&dir).unwrap();
        let path = dir.to_str().unwrap();
        let rows = vec![existing("mine", path)];

        assert_eq!(validate(&input("x", path), home, &rows, Some("mine")), Ok(path.to_string()));
        assert_eq!(
            validate(&input("x", path), home, &rows, Some("other")),
            Err("project dengan folder ini sudah ada".into())
        );
    }

    #[test]
    fn validate_rejects_unknown_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        let dir = tmp.path().join("repo");
        std::fs::create_dir(&dir).unwrap();
        let path = dir.to_str().unwrap();

        let mut bad = input("x", path);
        bad.default_profile = Some("emacs".into());
        assert_eq!(validate(&bad, home, &[], None), Err("profil tidak dikenal".into()));

        let mut good = input("x", path);
        good.default_profile = Some("claude".into());
        assert_eq!(validate(&good, home, &[], None), Ok(path.to_string()));
    }
}
