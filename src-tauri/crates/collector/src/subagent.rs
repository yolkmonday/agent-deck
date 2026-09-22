use crate::model::TokenUsage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

const JSONL_SUFFIX: &str = ".jsonl";
const META_SUFFIX: &str = ".meta.json";
const PREFIX: &str = "agent-";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubAgentMeta {
    pub agent_type: String,
    pub description: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubAgentScan {
    pub id: String,
    pub jsonl: PathBuf,
    pub meta_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubAgent {
    pub id: String,
    pub agent_type: String,
    pub description: String,
    pub model: Option<String>,
    pub tokens: TokenUsage,
    pub cost_usd: f64,
    pub priced: bool,
    pub started_ms: Option<i64>,
}

/// `"agent-a357b1.jsonl"` -> `"a357b1"`. A meta file, a foreign name, or an
/// empty id all yield `None` so a stray file can never become an entry.
pub fn agent_id_from(file_name: &str) -> Option<String> {
    let rest = file_name.strip_prefix(PREFIX)?;
    let id = rest.strip_suffix(JSONL_SUFFIX)?;
    if id.is_empty() || id.contains(".meta") {
        return None;
    }
    Some(id.to_string())
}

pub fn read_meta(path: &Path) -> Option<SubAgentMeta> {
    let raw = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    let text = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    Some(SubAgentMeta {
        agent_type: text("agentType"),
        description: text("description"),
        model: v.get("model").and_then(Value::as_str).map(str::to_string),
    })
}

/// Lists the subagent transcripts in a session directory. A missing directory is
/// normal: a session that never dispatched one has no `subagents/` at all.
pub fn scan(session_dir: &Path) -> Vec<SubAgentScan> {
    let dir = session_dir.join("subagents");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<SubAgentScan> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let id = agent_id_from(&name)?;
            let jsonl = e.path();
            let meta_path = dir.join(format!("{PREFIX}{id}{META_SUFFIX}"));
            Some(SubAgentScan { id, jsonl, meta_path })
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn agent_id_from_parses_the_filename() {
        assert_eq!(agent_id_from("agent-a357b1.jsonl").as_deref(), Some("a357b1"));
        assert_eq!(agent_id_from("agent-x.meta.json"), None);
        assert_eq!(agent_id_from("notes.jsonl"), None);
        assert_eq!(agent_id_from("agent-.jsonl"), None);
    }

    #[test]
    fn scan_lists_only_jsonl_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("subagents");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("agent-a1.jsonl"), "").unwrap();
        fs::write(sub.join("agent-a2.jsonl"), "").unwrap();
        fs::write(sub.join("agent-a1.meta.json"), "{}").unwrap();
        fs::write(sub.join("agent-a2.meta.json"), "{}").unwrap();

        let found = scan(tmp.path());
        assert_eq!(found.len(), 2);
        let ids: Vec<&str> = found.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["a1", "a2"]);
        assert_eq!(found[0].jsonl, sub.join("agent-a1.jsonl"));
        assert_eq!(found[0].meta_path, sub.join("agent-a1.meta.json"));
    }

    #[test]
    fn scan_on_a_missing_directory_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(scan(tmp.path()).is_empty());
    }

    #[test]
    fn read_meta_reads_the_real_shape() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("agent-a1.meta.json");
        fs::write(
            &p,
            r#"{"agentType":"scout","description":"Research Iconify icon names","model":"sonnet","spawnDepth":1,"requestShape":"background"}"#,
        )
        .unwrap();
        let m = read_meta(&p).unwrap();
        assert_eq!(m.agent_type, "scout");
        assert_eq!(m.description, "Research Iconify icon names");
        assert_eq!(m.model.as_deref(), Some("sonnet"));
    }

    #[test]
    fn read_meta_tolerates_missing_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("agent-a1.meta.json");
        fs::write(&p, "{}").unwrap();
        let m = read_meta(&p).unwrap();
        assert_eq!(m.agent_type, "");
        assert_eq!(m.description, "");
        assert_eq!(m.model, None);
    }

    #[test]
    fn read_meta_on_invalid_json_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("agent-a1.meta.json");
        fs::write(&p, "not json").unwrap();
        assert!(read_meta(&p).is_none());
    }
}
