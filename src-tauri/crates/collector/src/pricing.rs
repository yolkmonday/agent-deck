use crate::model::TokenUsage;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceEntry {
    pub model: String,
    pub input_per_m: f64,
    pub output_per_m: f64,
    pub cache_read_per_m: f64,
    pub cache_write_per_m: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriceTable {
    entries: Vec<PriceEntry>,
}

fn default_entry(model: &str, input: f64, output: f64, cache_read: f64, cache_write: f64) -> PriceEntry {
    PriceEntry {
        model: model.to_string(),
        input_per_m: input,
        output_per_m: output,
        cache_read_per_m: cache_read,
        cache_write_per_m: cache_write,
    }
}

impl PriceTable {
    pub fn defaults() -> PriceTable {
        PriceTable {
            entries: vec![
                default_entry("claude-opus-5", 15.0, 75.0, 1.5, 18.75),
                default_entry("claude-sonnet-5", 3.0, 15.0, 0.3, 3.75),
                default_entry("claude-haiku-4-5", 1.0, 5.0, 0.1, 1.25),
                default_entry("claude-fable-5", 3.0, 15.0, 0.3, 3.75),
                default_entry("gpt-5.4", 2.5, 10.0, 0.25, 3.125),
                default_entry("deepseek-v4-1-flash", 0.27, 1.1, 0.027, 0.34),
                default_entry("deepseek-v4-pro", 0.55, 2.2, 0.055, 0.69),
            ],
        }
    }

    pub fn load(path: &Path) -> PriceTable {
        let defaults = PriceTable::defaults();
        let Ok(raw) = std::fs::read_to_string(path) else {
            return defaults;
        };
        let Ok(entries) = serde_json::from_str::<Vec<PriceEntry>>(&raw) else {
            return defaults;
        };
        let mut merged = entries.clone();
        for d in defaults.entries {
            if !merged.iter().any(|e| e.model == d.model) {
                merged.push(d);
            }
        }
        PriceTable { entries: merged }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&self.entries)?)?;
        Ok(())
    }

    pub fn entries(&self) -> &[PriceEntry] {
        &self.entries
    }

    pub fn set(&mut self, entries: Vec<PriceEntry>) {
        self.entries = entries;
    }

    fn find(&self, model: &str) -> Option<&PriceEntry> {
        if let Some(e) = self.entries.iter().find(|e| e.model == model) {
            return Some(e);
        }
        let bare = model.rsplit('/').next().unwrap_or(model);
        if bare != model {
            if let Some(e) = self.entries.iter().find(|e| e.model == bare) {
                return Some(e);
            }
        }
        [model, bare]
            .iter()
            .filter_map(|m| {
                self.entries
                    .iter()
                    .filter(|e| !e.model.is_empty() && m.starts_with(&e.model))
                    .max_by_key(|e| e.model.len())
            })
            .next()
    }

    pub fn cost_usd(&self, model: &str, u: &TokenUsage) -> f64 {
        let Some(e) = self.find(model) else {
            return 0.0;
        };
        (u.input as f64 * e.input_per_m
            + u.output as f64 * e.output_per_m
            + u.cache_read as f64 * e.cache_read_per_m
            + u.cache_write as f64 * e.cache_write_per_m)
            / 1_000_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: u64, output: u64, cache_read: u64, cache_write: u64, reasoning: u64) -> TokenUsage {
        TokenUsage { input, output, cache_read, cache_write, reasoning }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn defaults_price_known_models() {
        let t = PriceTable::defaults();
        let u = usage(1_000_000, 0, 0, 0, 0);
        assert!(close(t.cost_usd("claude-sonnet-5", &u), 3.0));
    }

    #[test]
    fn cost_sums_all_four_buckets() {
        let t = PriceTable::defaults();
        let u = usage(1_000_000, 1_000_000, 1_000_000, 1_000_000, 9_999_999);
        assert!(close(t.cost_usd("claude-sonnet-5", &u), 22.05));
    }

    #[test]
    fn provider_prefix_is_stripped() {
        let t = PriceTable::defaults();
        let u = usage(1_000_000, 1_000_000, 1_000_000, 1_000_000, 0);
        assert!(close(
            t.cost_usd("kn/deepseek-v4-1-flash", &u),
            t.cost_usd("deepseek-v4-1-flash", &u)
        ));
        assert!(t.cost_usd("kn/deepseek-v4-1-flash", &u) > 0.0);
    }

    #[test]
    fn prefix_match_picks_longest_entry() {
        let mut t = PriceTable::defaults();
        t.set(vec![
            default_entry("claude", 100.0, 0.0, 0.0, 0.0),
            default_entry("claude-sonnet", 1.0, 0.0, 0.0, 0.0),
        ]);
        let u = usage(1_000_000, 0, 0, 0, 0);
        assert!(close(t.cost_usd("claude-sonnet-5", &u), 1.0));
    }

    #[test]
    fn unknown_model_costs_zero() {
        let t = PriceTable::defaults();
        assert!(close(t.cost_usd("totally-unknown", &usage(1_000_000, 1_000_000, 0, 0, 0)), 0.0));
    }

    #[test]
    fn save_then_load_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("pricing.json");
        let mut t = PriceTable::defaults();
        t.set(vec![default_entry("custom-model", 1.5, 2.5, 0.15, 0.25)]);
        t.save(&path).unwrap();

        let loaded = PriceTable::load(&path);
        let custom = loaded
            .entries()
            .iter()
            .find(|e| e.model == "custom-model")
            .expect("custom entry survives");
        assert!(close(custom.input_per_m, 1.5));
        assert!(close(custom.output_per_m, 2.5));
        assert!(close(custom.cache_read_per_m, 0.15));
        assert!(close(custom.cache_write_per_m, 0.25));
    }

    #[test]
    fn load_missing_file_returns_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = PriceTable::load(&tmp.path().join("nope.json"));
        assert_eq!(loaded.entries().len(), PriceTable::defaults().entries().len());
    }

    #[test]
    fn load_invalid_json_returns_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("pricing.json");
        std::fs::write(&path, "not json").unwrap();
        let loaded = PriceTable::load(&path);
        assert_eq!(loaded.entries().len(), PriceTable::defaults().entries().len());
    }
}
