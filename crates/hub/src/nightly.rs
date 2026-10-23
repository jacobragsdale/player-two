//! What the hub does every night at 03:00 UTC: forget what is past keeping, then copy the
//! database to the backups folder, keeping the newest seven copies.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;

use crate::app::AppState;

const DAY: i64 = 86_400_000;
const AT: i64 = 3 * 3_600_000;
const COPIES_KEPT: usize = 7;

/// When the job next runs after `now`, in Unix milliseconds.
#[must_use]
pub fn next_run(now: i64) -> i64 {
    let today = now - now.rem_euclid(DAY) + AT;
    if today > now { today } else { today + DAY }
}

/// The date of a Unix millisecond, as `YYYY-MM-DD` in UTC.
#[must_use]
pub fn date(ms: i64) -> String {
    // Howard Hinnant's days-to-civil, for the proleptic Gregorian calendar.
    let days = ms.div_euclid(DAY) + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * of_year + 2) / 153;
    let day = of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Runs the job every night, for as long as the hub runs.
pub async fn every_night(state: AppState) {
    loop {
        let now = state.clock.now();
        let wait = u64::try_from(next_run(now) - now).unwrap_or_default();
        tokio::time::sleep(Duration::from_millis(wait)).await;
        if let Err(error) = run(&state).await {
            eprintln!("error: last night's clean-up or backup failed: {error:#}");
        }
    }
}

/// Deletes events older than the retention period, then writes tonight's copy and drops the
/// oldest copies beyond seven.
pub async fn run(state: &AppState) -> anyhow::Result<()> {
    let now = state.clock.now();
    let days = i64::from(state.config.retention_days);
    let backups = state.config.backups.clone();
    let copy = backups.join(format!("player-two-{}.db", date(now)));
    state
        .db
        .call(move |connection| {
            connection.execute("DELETE FROM events WHERE ts < ?1", [now - days * DAY])?;
            fs::create_dir_all(&backups)
                .with_context(|| format!("cannot make the backups folder {}", backups.display()))?;
            // A copy already made today, by a hub that restarted since, is replaced.
            if copy.exists() {
                fs::remove_file(&copy)?;
            }
            connection.execute("VACUUM INTO ?1", [copy.to_string_lossy()])?;
            prune(&backups)
        })
        .await
}

/// Removes all but the newest copies. Their names hold their dates, so they sort by age.
fn prune(backups: &Path) -> anyhow::Result<()> {
    let mut copies: Vec<PathBuf> = fs::read_dir(backups)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("player-two-") && name.ends_with(".db"))
        })
        .collect();
    copies.sort();
    let extra = copies.len().saturating_sub(COPIES_KEPT);
    for old in copies.into_iter().take(extra) {
        fs::remove_file(&old)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{date, next_run};

    #[test]
    fn it_runs_at_three_in_the_morning_utc() {
        let midnight = 1_700_006_400_000; // 2023-11-15T00:00:00Z
        assert_eq!(next_run(midnight), midnight + 3 * 3_600_000);
        assert_eq!(
            next_run(midnight + 3 * 3_600_000),
            midnight + 27 * 3_600_000
        );
        assert_eq!(
            next_run(midnight + 5 * 3_600_000),
            midnight + 27 * 3_600_000
        );
    }

    #[test]
    fn dates_are_utc_calendar_days() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_700_006_400_000), "2023-11-15");
        assert_eq!(date(951_782_400_000), "2000-02-29");
        assert_eq!(date(4_107_542_399_999), "2100-02-28");
    }
}
