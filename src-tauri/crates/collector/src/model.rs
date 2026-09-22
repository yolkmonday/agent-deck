use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Opencode,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Busy,
    Waiting,
    Idle,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
}

impl TokenUsage {
    /// `reasoning` is informational and may already be included in `output`.
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }

    pub fn add(&mut self, o: &TokenUsage) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
        self.reasoning += o.reasoning;
    }

    pub fn sub(&mut self, o: &TokenUsage) {
        self.input = self.input.saturating_sub(o.input);
        self.output = self.output.saturating_sub(o.output);
        self.cache_read = self.cache_read.saturating_sub(o.cache_read);
        self.cache_write = self.cache_write.saturating_sub(o.cache_write);
        self.reasoning = self.reasoning.saturating_sub(o.reasoning);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityKind {
    Tool,
    Waiting,
    Thinking,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub kind: ActivityKind,
    pub label: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub agent: Agent,
    pub pid: Option<u32>,
    pub project: String,
    pub cwd: String,
    pub model: Option<String>,
    pub branch: Option<String>,
    pub status: Status,
    pub activity: Option<Activity>,
    pub tokens: TokenUsage,
    pub started_at_ms: Option<i64>,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSnapshot {
    pub sessions: Vec<Session>,
    pub warnings: Vec<String>,
    pub generated_at_ms: i64,
}

impl LiveSnapshot {
    pub fn same_content(&self, other: &Self) -> bool {
        self.sessions == other.sessions && self.warnings == other.warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(i: u64, o: u64, cr: u64, cw: u64) -> TokenUsage {
        TokenUsage { input: i, output: o, cache_read: cr, cache_write: cw, reasoning: 0 }
    }

    #[test]
    fn total_excludes_reasoning() {
        let u = TokenUsage { input: 1, output: 2, cache_read: 3, cache_write: 4, reasoning: 99 };
        assert_eq!(u.total(), 10);
    }

    #[test]
    fn add_and_saturating_sub() {
        let mut u = usage(1, 2, 3, 4);
        u.add(&usage(10, 20, 30, 40));
        assert_eq!(u, usage(11, 22, 33, 44));
        u.sub(&usage(100, 0, 0, 0));
        assert_eq!(u.input, 0);
    }

    #[test]
    fn session_serializes_camel_case_with_lowercase_enums() {
        let s = Session {
            id: "s1".into(), agent: Agent::Claude, pid: Some(1), project: "p".into(), cwd: "/p".into(),
            model: None, branch: None, status: Status::Waiting, activity: None,
            tokens: TokenUsage::default(), started_at_ms: None, updated_at_ms: 5,
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["agent"], "claude");
        assert_eq!(v["status"], "waiting");
        assert_eq!(v["updatedAtMs"], 5);
        assert_eq!(v["tokens"]["cacheRead"], 0);
    }

    #[test]
    fn same_content_ignores_timestamp() {
        let a = LiveSnapshot { sessions: vec![], warnings: vec![], generated_at_ms: 1 };
        let b = LiveSnapshot { sessions: vec![], warnings: vec![], generated_at_ms: 2 };
        assert!(a.same_content(&b));
        let c = LiveSnapshot { sessions: vec![], warnings: vec!["x".into()], generated_at_ms: 2 };
        assert!(!a.same_content(&c));
    }
}
