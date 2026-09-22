use collector::model::LiveSnapshot;
use collector::store::ProjectRow;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const PROFILE_IDS: [&str; 3] = ["claude", "opencode", "codex"];
pub const MAX_NAME_LEN: usize = 60;
const SUGGESTION_LIMIT: usize = 30;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInput {
    pub name: String,
    pub path: String,
    pub default_profile: Option<String>,
    pub color: Option<String>,
    pub sort_order: i64,
}

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

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub default_profile: Option<String>,
    pub color: Option<String>,
    pub sort_order: i64,
    pub last_used_ms: Option<i64>,
    pub exists: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSuggestion {
    pub name: String,
    pub path: String,
    pub source: &'static str,
    pub messages: i64,
}

/// `exists` is decided here, every read: a project whose folder was moved or
/// deleted must say so without anything having to invalidate a stored flag.
pub fn to_project(row: ProjectRow) -> Project {
    let exists = std::path::Path::new(&row.path).is_dir();
    Project {
        id: row.id,
        name: row.name,
        path: row.path,
        default_profile: row.default_profile,
        color: row.color,
        sort_order: row.sort_order,
        last_used_ms: row.last_used_ms,
        exists,
    }
}

/// Directories worth offering: the ones live sessions sit in, then the names in
/// the history index resolved back to a path. A name that cannot be resolved to a
/// directory that exists right now is skipped rather than guessed.
pub fn suggestions(
    live: &LiveSnapshot,
    known: &[(String, i64)],
    saved_paths: &HashSet<String>,
    home: &str,
) -> Vec<ProjectSuggestion> {
    let mut out: Vec<ProjectSuggestion> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let mut cwds: Vec<String> = Vec::new();
    for s in &live.sessions {
        let path = normalise_path(&s.cwd, home);
        if path.is_empty() || !path.starts_with('/') {
            continue;
        }
        if seen.insert(path.clone()) {
            cwds.push(path);
        }
    }

    for path in &cwds {
        if !std::path::Path::new(path).is_dir() {
            continue;
        }
        if saved_paths.contains(path) {
            continue;
        }
        out.push(ProjectSuggestion {
            name: display_name(path, home),
            path: path.clone(),
            source: "live",
            messages: 0,
        });
    }

    let mut history: Vec<ProjectSuggestion> = Vec::new();
    let mut history_seen: HashSet<String> = HashSet::new();
    for (name, messages) in known {
        if let Some(path) = resolve_name(name, &cwds, home) {
            if !history_seen.insert(path.clone()) {
                continue;
            }
            if saved_paths.contains(&path) {
                continue;
            }
            history.push(ProjectSuggestion {
                name: display_name(&path, home),
                path,
                source: "history",
                messages: *messages,
            });
        }
    }
    history.sort_by(|a, b| b.messages.cmp(&a.messages).then_with(|| a.path.cmp(&b.path)));
    out.extend(history);
    out.truncate(SUGGESTION_LIMIT);
    out
}

/// The history index stores a project NAME, not a directory, so the path has to be
/// recovered: first from a live session that reports it, then from the convention
/// `<home>/Dev/<name>`. Anything else is unresolvable.
fn resolve_name(name: &str, cwds: &[String], home: &str) -> Option<String> {
    if let Some(path) = cwds
        .iter()
        .find(|p| p.rsplit('/').next() == Some(name))
    {
        return Some(path.clone());
    }
    let candidate = normalise_path(&format!("{home}/Dev/{name}"), home);
    std::path::Path::new(&candidate)
        .is_dir()
        .then_some(candidate)
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
    fn validate_rejects_unknown_profile() {        let tmp = tempfile::tempdir().unwrap();
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

    fn live_with(cwds: &[&str]) -> LiveSnapshot {
        LiveSnapshot {
            sessions: cwds
                .iter()
                .enumerate()
                .map(|(i, cwd)| collector::model::Session {
                    id: format!("s{i}"),
                    agent: collector::model::Agent::Claude,
                    pid: None,
                    project: "p".into(),
                    cwd: (*cwd).into(),
                    model: None,
                    branch: None,
                    status: collector::model::Status::Idle,
                    activity: None,
                    tokens: Default::default(),
                    cost_usd: 0.0,
                    priced: false,
                    started_at_ms: None,
                    updated_at_ms: 0,
                })
                .collect(),
            warnings: Vec::new(),
            generated_at_ms: 0,
            cost_usd: 0.0,
            unpriced: 0,
        }
    }

    #[test]
    fn project_wire_shape_matches_the_frontend_contract() {
        let row = ProjectRow {
            id: "p1".into(),
            name: "kirimi".into(),
            path: "/Users/yolk/Dev/kirimi".into(),
            default_profile: Some("claude".into()),
            color: Some("#4C9AFF".into()),
            sort_order: 3,
            last_used_ms: Some(1_700_000_000_000),
        };
        let v = serde_json::to_value(to_project(row)).unwrap();
        assert_eq!(v["id"], "p1");
        assert_eq!(v["defaultProfile"], "claude");
        assert_eq!(v["sortOrder"], 3);
        assert_eq!(v["lastUsedMs"], 1_700_000_000_000i64);
        assert_eq!(v["exists"], true);

        let v = serde_json::to_value(ProjectSuggestion {
            name: "kirimi".into(),
            path: "/x".into(),
            source: "history",
            messages: 7,
        })
        .unwrap();
        assert_eq!(v["source"], "history");
        assert_eq!(v["messages"], 7);
    }

    #[test]
    fn to_project_decides_exists_at_read_time() {
        let tmp = tempfile::tempdir().unwrap();
        let row = existing("a", tmp.path().to_str().unwrap());
        assert!(to_project(row.clone()).exists);
        std::fs::remove_dir(tmp.path()).unwrap();
        assert!(!to_project(row).exists);
    }

    #[test]
    fn suggestions_lead_with_live_then_history_by_count() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        let busy = tmp.path().join("Dev/busy");
        std::fs::create_dir_all(&busy).unwrap();
        let live_dir = tmp.path().join("work/live");
        std::fs::create_dir_all(&live_dir).unwrap();

        let live = live_with(&[live_dir.to_str().unwrap()]);
        let known = vec![("busy".to_string(), 9), ("gone".to_string(), 50)];
        let out = suggestions(&live, &known, &HashSet::new(), home);

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].source, "live");
        assert_eq!(out[0].path, live_dir.to_str().unwrap());
        assert_eq!(out[0].messages, 0);
        assert_eq!(out[1].source, "history");
        assert_eq!(out[1].name, "busy");
        assert_eq!(out[1].messages, 9);
    }

    #[test]
    fn suggestions_skip_saved_paths_and_unresolvable_names() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        let live_dir = tmp.path().join("work/live");
        std::fs::create_dir_all(&live_dir).unwrap();

        let live = live_with(&[live_dir.to_str().unwrap()]);
        let saved: HashSet<String> = [live_dir.to_string_lossy().to_string()].into_iter().collect();
        let out = suggestions(&live, &[("ghost-name".to_string(), 3)], &saved, home);
        assert!(out.is_empty(), "unexpected suggestions: {out:?}");
    }

    #[test]
    fn suggestions_resolve_a_history_name_from_a_live_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        let elsewhere = tmp.path().join("mnt/code/kirimi");
        std::fs::create_dir_all(&elsewhere).unwrap();

        let live = live_with(&[elsewhere.to_str().unwrap()]);
        let out = suggestions(&live, &[("kirimi".to_string(), 4)], &HashSet::new(), home);
        // Live offers the directory under its real path, and read-only history must
        // still count it in: the caller excludes it via the saved-path set instead.
        assert_eq!(out.len(), 2, "unexpected suggestions: {out:?}");
        assert_eq!(out[0].source, "live");
        assert_eq!(out[0].path, elsewhere.to_str().unwrap());
        assert_eq!(out[1].source, "history");
        assert_eq!(out[1].path, elsewhere.to_str().unwrap());
        assert_eq!(out[1].messages, 4);
    }

    #[test]
    fn suggestions_cap_at_thirty() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        let live_dir = tmp.path().join("work/live");
        std::fs::create_dir_all(&live_dir).unwrap();
        // `<home>/Dev/<name>` is the one place a history name can be recovered from,
        // so the fixtures have to live there for the cap to be exercised.
        let known: Vec<(String, i64)> = (0..40)
            .map(|i| {
                std::fs::create_dir_all(tmp.path().join(format!("Dev/dev{i}"))).unwrap();
                (format!("dev{i}"), 40 - i as i64)
            })
            .collect();

        let live = live_with(&[live_dir.to_str().unwrap()]);
        let out = suggestions(&live, &known, &HashSet::new(), home);
        assert_eq!(out.len(), SUGGESTION_LIMIT);
        assert_eq!(out[0].source, "live");
        assert_eq!(out[1].messages, 40);
    }
}
