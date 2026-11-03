use crate::model::Config;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SESSION_GAP: i64 = 30 * 60;
pub const QUALIFY_SECONDS: u32 = 5 * 60;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum Mode {
    #[default]
    Sessions,
    Days,
    Either,
    Both,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub started_at: i64,
    pub completed_at: i64,
    pub sessions: u32,
    pub last_active_at: i64,
    pub session_seconds: u32,
    pub session_counted: bool,
    pub day_seconds: BTreeMap<String, u32>,
}
impl Progress {
    fn plausible_day(day: &str, latest: chrono::NaiveDate) -> bool {
        chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").is_ok_and(|parsed| parsed <= latest)
    }

    pub fn active_days(&self) -> u32 {
        // Restored profiles can contain valid future dates from a corrected
        // Windows clock. Those are not completed days today. A one-day window
        // tolerates small time and time-zone corrections without erasing data.
        let today = chrono::Local::now().date_naive();
        let latest = today.succ_opt().unwrap_or(today);
        self.day_seconds
            .iter()
            .filter(|(day, seconds)| {
                **seconds >= QUALIFY_SECONDS && Self::plausible_day(day, latest)
            })
            .count() as u32
    }
    pub fn criteria_met(&self, cfg: &Config) -> bool {
        let sessions = self.sessions >= cfg.learning_sessions;
        let days = self.active_days() >= cfg.learning_days;
        match cfg.learning_mode {
            Mode::Sessions => sessions,
            Mode::Days => days,
            Mode::Either => sessions || days,
            Mode::Both => sessions && days,
        }
    }
    pub fn ready(&self, cfg: &Config) -> bool {
        !cfg.learning_enabled || self.completed_at > 0 || self.criteria_met(cfg)
    }
    pub fn observe(&mut self, time: i64, day: &str, seconds: u32) {
        if self.completed_at > 0 || time <= 0 || seconds == 0 || seconds > 4 {
            return;
        }
        // Replayed samples and backwards clock corrections cannot create usage.
        // The caller supplies monotonic elapsed time; wall time identifies the day.
        if self.last_active_at > 0 && time <= self.last_active_at {
            if time < self.last_active_at {
                // A large backwards wall-clock correction must not glue an
                // unfinished session to a later visit at the corrected time.
                // Keep an already counted session counted, so the correction
                // cannot manufacture a duplicate session either.
                if self.last_active_at.saturating_sub(time) >= SESSION_GAP {
                    self.session_seconds = 0;
                }
                self.last_active_at = time;
            }
            return;
        }
        let seconds = if self.last_active_at > 0 {
            seconds.min(u32::try_from(time.saturating_sub(self.last_active_at)).unwrap_or(u32::MAX))
        } else {
            seconds
        };
        if chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").is_err() {
            return;
        }
        if self.started_at == 0 {
            self.started_at = time;
        }
        if self.last_active_at == 0 || time.saturating_sub(self.last_active_at) >= SESSION_GAP {
            self.session_seconds = 0;
            self.session_counted = false;
        }
        self.last_active_at = time;
        self.session_seconds = self.session_seconds.saturating_add(seconds);
        if self.session_seconds >= QUALIFY_SECONDS && !self.session_counted {
            self.sessions = self.sessions.saturating_add(1);
            self.session_counted = true;
        }
        let active = self.day_seconds.entry(day.into()).or_default();
        *active = active.saturating_add(seconds).min(86400);
        while self.day_seconds.len() > 90 {
            // The learning goal is cumulative. Brief visits must not push an
            // already qualified day out of progress and make a 90-day goal
            // impossible to finish. Keep the same bounded amount of local data.
            // Remove implausible future records before a new, incomplete
            // real day. Otherwise a saved future 90th day can be counted as
            // learning and can prevent a genuine day from accumulating.
            let today = chrono::Local::now().date_naive();
            let latest = today.succ_opt().unwrap_or(today);
            let incomplete = self
                .day_seconds
                .keys()
                .find(|day| !Self::plausible_day(day, latest))
                .cloned()
                .or_else(|| {
                    self.day_seconds
                        .iter()
                        .find(|(_, seconds)| **seconds < QUALIFY_SECONDS)
                        .map(|(day, _)| day.clone())
                });
            if let Some(day) = incomplete {
                self.day_seconds.remove(&day);
            } else {
                self.day_seconds.pop_first();
            }
        }
    }
    pub fn finish_if_ready(&mut self, cfg: &Config, time: i64) {
        if cfg.learning_enabled && self.completed_at == 0 && self.criteria_met(cfg) {
            self.completed_at = time;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn activity(p: &mut Progress, start: i64, day: &str, seconds: u32) {
        for offset in (0..seconds).step_by(2) {
            p.observe(start + i64::from(offset), day, 2);
        }
    }
    #[test]
    fn seven_qualified_sessions_unlock_the_first_placement() {
        let cfg = Config::default();
        let mut p = Progress::default();
        for i in 0..7 {
            activity(&mut p, 10000 + i * 4000, "2026-01-01", 300);
            assert_eq!(p.ready(&cfg), i == 6);
        }
        assert_eq!(p.sessions, 7);
        assert_eq!(p.active_days(), 1);
    }
    #[test]
    fn short_visits_do_not_count() {
        let mut p = Progress::default();
        activity(&mut p, 10000, "2026-01-01", 20);
        activity(&mut p, 20000, "2026-01-02", 20);
        assert_eq!(p.sessions, 0);
        assert_eq!(p.active_days(), 0);
    }
    #[test]
    fn restart_and_short_break_do_not_create_a_session() {
        let mut p = Progress::default();
        activity(&mut p, 10000, "2026-01-01", 300);
        let mut resumed: Progress =
            serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        activity(&mut resumed, 11000, "2026-01-01", 300);
        assert_eq!(resumed.sessions, 1);
        activity(&mut resumed, 15000, "2026-01-01", 300);
        assert_eq!(resumed.sessions, 2);
    }
    #[test]
    fn criteria_support_days_either_and_both() {
        let mut p = Progress {
            sessions: 7,
            ..Default::default()
        };
        let mut cfg = Config {
            learning_mode: Mode::Both,
            ..Default::default()
        };
        assert!(!p.ready(&cfg));
        cfg.learning_mode = Mode::Either;
        assert!(p.ready(&cfg));
        cfg.learning_mode = Mode::Days;
        assert!(!p.ready(&cfg));
        for day in 1..=7 {
            p.day_seconds.insert(format!("2026-01-{day:02}"), 300);
        }
        assert!(p.ready(&cfg));
        cfg.learning_mode = Mode::Both;
        assert!(p.ready(&cfg));
    }
    #[test]
    fn elapsed_calendar_days_and_sleep_are_not_usage() {
        let mut p = Progress::default();
        p.observe(10000, "2026-01-01", 2);
        p.observe(900000, "2026-02-01", 800000);
        assert_eq!(p.active_days(), 0);
        assert_eq!(p.sessions, 0);
    }
    #[test]
    fn completion_is_persistent_and_does_not_reenter_after_setting_changes() {
        let mut p = Progress {
            sessions: 7,
            ..Default::default()
        };
        p.finish_if_ready(&Config::default(), 12345);
        let mut cfg = Config {
            learning_sessions: 50,
            ..Default::default()
        };
        assert!(p.ready(&cfg));
        cfg.learning_enabled = false;
        assert!(Progress::default().ready(&cfg));
    }
    #[test]
    fn repeated_samples_do_not_manufacture_learning_time() {
        let mut progress = Progress::default();
        for _ in 0..1000 {
            progress.observe(10000, "2026-01-01", 2);
        }
        assert_eq!(progress.session_seconds, 2);
        assert_eq!(progress.sessions, 0);
        assert_eq!(progress.day_seconds["2026-01-01"], 2);
    }
    #[test]
    fn clock_correction_does_not_create_an_extra_computer_session() {
        let mut progress = Progress::default();
        activity(&mut progress, 10000, "2026-01-01", 300);
        progress.observe(9000, "2026-01-01", 2);
        activity(&mut progress, 9002, "2026-01-01", 300);
        assert_eq!(progress.sessions, 1);
    }
    #[test]
    fn invalid_days_are_not_accepted_as_learning_evidence() {
        let mut progress = Progress::default();
        progress.observe(10000, "invalid", 2);
        progress.observe(10002, "2026-02-30", 2);
        assert_eq!(progress.started_at, 0);
        assert!(progress.day_seconds.is_empty());
    }
    #[test]
    fn invalid_saved_calendar_keys_never_unlock_learning_or_displace_real_days() {
        let config = Config {
            learning_mode: Mode::Days,
            learning_days: 90,
            ..Default::default()
        };
        let mut progress = Progress::default();
        let first_day = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        for index in 0..89 {
            progress
                .day_seconds
                .insert((first_day + chrono::Duration::days(index)).to_string(), 300);
        }
        // Simulate a legacy or edited persisted profile, not an observation:
        // observe() already rejects malformed dates in fresh input.
        progress.day_seconds.insert("9999-invalid".into(), 300);
        let saved = serde_json::to_string(&progress).unwrap();
        let mut restored: Progress = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.day_seconds.len(), 90);
        assert_eq!(restored.active_days(), 89);
        assert!(!restored.ready(&config));

        // On the very next real day, the bounded map must discard bad
        // evidence rather than pruning each new sample forever.
        activity(&mut restored, 10000, "2026-04-01", 300);
        assert_eq!(restored.day_seconds.len(), 90);
        assert!(!restored.day_seconds.contains_key("9999-invalid"));
        assert_eq!(restored.active_days(), 90);
        assert!(restored.ready(&config));
        restored.finish_if_ready(&config, 20000);
        assert_eq!(restored.completed_at, 20000);
    }

    #[test]
    fn future_saved_day_does_not_unlock_learning_or_block_the_ninetieth_real_day() {
        let config = Config {
            learning_mode: Mode::Days,
            learning_days: 90,
            ..Default::default()
        };
        let today = chrono::Local::now().date_naive();
        let future = (today + chrono::Duration::days(400)).to_string();
        let mut progress = Progress::default();
        for index in 0..89 {
            let day = today - chrono::Duration::days(100 - index);
            progress.day_seconds.insert(day.to_string(), 300);
        }
        progress.day_seconds.insert(future.clone(), 300);
        let home = std::env::temp_dir().join(format!(
            "desktop-pilot-future-learning-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = crate::store::Store::open(&home).unwrap();
        store.set("learning", &progress).unwrap();
        let mut restored: Progress = store.get("learning").unwrap();
        assert_eq!(restored.day_seconds.len(), 90);
        assert_eq!(restored.active_days(), 89);
        assert!(!restored.ready(&config));
        // A new real day must accumulate samples, not be repeatedly pruned.
        activity(&mut restored, 10000, &today.to_string(), 300);
        assert_eq!(restored.day_seconds.len(), 90);
        assert!(!restored.day_seconds.contains_key(&future));
        assert_eq!(restored.active_days(), 90);
        assert!(restored.ready(&config));
        restored.finish_if_ready(&config, 20000);
        store.set("learning", &restored).unwrap();
        let persisted: Progress = store.get("learning").unwrap();
        assert_eq!(persisted.completed_at, 20000);
        assert_eq!(persisted.active_days(), 90);
        drop(store);
        // Brief sharing violations during SQLite shutdown are not a
        // functional failure of the learning gate.
        for attempt in 0..5 {
            match std::fs::remove_dir_all(&home) {
                Ok(()) => break,
                Err(_) if attempt < 4 => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(error) => panic!("could not remove test profile: {error}"),
            }
        }
    }

    #[test]
    fn future_saved_days_do_not_reset_previously_completed_learning() {
        let mut progress = Progress {
            completed_at: 12345,
            ..Default::default()
        };
        let future = (chrono::Local::now().date_naive() + chrono::Duration::days(400)).to_string();
        progress.day_seconds.insert(future, 300);
        let config = Config {
            learning_mode: Mode::Days,
            learning_days: 1,
            ..Default::default()
        };
        assert_eq!(progress.active_days(), 0);
        assert!(progress.ready(&config));
        assert_eq!(progress.completed_at, 12345);
    }

    #[test]
    fn invalid_saved_days_are_ignored_without_erasing_completed_learning() {
        let config = Config {
            learning_mode: Mode::Days,
            learning_days: 2,
            ..Default::default()
        };
        let mut progress = Progress::default();
        for day in ["2026-02-28", "2026-02-30", "2026-02-28-extra", "junk"] {
            progress.day_seconds.insert(day.into(), 300);
        }
        assert_eq!(progress.active_days(), 1);
        assert!(!progress.ready(&config));
        // A historical completion remains intact on read; do not reset
        // explicitly completed learning just because of malformed keys.
        progress.completed_at = 12345;
        assert!(progress.ready(&config));
        assert_eq!(progress.day_seconds.len(), 4);
    }

    #[test]
    fn short_days_cannot_erase_qualified_progress_towards_ninety_days() {
        let config = Config {
            learning_mode: Mode::Days,
            learning_days: 90,
            ..Default::default()
        };
        let mut progress = Progress::default();
        let first_day = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        for index in 0..180 {
            let day = first_day + chrono::Duration::days(index);
            let time = 10000 + index * 86400;
            activity(
                &mut progress,
                time,
                &day.to_string(),
                if index % 2 == 0 { 2 } else { 300 },
            );
            assert!(progress.day_seconds.len() <= 90);
        }
        assert_eq!(progress.active_days(), 90);
        assert!(progress.ready(&config));
        progress.finish_if_ready(&config, 20000000);
        assert_eq!(progress.completed_at, 20000000);
    }
    #[test]
    fn large_clock_rewind_does_not_complete_a_stale_partial_learning_session() {
        let home = std::env::temp_dir().join(format!(
            "desktop-pilot-learning-rewind-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = crate::store::Store::open(&home).unwrap();
        let config = Config {
            learning_sessions: 1,
            learning_mode: Mode::Sessions,
            ..Config::default()
        };
        let day = "2026-01-01";
        let mut progress = Progress::default();
        activity(&mut progress, 10_000, day, 240);
        assert_eq!(progress.session_seconds, 240);
        assert!(!progress.ready(&config));
        store.set("learning", &progress).unwrap();

        // Simulate a saved profile restored after Windows time moved back by
        // more than the session gap. The first corrected tick cannot earn time
        // or keep four minutes of an uncompleted session alive.
        let mut restored: Progress = store.get("learning").unwrap();
        restored.observe(6_000, day, 2);
        activity(&mut restored, 6_002, day, 60);
        assert_eq!(restored.session_seconds, 60);
        assert_eq!(restored.sessions, 0);
        assert!(!restored.ready(&config));
        store.set("learning", &restored).unwrap();
        assert_eq!(store.get::<Progress>("learning").unwrap().sessions, 0);

        // Genuine subsequent activity can still finish learning after an
        // ordinary interval, using the same production observation method.
        activity(&mut restored, 20_000, day, 300);
        assert_eq!(restored.sessions, 1);
        assert!(restored.ready(&config));
        restored.finish_if_ready(&config, 20_400);
        store.set("learning", &restored).unwrap();
        assert_eq!(
            store.get::<Progress>("learning").unwrap().completed_at,
            20_400
        );
        drop(store);
        for attempt in 0..5 {
            match std::fs::remove_dir_all(&home) {
                Ok(()) => break,
                Err(_) if attempt < 4 => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(error) => panic!("could not remove test profile: {error}"),
            }
        }
    }

    #[test]
    fn short_clock_rewind_preserves_partial_progress_without_extra_sessions() {
        let day = "2026-01-01";
        let mut partial = Progress::default();
        activity(&mut partial, 10_000, day, 240);
        partial.observe(9_700, day, 2);
        activity(&mut partial, 9_702, day, 60);
        assert_eq!(partial.sessions, 1);
        assert_eq!(partial.session_seconds, 300);

        // A completed session remains counted even when a much larger
        // correction happens during ongoing observation.
        let mut counted = Progress::default();
        activity(&mut counted, 10_000, day, 300);
        counted.observe(6_000, day, 2);
        activity(&mut counted, 6_002, day, 300);
        assert_eq!(counted.sessions, 1);
    }
}
