//! What the hub does every night at 03:00 UTC: forget what is past keeping. Backups are the
//! database instance's.

use std::time::Duration;

use crate::app::AppState;

const DAY: i64 = 86_400_000;
const AT: i64 = 3 * 3_600_000;

/// When the job next runs after `now`, in Unix milliseconds.
#[must_use]
pub fn next_run(now: i64) -> i64 {
    let today = now - now.rem_euclid(DAY) + AT;
    if today > now { today } else { today + DAY }
}

/// Runs the job every night, for as long as the hub runs.
pub async fn every_night(state: AppState) {
    loop {
        let now = state.clock.now();
        let wait = u64::try_from(next_run(now) - now).unwrap_or_default();
        tokio::time::sleep(Duration::from_millis(wait)).await;
        if let Err(error) = run(&state).await {
            eprintln!("error: last night's clean-up failed: {error:#}");
        }
    }
}

/// Deletes events older than the retention period.
pub async fn run(state: &AppState) -> anyhow::Result<()> {
    let now = state.clock.now();
    let days = i64::from(state.config.retention_days);
    state
        .db
        .lock()
        .await
        .execute("DELETE FROM events WHERE ts < $1", &[&(now - days * DAY)])
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::next_run;

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
}
