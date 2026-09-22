use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// Aggregate of rtk's own history. `total_tokens` is what the compressed output
/// would have cost uncompressed, so `saved / total` is the saving rate.
#[derive(Debug, Clone, PartialEq)]
pub struct RtkStats {
    pub saved_tokens: i64,
    pub total_tokens: i64,
    pub entries: i64,
    pub daily: Vec<(String, i64)>,
    pub top_commands: Vec<(String, i64, f64, i64)>,
}

pub fn read_rtk(db: &Path, since_ms: i64) -> Result<RtkStats> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open rtk history at {}", db.display()))?;

    // `timestamp` is an RFC3339 UTC text column, so the cutoff is expressed the
    // same way and compared lexicographically (both sides share the +00:00 offset).
    let since = rfc3339_from_ms(since_ms);
    let mut stmt = conn.prepare(
        "SELECT original_cmd, saved_tokens, input_tokens, timestamp FROM commands \
         WHERE timestamp >= ?1",
    )?;
    let rows = stmt.query_map([since.as_str()], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;

    let mut saved_tokens = 0i64;
    let mut total_tokens = 0i64;
    let mut entries = 0i64;
    let mut by_command: std::collections::HashMap<String, (i64, i64, i64)> = Default::default();
    let mut by_day: std::collections::BTreeMap<String, i64> = Default::default();

    for row in rows {
        let (raw, saved, input, ts) = row?;
        // The raw command line is normalised here and dropped in this same scope.
        let command = normalise_command(&raw);
        saved_tokens += saved;
        total_tokens += input;
        entries += 1;
        let e = by_command.entry(command).or_insert((0, 0, 0));
        e.0 += saved;
        e.1 += input;
        e.2 += 1;
        *by_day.entry(local_date_from_rfc3339(&ts)).or_default() += saved;
    }

    let mut top_commands: Vec<(String, i64, f64, i64)> = by_command
        .into_iter()
        .map(|(command, (saved, input, runs))| {
            let pct = if input == 0 {
                0.0
            } else {
                saved as f64 / input as f64 * 100.0
            };
            (command, saved, pct, runs)
        })
        .collect();
    top_commands.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    Ok(RtkStats {
        saved_tokens,
        total_tokens,
        entries,
        daily: by_day.into_iter().collect(),
        top_commands,
    })
}

/// `0` means "all time"; anything earlier than the epoch also means all time,
/// because no rtk row can predate it.
fn rfc3339_from_ms(ms: i64) -> String {
    let secs = ms / 1000;
    let (days, rem_secs) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    let (h, mi, s) = (rem_secs / 3600, rem_secs % 3600 / 60, rem_secs % 60);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}.000000+00:00")
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Shifts the UTC instant to local time using the offset SQLite reports for it.
fn local_date_from_rfc3339(ts: &str) -> String {
    let secs: i64 = Connection::open_in_memory()
        .and_then(|c| c.query_row("SELECT strftime('%s', ?1)", [ts], |r| r.get::<_, String>(0)))
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let date = Connection::open_in_memory()
        .and_then(|c| {
            c.query_row(
                "SELECT strftime('%Y-%m-%d', ?1, 'unixepoch', 'localtime')",
                [secs],
                |r| r.get::<_, String>(0),
            )
        })
        .unwrap_or_default();
    date
}

const KNOWN_TOOLS: [&str; 8] = [
    "git", "cargo", "bun", "npm", "go", "docker", "kubectl", "pnpm",
];

/// The single place allowed to read rtk command text. Everything downstream sees
/// only the first token, or the first two for tools where the subcommand is what
/// the reader cares about.
pub fn normalise_command(raw: &str) -> String {
    let trimmed = raw.trim();
    let trimmed = trimmed.strip_prefix("rtk ").map(str::trim_start).unwrap_or(trimmed);
    let mut tokens = trimmed.split_whitespace();
    let Some(first) = tokens.next() else {
        return "(lainnya)".to_string();
    };
    let command = match tokens.next() {
        Some(second) if KNOWN_TOOLS.contains(&first.to_lowercase().as_str()) => {
            if is_bare_word(second) {
                format!("{first} {second}")
            } else {
                first.to_string()
            }
        }
        _ => first.to_string(),
    };
    command.to_lowercase()
}

fn is_bare_word(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    const RTK_SCHEMA: &str = "
CREATE TABLE commands (
  id INTEGER PRIMARY KEY,
  timestamp TEXT NOT NULL,
  original_cmd TEXT NOT NULL,
  rtk_cmd TEXT NOT NULL,
  input_tokens INTEGER NOT NULL,
  output_tokens INTEGER NOT NULL,
  saved_tokens INTEGER NOT NULL,
  savings_pct REAL NOT NULL,
  exec_time_ms INTEGER DEFAULT 0,
  project_path TEXT DEFAULT ''
);
";

    fn rtk_db(rows: &[(&str, &str, i64, i64, f64)]) -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("history.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(RTK_SCHEMA).unwrap();
        for (ts, cmd, input, saved, pct) in rows {
            conn.execute(
                "INSERT INTO commands (timestamp, original_cmd, rtk_cmd, input_tokens, output_tokens, saved_tokens, savings_pct) \
                 VALUES (?1, ?2, ?2, ?3, ?4, ?4, ?5)",
                rusqlite::params![ts, cmd, input, saved, pct],
            )
            .unwrap();
        }
        (tmp, path)
    }

    #[test]
    fn normalise_command_keeps_subcommand_for_known_tools() {
        assert_eq!(normalise_command("git status --short"), "git status");
        assert_eq!(normalise_command("cargo test -p x"), "cargo test");
    }

    #[test]
    fn normalise_command_takes_first_token_otherwise() {
        assert_eq!(normalise_command("ls -la /tmp"), "ls");
        assert_eq!(normalise_command("  "), "(lainnya)");
    }

    #[test]
    fn normalise_command_strips_rtk_prefix() {
        assert_eq!(normalise_command("rtk git diff"), "git diff");
    }

    #[test]
    fn normalise_command_ignores_flag_as_subcommand() {
        assert_eq!(normalise_command("git --version"), "git");
    }

    #[test]
    fn read_rtk_aggregates_by_normalised_command() {
        let (_tmp, path) = rtk_db(&[
            ("2026-03-10T10:00:00.000000+00:00", "git status --short", 100, 80, 80.0),
            ("2026-03-10T11:00:00.000000+00:00", "git status -sb", 50, 40, 80.0),
            ("2026-03-10T12:00:00.000000+00:00", "ls -la", 10, 5, 50.0),
        ]);
        let s = read_rtk(&path, 0).unwrap();
        assert_eq!(s.entries, 3);
        assert_eq!(s.saved_tokens, 125);
        assert_eq!(s.total_tokens, 160);

        let git = s.top_commands.iter().find(|c| c.0 == "git status").unwrap();
        assert_eq!(git.1, 120);
        assert_eq!(git.3, 2);
        assert!((git.2 - 80.0).abs() < 1e-9);
        assert_eq!(s.top_commands[0].0, "git status");
    }

    #[test]
    fn read_rtk_respects_since_ms() {
        let (_tmp, path) = rtk_db(&[
            ("2020-01-01T00:00:00.000000+00:00", "git status", 100, 80, 80.0),
            ("2026-03-10T10:00:00.000000+00:00", "git status", 10, 8, 80.0),
        ]);
        let cutoff = 1_700_000_000_000i64;
        let s = read_rtk(&path, cutoff).unwrap();
        assert_eq!(s.entries, 1);
        assert_eq!(s.saved_tokens, 8);
    }

    #[test]
    fn read_rtk_daily_groups_by_local_date() {
        let (_tmp, path) = rtk_db(&[
            ("2026-03-10T10:00:00.000000+00:00", "git status", 100, 80, 80.0),
            ("2026-03-10T11:00:00.000000+00:00", "ls", 50, 40, 80.0),
            ("2026-03-11T10:00:00.000000+00:00", "git status", 10, 5, 50.0),
        ]);
        let s = read_rtk(&path, 0).unwrap();
        assert_eq!(s.daily.len(), 2);
        assert_eq!(s.daily[0].1, 120);
        assert_eq!(s.daily[1].1, 5);
        let dates: Vec<&str> = s.daily.iter().map(|d| d.0.as_str()).collect();
        let mut sorted = dates.clone();
        sorted.sort_unstable();
        assert_eq!(dates, sorted);
    }

    #[test]
    fn read_rtk_missing_db_is_an_error_not_a_panic() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_rtk(&tmp.path().join("nope.db"), 0).is_err());
    }
}
