//! Pure performance math for model responses: noise filter, TPS, percentiles,
//! model-family normalization and per-model aggregation.
use std::collections::BTreeMap;

use crate::model::Agent;
use crate::store::PerfRow;

pub const MIN_OUTPUT_TOKENS: i64 = 20;
pub const MIN_GEN_MS: i64 = 200;
pub const MAX_GEN_MS: i64 = 1_800_000;
pub const LOW_SAMPLES: usize = 5;

pub fn keep_sample(r: &PerfRow) -> bool {
    r.output_tokens >= MIN_OUTPUT_TOKENS && r.gen_ms >= MIN_GEN_MS && r.gen_ms <= MAX_GEN_MS
}

pub fn tps(r: &PerfRow) -> f64 {
    r.output_tokens as f64 / (r.gen_ms as f64 / 1000.0)
}

/// Nearest-rank percentile over an ascending, non-empty slice.
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    let rank = ((p / 100.0) * n as f64).ceil().max(1.0) as usize;
    sorted[rank.min(n) - 1]
}

/// `aki/cbai/deepseek-v4.1-flash(high)` -> `deepseek-v4-1-flash`.
pub fn family(model: &str) -> String {
    let base = model.rsplit('/').next().unwrap_or(model);
    let base = base.split(['(', ':']).next().unwrap_or(base).trim();
    let chars: Vec<char> = base.to_lowercase().chars().collect();
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let between_digits = *c == '.'
                && i > 0
                && chars[i - 1].is_ascii_digit()
                && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
            if between_digits {
                '-'
            } else {
                *c
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyTps {
    pub day: String,
    pub tps_p50: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfAgg {
    pub key: String,
    pub agent: Agent,
    pub models: Vec<String>,
    pub samples: usize,
    pub tps_p50: f64,
    pub tps_p10: f64,
    pub ttft_p50_ms: Option<i64>,
    pub precise: bool,
    pub daily: Vec<DailyTps>,
    pub children: Vec<PerfAgg>,
}

fn utc_day(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    // civil-from-days (Howard Hinnant), avoids a date crate dependency
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn sorted_tps(rows: &[&PerfRow]) -> Vec<f64> {
    let mut v: Vec<f64> = rows.iter().map(|r| tps(r)).collect();
    v.sort_by(|a, b| a.total_cmp(b));
    v
}

fn build(key: String, rows: &[&PerfRow]) -> PerfAgg {
    let all = sorted_tps(rows);
    let mut ttft: Vec<i64> = rows.iter().filter_map(|r| r.ttft_ms).collect();
    ttft.sort_unstable();
    let ttft_p50_ms = if ttft.is_empty() {
        None
    } else {
        let f: Vec<f64> = ttft.iter().map(|v| *v as f64).collect();
        Some(percentile(&f, 50.0) as i64)
    };
    let mut by_day: BTreeMap<String, Vec<&PerfRow>> = BTreeMap::new();
    for r in rows {
        by_day.entry(utc_day(r.end_ms)).or_default().push(r);
    }
    let daily = by_day
        .into_iter()
        .map(|(day, rs)| DailyTps {
            day,
            tps_p50: percentile(&sorted_tps(&rs), 50.0),
        })
        .collect();
    let mut models: Vec<String> = rows.iter().map(|r| r.model.clone()).collect();
    models.sort();
    models.dedup();
    PerfAgg {
        key,
        agent: rows[0].agent,
        models,
        samples: rows.len(),
        tps_p50: percentile(&all, 50.0),
        tps_p10: percentile(&all, 10.0),
        ttft_p50_ms,
        precise: rows.iter().all(|r| r.precise),
        daily,
        children: Vec::new(),
    }
}

fn by_model(rows: &[&PerfRow]) -> Vec<PerfAgg> {
    let mut groups: BTreeMap<(String, String), Vec<&PerfRow>> = BTreeMap::new();
    for r in rows {
        groups
            .entry((format!("{:?}", r.agent), r.model.clone()))
            .or_default()
            .push(r);
    }
    let mut out: Vec<PerfAgg> = groups
        .into_iter()
        .map(|((_, m), rs)| build(m, &rs))
        .collect();
    out.sort_by(|a, b| b.tps_p50.total_cmp(&a.tps_p50));
    out
}

pub fn aggregate(rows: &[PerfRow], group_by_family: bool) -> Vec<PerfAgg> {
    let kept: Vec<&PerfRow> = rows.iter().filter(|r| keep_sample(r)).collect();
    if !group_by_family {
        return by_model(&kept);
    }
    let mut fams: BTreeMap<String, Vec<&PerfRow>> = BTreeMap::new();
    for r in &kept {
        fams.entry(family(&r.model)).or_default().push(r);
    }
    let mut out: Vec<PerfAgg> = fams
        .into_iter()
        .map(|(f, rs)| {
            let mut agg = build(f, &rs);
            agg.children = by_model(&rs);
            agg
        })
        .collect();
    out.sort_by(|a, b| b.tps_p50.total_cmp(&a.tps_p50));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Agent;
    use crate::store::PerfRow;

    fn s(model: &str, end: i64, gen: i64, out: i64, ttft: Option<i64>, precise: bool) -> PerfRow {
        PerfRow {
            id: format!("{model}:{end}"),
            agent: Agent::Opencode,
            session_id: "s".into(),
            model: model.into(),
            start_ms: end - gen,
            end_ms: end,
            gen_ms: gen,
            output_tokens: out,
            ttft_ms: ttft,
            precise,
        }
    }

    #[test]
    fn noise_filter() {
        assert!(!keep_sample(&s("m", 10_000, 1_000, 19, None, true)));
        assert!(!keep_sample(&s("m", 10_000, 199, 100, None, true)));
        assert!(!keep_sample(&s(
            "m", 10_000_000, 1_800_001, 100, None, true
        )));
        assert!(keep_sample(&s("m", 10_000, 1_000, 20, None, true)));
    }

    #[test]
    fn tps_is_tokens_per_second() {
        assert_eq!(tps(&s("m", 10_000, 2_000, 100, None, true)), 50.0);
    }

    #[test]
    fn percentile_nearest_rank() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert_eq!(percentile(&v, 50.0), 5.0);
        assert_eq!(percentile(&v, 10.0), 1.0);
        assert_eq!(percentile(&v, 100.0), 10.0);
        assert_eq!(percentile(&[7.0], 10.0), 7.0);
    }

    #[test]
    fn family_normalizes_provider_suffix_and_dots() {
        assert_eq!(family("kn/deepseek-v4-1-flash"), "deepseek-v4-1-flash");
        assert_eq!(
            family("aki/cbai/deepseek-v4.1-flash(high)"),
            "deepseek-v4-1-flash"
        );
        assert_eq!(
            family("Sumo/deepseek-v4.1-flash:netra"),
            "deepseek-v4-1-flash"
        );
        assert_eq!(family("claude-opus-4-8"), "claude-opus-4-8");
    }

    #[test]
    fn family_keeps_distinct_models_apart() {
        assert_ne!(
            family("kn/deepseek-v4-pro"),
            family("kn/deepseek-v4-1-flash")
        );
        assert_ne!(family("oa/claude-opus-4.8"), family("oa/claude-opus-5"));
    }

    #[test]
    fn aggregate_uses_median_not_mean() {
        // tps: 10,10,10,10,1000 -> median 10
        let rows = vec![
            s("kn/a", 1_000, 2_000, 20, None, true),    // 10 tps
            s("kn/a", 2_000, 2_000, 20, None, true),    // 10
            s("kn/a", 3_000, 2_000, 20, None, true),    // 10
            s("kn/a", 4_000, 2_000, 20, None, true),    // 10
            s("kn/a", 5_000, 1_000, 1_000, None, true), // 1000
        ];
        let out = aggregate(&rows, false);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].tps_p50, 10.0);
        assert_eq!(out[0].tps_p10, 10.0);
        assert_eq!(out[0].samples, 5);
    }

    #[test]
    fn aggregate_drops_noise_and_reports_ttft_and_precision() {
        let rows = vec![
            s("kn/a", 1_000, 1_000, 100, Some(300), true),
            s("kn/a", 2_000, 1_000, 100, Some(500), true),
            s("kn/a", 3_000, 1_000, 5, Some(1), true), // noise: < 20 tokens
        ];
        let out = aggregate(&rows, false);
        assert_eq!(out[0].samples, 2);
        assert_eq!(out[0].ttft_p50_ms, Some(300));
        assert!(out[0].precise);
    }

    #[test]
    fn aggregate_family_groups_providers_with_children() {
        let rows = vec![
            s("kn/deepseek-v4-1-flash", 1_000, 1_000, 40, None, true),
            s(
                "aki/cbai/deepseek-v4.1-flash(high)",
                2_000,
                1_000,
                25,
                None,
                true,
            ),
            s("kn/deepseek-v4-pro", 3_000, 1_000, 12 + 20, None, true),
        ];
        let out = aggregate(&rows, true);
        let flash = out.iter().find(|r| r.key == "deepseek-v4-1-flash").unwrap();
        assert_eq!(flash.samples, 2);
        assert_eq!(flash.children.len(), 2);
        assert_eq!(flash.children[0].key, "kn/deepseek-v4-1-flash"); // 40 tps > 25 tps
        assert!(out.iter().any(|r| r.key == "deepseek-v4-pro"));
    }

    #[test]
    fn daily_buckets_by_utc_day() {
        let day1 = 1_790_000_000_000; // some UTC ms
        let rows = vec![
            s("kn/a", day1, 1_000, 100, None, true),
            s("kn/a", day1 + 86_400_000, 1_000, 50, None, true),
        ];
        let out = aggregate(&rows, false);
        assert_eq!(out[0].daily.len(), 2);
        assert_eq!(out[0].daily[0].tps_p50, 100.0);
        assert_eq!(out[0].daily[1].tps_p50, 50.0);
    }
}
