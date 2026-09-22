use crate::model::Status;
use serde::{Deserialize, Serialize};

pub const DEFAULT_STALL_MS: i64 = 5 * 60 * 1000;
pub const DEFAULT_SLOW_TOOL_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Ok,
    Slow,
    Stalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub stall_ms: i64,
    pub slow_tool_ms: i64,
}

impl Thresholds {
    pub fn defaults() -> Thresholds {
        Thresholds {
            stall_ms: DEFAULT_STALL_MS,
            slow_tool_ms: DEFAULT_SLOW_TOOL_MS,
        }
    }
}

fn minutes_floor(ms: i64) -> i64 {
    ms.max(0) / 60_000
}

pub fn evaluate(
    status: Status,
    quiet_ms: i64,
    tool: Option<(&str, i64)>,
    t: &Thresholds,
) -> (Health, Option<String>) {
    if status == Status::Waiting {
        return (Health::Ok, None);
    }
    if let Some((name, running_ms)) = tool {
        if running_ms >= t.slow_tool_ms {
            return (
                Health::Slow,
                Some(format!("tool \"{name}\" berjalan {} mnt", minutes_floor(running_ms))),
            );
        }
        return (Health::Ok, None);
    }
    if status == Status::Busy && quiet_ms >= t.stall_ms {
        return (
            Health::Stalled,
            Some(format!("diam {} mnt tanpa tool berjalan", minutes_floor(quiet_ms))),
        );
    }
    (Health::Ok, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;

    #[test]
    fn waiting_is_never_stalled() {
        let (h, r) = evaluate(Status::Waiting, 60 * MIN, None, &Thresholds::defaults());
        assert_eq!(h, Health::Ok);
        assert_eq!(r, None);
    }

    #[test]
    fn idle_is_never_stalled() {
        let (h, r) = evaluate(Status::Idle, 60 * MIN, None, &Thresholds::defaults());
        assert_eq!(h, Health::Ok);
        assert_eq!(r, None);
    }

    #[test]
    fn busy_and_quiet_past_threshold_is_stalled() {
        let (h, r) = evaluate(Status::Busy, 6 * MIN, None, &Thresholds::defaults());
        assert_eq!(h, Health::Stalled);
        let r = r.unwrap();
        assert!(r.contains("6 mnt"), "{r}");
        assert!(r.contains("tanpa tool"), "{r}");
    }

    #[test]
    fn busy_and_quiet_below_threshold_is_ok() {
        let (h, r) = evaluate(Status::Busy, 4 * MIN, None, &Thresholds::defaults());
        assert_eq!(h, Health::Ok);
        assert_eq!(r, None);
    }

    #[test]
    fn open_tool_below_threshold_is_ok_even_when_quiet() {
        let (h, r) = evaluate(Status::Busy, 30 * MIN, Some(("Bash", 2 * MIN)), &Thresholds::defaults());
        assert_eq!(h, Health::Ok);
        assert_eq!(r, None);
    }

    #[test]
    fn open_tool_past_threshold_is_slow_not_stalled() {
        let (h, r) = evaluate(Status::Busy, 60 * MIN, Some(("Bash", 11 * MIN)), &Thresholds::defaults());
        assert_eq!(h, Health::Slow);
        let r = r.unwrap();
        assert!(r.contains("Bash"), "{r}");
        assert!(r.contains("11 mnt"), "{r}");
    }

    #[test]
    fn custom_thresholds_are_respected() {
        let t = Thresholds { stall_ms: 60_000, slow_tool_ms: 10 * MIN };
        let (h, _) = evaluate(Status::Busy, 90_000, None, &t);
        assert_eq!(h, Health::Stalled);
    }

    #[test]
    fn reason_rounds_minutes_down() {
        let (_, r) = evaluate(Status::Busy, 6 * MIN + 59_000, None, &Thresholds::defaults());
        assert!(r.unwrap().contains("6 mnt"));
    }
}
