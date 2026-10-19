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
    pub fn active_days(&self) -> u32 {
        self.day_seconds
            .values()
            .filter(|seconds| **seconds >= QUALIFY_SECONDS)
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
            self.day_seconds.pop_first();
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
}
