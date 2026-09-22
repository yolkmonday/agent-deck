use serde::{Deserialize, Serialize};
use std::path::Path;
use time::{Date, Month};

pub type DateYear = i32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BillingMode {
    Subscription,
    Prepaid,
    Payg,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingAccount {
    pub id: String,
    pub label: String,
    pub mode: BillingMode,
    /// Model-id prefixes this account covers. The longest match wins.
    pub matches: Vec<String>,
    pub monthly_usd: Option<f64>,
    pub renewal_day: Option<u32>,
    pub credit_usd: Option<f64>,
    pub started_on: Option<String>,
    pub expires_on: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BillingTable {
    accounts: Vec<BillingAccount>,
}

fn parse_date(s: &str) -> Option<Date> {
    let (y, m, d) = {
        let mut it = s.split('-');
        let y = it.next()?.parse::<DateYear>().ok()?;
        let m = it.next()?.parse::<u8>().ok()?;
        let d = it.next()?.parse::<u8>().ok()?;
        if it.next().is_some() {
            return None;
        }
        (y, m, d)
    };
    let month = Month::try_from(m).ok()?;
    Date::from_calendar_date(y, month, d).ok()
}

/// The wire form of a cycle boundary: `YYYY-MM-DD`, local calendar days.
pub fn format_date(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

/// Parses the `YYYY-MM-DD` form back into a calendar day.
pub fn parse_date_iso(s: &str) -> Option<Date> {
    parse_date(s)
}

/// The local calendar day an instant falls on, as a `Date`. The offset is taken
/// from the system clock (`local-offset`), which is the same wall clock the rest
/// of the app uses for daily buckets.
pub fn local_date(now_ms: i64) -> Date {
    let secs = now_ms.div_euclid(1000);
    let dt = time::OffsetDateTime::from_unix_timestamp(secs)
        .ok()
        .map(|t| t.to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC)))
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    dt.date()
}

/// Midnight local time on `d`, as milliseconds since the epoch. Used to turn a
/// cycle boundary into a query window.
pub fn start_of_day_ms(d: Date) -> i64 {
    (d - Date::from_calendar_date(1970, Month::January, 1).expect("epoch"))
        .whole_days()
        * 86_400_000
}

/// A renewal day above 28 is pushed back to 28 rather than clamped per month, so
/// every month has the day and no cycle is skipped.
fn renewal_day(account: &BillingAccount) -> u32 {
    account.renewal_day.unwrap_or(1).clamp(1, 28)
}

fn day_in(year: DateYear, month: Month, day: u32) -> Option<Date> {
    Date::from_calendar_date(year, month, day as u8).ok()
}

/// The date `months` months away that keeps `day` when the target month has it,
/// and otherwise moves to the following day. `renewal_day` is capped at 28, so
/// the fallback only ever applies to a caller that skips the cap.
fn month_shift(d: Date, months: i32, day: u32) -> Date {
    let (year, month) = shift_year_month(d.year(), d.month(), months);
    day_in(year, month, day)
        .or_else(|| day_in(year, month, day.saturating_add(1)))
        .unwrap_or(d)
}

fn shift_year_month(year: DateYear, month: Month, months: i32) -> (DateYear, Month) {
    let base = u8::from(month) as i32 - 1 + months;
    let new_year = year + base.div_euclid(12);
    let new_month = (base.rem_euclid(12) + 1) as u8;
    (new_year, Month::try_from(new_month).unwrap_or(Month::January))
}

/// The most recent occurrence of `day` in `month` at or before `as_of`, scanning
/// back month by month. A capped `day` always exists, so the scan is short.
fn last_day_on_or_before(as_of: Date, day: u32) -> Date {
    let current = day_in(as_of.year(), as_of.month(), day);
    match current {
        Some(c) if c <= as_of => c,
        _ => month_shift(
            day_in(as_of.year(), as_of.month(), 15).unwrap_or(as_of),
            -1,
            day,
        ),
    }
}

fn next_day_after(as_of: Date, day: u32) -> Date {
    match day_in(as_of.year(), as_of.month(), day) {
        Some(c) if c > as_of => c,
        _ => month_shift(
            day_in(as_of.year(), as_of.month(), 15).unwrap_or(as_of),
            1,
            day,
        ),
    }
}

/// The account's own current cycle, inclusive on both ends.
pub fn cycle_for(account: &BillingAccount, as_of: Date) -> (Date, Date) {
    match account.mode {
        BillingMode::Subscription => {
            let day = renewal_day(account);
            let start = last_day_on_or_before(as_of, day);
            let next = next_day_after(start, day);
            (start, next - time::Duration::days(1))
        }
        BillingMode::Prepaid => {
            let start = account
                .started_on
                .as_deref()
                .and_then(parse_date)
                .unwrap_or(as_of);
            let end = account
                .expires_on
                .as_deref()
                .and_then(parse_date)
                .unwrap_or(as_of);
            (start, end.max(start))
        }
        BillingMode::Payg => {
            let start = day_in(as_of.year(), as_of.month(), 1).unwrap_or(as_of);
            let (year, month) = shift_year_month(start.year(), start.month(), 1);
            let next = day_in(year, month, 1).unwrap_or(start);
            (start, next - time::Duration::days(1))
        }
    }
}

/// Days to the next renewal for a subscription, to the expiry for prepaid, and
/// `None` for pay-as-you-go. Negative means the date has already passed.
pub fn days_left(account: &BillingAccount, as_of: Date) -> Option<i64> {
    match account.mode {
        BillingMode::Subscription => {
            let day = renewal_day(account);
            let next = next_day_after(as_of, day);
            Some((next - as_of).whole_days())
        }
        BillingMode::Prepaid => {
            let expiry = account.expires_on.as_deref().and_then(parse_date)?;
            Some((expiry - as_of).whole_days())
        }
        BillingMode::Payg => None,
    }
}

/// The bare model id, with any provider prefix removed: `kn/deepseek` -> `deepseek`.
fn bare_model(model: &str) -> &str {
    model.rsplit('/').next().unwrap_or(model)
}

impl BillingTable {
    pub fn new(accounts: Vec<BillingAccount>) -> BillingTable {
        BillingTable { accounts }
    }

    /// An empty list: the user's plans and prices are not ours to invent.
    pub fn defaults() -> BillingTable {
        BillingTable { accounts: Vec::new() }
    }

    pub fn load(path: &Path) -> BillingTable {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return BillingTable::defaults();
        };
        match serde_json::from_str::<Vec<BillingAccount>>(&raw) {
            Ok(accounts) => BillingTable { accounts },
            Err(_) => BillingTable::defaults(),
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&self.accounts)?)?;
        Ok(())
    }

    pub fn accounts(&self) -> &[BillingAccount] {
        &self.accounts
    }

    pub fn set(&mut self, accounts: Vec<BillingAccount>) {
        self.accounts = accounts;
    }

    /// The account whose longest `matches` prefix covers the model. A prefix only
    /// matches at the start of the id, either the full one or the bare one, so
    /// `kn/deepseek-v4-1-flash` can match `kn/` or `deepseek-`.
    pub fn match_account(&self, model: &str) -> Option<&BillingAccount> {
        if model.is_empty() {
            return None;
        }
        let bare = bare_model(model);
        self.accounts
            .iter()
            .flat_map(|a| {
                a.matches.iter().filter_map(move |p| {
                    if p.is_empty() {
                        return None;
                    }
                    if model.starts_with(p) || bare.starts_with(p) {
                        Some((p.len(), a))
                    } else {
                        None
                    }
                })
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, a)| a)
    }
}

/// The price of a message at API rates, and the part of it that actually leaves
/// the bank account. A subscription saves `notional` in full, so its spend is
/// always zero — the month costs whatever the plan costs, never more.
pub fn split_cost(account: Option<&BillingAccount>, notional: f64) -> (f64, f64) {
    match account.map(|a| a.mode) {
        Some(BillingMode::Subscription) => (0.0, notional),
        // Prepaid credit is consumed at API rates, so it moves money like payg.
        Some(BillingMode::Prepaid) | Some(BillingMode::Payg) | None => (notional, notional),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(mode: BillingMode, matches: &[&str]) -> BillingAccount {
        BillingAccount {
            id: "a1".into(),
            label: "Claude Max".into(),
            mode,
            matches: matches.iter().map(|s| s.to_string()).collect(),
            monthly_usd: Some(200.0),
            renewal_day: Some(14),
            credit_usd: None,
            started_on: None,
            expires_on: None,
        }
    }

    fn date(s: &str) -> Date {
        parse_date(s).expect("valid test date")
    }

    #[test]
    fn match_picks_the_longest_prefix() {
        let table = BillingTable::new(vec![
            account(BillingMode::Payg, &["claude-"]),
            account(BillingMode::Subscription, &["claude-opus"]),
        ]);
        let hit = table.match_account("claude-opus-5").expect("matches");
        assert_eq!(hit.mode, BillingMode::Subscription);
        assert_eq!(hit.matches, vec!["claude-opus".to_string()]);
    }

    #[test]
    fn match_strips_the_provider_prefix() {
        let table = BillingTable::new(vec![account(BillingMode::Prepaid, &["deepseek-"])]);
        assert!(table.match_account("kn/deepseek-v4-1-flash").is_some());
        assert!(table.match_account("deepseek-v4-pro").is_some());
    }

    #[test]
    fn match_returns_none_for_an_unknown_model() {
        let table = BillingTable::new(vec![account(BillingMode::Payg, &["claude-"])]);
        assert!(table.match_account("totally-unknown").is_none());
        assert!(table.match_account("").is_none());
    }

    #[test]
    fn cycle_for_subscription_mid_month() {
        let a = account(BillingMode::Subscription, &["claude-"]);
        let (from, to) = cycle_for(&a, date("2026-03-20"));
        assert_eq!(format_date(from), "2026-03-14");
        assert_eq!(format_date(to), "2026-04-13");
    }

    #[test]
    fn cycle_for_subscription_before_the_renewal_day() {
        let a = account(BillingMode::Subscription, &["claude-"]);
        let (from, to) = cycle_for(&a, date("2026-03-03"));
        assert_eq!(format_date(from), "2026-02-14");
        assert_eq!(format_date(to), "2026-03-13");
    }

    #[test]
    fn cycle_for_subscription_handles_year_rollover() {
        let a = account(BillingMode::Subscription, &["claude-"]);
        let (from, to) = cycle_for(&a, date("2026-01-05"));
        assert_eq!(format_date(from), "2025-12-14");
        assert_eq!(format_date(to), "2026-01-13");
    }

    #[test]
    fn cycle_for_prepaid_uses_started_and_expires() {
        let mut a = account(BillingMode::Prepaid, &["kn/"]);
        a.started_on = Some("2026-03-02".into());
        a.expires_on = Some("2026-04-01".into());
        let (from, to) = cycle_for(&a, date("2026-03-20"));
        assert_eq!(format_date(from), "2026-03-02");
        assert_eq!(format_date(to), "2026-04-01");
    }

    #[test]
    fn cycle_for_prepaid_without_an_expiry_runs_to_as_of() {
        let mut a = account(BillingMode::Prepaid, &["kn/"]);
        a.started_on = Some("2026-03-02".into());
        let (from, to) = cycle_for(&a, date("2026-03-20"));
        assert_eq!(format_date(from), "2026-03-02");
        assert_eq!(format_date(to), "2026-03-20");
    }

    #[test]
    fn cycle_for_payg_is_the_calendar_month() {
        let a = account(BillingMode::Payg, &["gpt-"]);
        let (from, to) = cycle_for(&a, date("2026-02-17"));
        assert_eq!(format_date(from), "2026-02-01");
        assert_eq!(format_date(to), "2026-02-28");
    }

    #[test]
    fn days_left_counts_down_and_goes_negative_when_expired() {
        let mut sub = account(BillingMode::Subscription, &["claude-"]);
        assert_eq!(days_left(&sub, date("2026-03-10")), Some(4));
        assert_eq!(days_left(&sub, date("2026-03-14")), Some(31));

        sub.mode = BillingMode::Prepaid;
        sub.expires_on = Some("2026-03-12".into());
        assert_eq!(days_left(&sub, date("2026-03-10")), Some(2));
        assert_eq!(days_left(&sub, date("2026-03-18")), Some(-6));

        let payg = account(BillingMode::Payg, &["gpt-"]);
        assert_eq!(days_left(&payg, date("2026-03-10")), None);
    }

    #[test]
    fn split_cost_subscription_has_zero_spend_and_full_notional() {
        let sub = account(BillingMode::Subscription, &["claude-"]);
        let (spend, notional) = split_cost(Some(&sub), 806.42);
        assert_eq!(spend, 0.0, "a subscription's tokens cost nothing extra");
        assert_eq!(notional, 806.42);
    }

    #[test]
    fn split_cost_payg_and_prepaid_spend_the_full_amount() {
        let payg = account(BillingMode::Payg, &["gpt-"]);
        assert_eq!(split_cost(Some(&payg), 12.5), (12.5, 12.5));

        let prepaid = account(BillingMode::Prepaid, &["kn/"]);
        assert_eq!(split_cost(Some(&prepaid), 3.25), (3.25, 3.25));
    }

    #[test]
    fn split_cost_without_an_account_is_payg() {
        assert_eq!(split_cost(None, 7.0), (7.0, 7.0));
    }

    #[test]
    fn renewal_day_is_capped_at_28() {
        let mut a = account(BillingMode::Subscription, &["claude-"]);
        a.renewal_day = Some(31);
        assert_eq!(renewal_day(&a), 28);

        let (from, to) = cycle_for(&a, date("2026-03-30"));
        assert_eq!(format_date(from), "2026-03-28");
        assert_eq!(format_date(to), "2026-04-27");

        // February still has the day, which a 30 or 31 would have skipped.
        let (from, to) = cycle_for(&a, date("2026-02-27"));
        assert_eq!(format_date(from), "2026-01-28");
        assert_eq!(format_date(to), "2026-02-27");
    }
}
