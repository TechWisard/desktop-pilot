use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
pub fn norm(s: &str) -> String {
    expand_env(s)
        .trim_start_matches(r"\\?\")
        .replace('/', "\\")
        .to_lowercase()
}
pub fn expand_env(s: &str) -> String {
    let mut output = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        output.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        if let Some(end) = tail.find('%') {
            let key = &tail[..end];
            output.push_str(&std::env::var(key).unwrap_or_else(|_| format!("%{key}%")));
            rest = &tail[end + 1..];
        } else {
            output.push_str(&rest[start..]);
            return output;
        }
    }
    output.push_str(rest);
    output
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum Policy {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub language: String,
    pub respect_taskbar_pins: bool,
    pub learning_enabled: bool,
    pub learning_mode: crate::learning::Mode,
    pub learning_sessions: u32,
    pub learning_days: u32,
    pub automatic: bool,
    pub collect: bool,
    pub autostart: bool,
    pub slots: usize,
    pub interval_days: i64,
    pub half_life_days: f64,
    pub min_days: usize,
    pub advantage: f64,
    pub max_changes: usize,
    pub idle_seconds: u32,
    pub zone_col: i32,
    pub zone_row: i32,
    pub zone_columns: i32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            language: "en".into(),
            respect_taskbar_pins: true,
            learning_enabled: true,
            learning_mode: crate::learning::Mode::Sessions,
            learning_sessions: 7,
            learning_days: 7,
            automatic: false,
            collect: false,
            autostart: false,
            slots: 4,
            interval_days: 7,
            half_life_days: 14.0,
            min_days: 1,
            advantage: 1.5,
            max_changes: 1,
            idle_seconds: 120,
            zone_col: 4,
            zone_row: 0,
            zone_columns: 2,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=100).contains(&self.learning_sessions),
            "Learning sessions must be between 1 and 100"
        );
        ensure!(
            (1..=90).contains(&self.learning_days),
            "Learning days must be between 1 and 90"
        );
        ensure!(
            ["en", "ru"].contains(&self.language.as_str()),
            "Language must be en or ru"
        );
        ensure!((1..=8).contains(&self.slots), "Число мест: от 1 до 8");
        ensure!(
            (1..=90).contains(&self.interval_days),
            "Период: от 1 до 90 дней"
        );
        ensure!(
            self.half_life_days.is_finite() && (3.0..=90.0).contains(&self.half_life_days),
            "Память: от 3 до 90 дней"
        );
        ensure!(
            (1..=14).contains(&self.min_days),
            "Нужно от 1 до 14 дней наблюдений"
        );
        ensure!(
            self.advantage.is_finite() && (1.1..=3.0).contains(&self.advantage),
            "Порог преимущества: 1,1–3"
        );
        ensure!(
            (1..=self.slots).contains(&self.max_changes),
            "Число замен превышает число мест"
        );
        ensure!(
            (30..=900).contains(&self.idle_seconds),
            "Порог бездействия: 30–900 секунд"
        );
        ensure!(
            (0..=40).contains(&self.zone_col)
                && (0..=40).contains(&self.zone_row)
                && (1..=8).contains(&self.zone_columns),
            "Недопустимая область размещения"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct App {
    pub id: String,
    pub name: String,
    pub category: String,
    pub source: String,
    pub target: String,
    pub arguments: String,
    pub cwd: String,
    pub icon: String,
    pub icon_index: i32,
    #[serde(default)]
    pub policy: Policy,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub seed: f64,
    #[serde(default)]
    pub seed_at: i64,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub valid: bool,
    #[serde(default)]
    pub observed_roots: Vec<String>,
}
impl App {
    pub fn identity(&self) -> String {
        format!(
            "{}\n{}\n{}",
            norm(&self.target),
            self.arguments.trim(),
            norm(&self.cwd)
        )
    }
    pub fn assign_id(&mut self) {
        self.id = hash(self.identity().as_bytes())[..20].to_string();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Day {
    pub app: String,
    pub date: i64,
    pub sessions: u32,
    pub launches: u32,
    pub seconds: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Owned {
    pub app: String,
    pub path: String,
    pub sha: String,
    pub position: Point,
    pub manual: bool,
    pub since: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct State {
    pub owned: Vec<Owned>,
    pub last_review: i64,
    pub last_change: i64,
    pub topology: String,
    pub initialized: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ranked {
    pub id: String,
    pub name: String,
    pub score: f64,
    pub days: usize,
    pub launches: u32,
    pub seconds: u32,
    pub eligible: bool,
    pub reason: String,
}

pub fn rank(apps: &[App], days: &[Day], cfg: &Config, time: i64) -> Vec<Ranked> {
    let mut output = Vec::new();
    for app in apps {
        let mut score = if app.seed_at > 0 {
            app.seed
                * 2f64.powf(-((time - app.seed_at).max(0) as f64) / (86400.0 * cfg.half_life_days))
        } else {
            0.0
        };
        let mut active_days = HashSet::new();
        let (mut launches, mut seconds) = (0, 0);
        for d in days
            .iter()
            .filter(|d| d.app == app.id && time - d.date < 90 * 86400 && d.date <= time)
        {
            let age = (time - d.date).max(0) as f64 / 86400.0;
            let value = 2.0 * f64::from(d.sessions.min(12))
                + (f64::from(d.seconds.min(3600)) / 60.0).ln_1p() * 0.4;
            score += value * 2f64.powf(-age / cfg.half_life_days);
            if d.sessions > 0 || d.seconds >= 10 {
                active_days.insert(d.date);
            }
            launches += d.launches;
            seconds += d.seconds;
        }
        let enough = active_days.len() >= cfg.min_days
            || (app.seed > 0.0 && time - app.seed_at <= 30 * 86400);
        let (eligible, reason) = if !app.valid {
            (false, "Исходный ярлык или цель недоступны")
        } else if app.policy == Policy::Never {
            (false, "Исключено вручную")
        } else if app.policy == Policy::Always {
            (true, "Постоянно: ручной выбор")
        } else if app.pinned {
            (false, "Уже закреплено на панели задач")
        } else if !enough {
            (false, "Пока мало независимых дней наблюдений")
        } else {
            (true, "Повторяемое использование; есть польза прямого входа")
        };
        output.push(Ranked {
            id: app.id.clone(),
            name: app.name.clone(),
            score,
            days: active_days.len(),
            launches,
            seconds,
            eligible,
            reason: reason.into(),
        });
    }
    output.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.name.cmp(&b.name)));
    output
}

pub fn select(
    apps: &[App],
    ranking: &[Ranked],
    state: &State,
    cfg: &Config,
    initial: bool,
) -> Vec<String> {
    let by_id: HashMap<_, _> = apps.iter().map(|a| (a.id.as_str(), a)).collect();
    let scores: HashMap<_, _> = ranking.iter().map(|r| (r.id.as_str(), r)).collect();
    let fixed = |id: &str| {
        by_id.get(id).is_some_and(|a| a.policy == Policy::Always)
            || state.owned.iter().any(|o| o.app == id && o.manual)
    };
    let allowed = |id: &str| {
        scores.get(id).is_some_and(|r| r.eligible)
            || (fixed(id)
                && by_id
                    .get(id)
                    .is_some_and(|a| a.valid && a.policy != Policy::Never))
    };
    let mut result: Vec<String> = state
        .owned
        .iter()
        .filter(|o| {
            allowed(&o.app)
                || by_id
                    .get(o.app.as_str())
                    .is_some_and(|a| a.valid && a.policy == Policy::Auto && !a.pinned)
        })
        .map(|o| o.app.clone())
        .collect();
    for a in apps
        .iter()
        .filter(|a| a.policy == Policy::Always && a.valid)
    {
        if !result.contains(&a.id) {
            result.push(a.id.clone());
        }
    }
    while result.len() > cfg.slots {
        let worst = result
            .iter()
            .enumerate()
            .filter(|(_, id)| !fixed(id))
            .min_by(|(_, a), (_, b)| {
                scores[a.as_str()]
                    .score
                    .total_cmp(&scores[b.as_str()].score)
            })
            .map(|(i, _)| i);
        if let Some(i) = worst {
            result.remove(i);
        } else {
            break;
        }
    }
    let mut changes = 0;
    for candidate in ranking.iter().filter(|r| r.eligible) {
        if result.contains(&candidate.id) {
            continue;
        }
        if !initial && changes >= cfg.max_changes {
            break;
        }
        if result.len() < cfg.slots {
            result.push(candidate.id.clone());
            changes += 1;
            continue;
        }
        let worst = result
            .iter()
            .enumerate()
            .filter(|(_, id)| !fixed(id))
            .min_by(|(_, a), (_, b)| {
                scores[a.as_str()]
                    .score
                    .total_cmp(&scores[b.as_str()].score)
            })
            .map(|(i, id)| (i, scores[id.as_str()].score));
        if let Some((i, value)) = worst
            && candidate.score >= value * cfg.advantage
            && candidate.score - value >= 3.0
        {
            result[i] = candidate.id.clone();
            changes += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app(id: &str, seed: f64) -> App {
        App {
            id: id.into(),
            name: id.into(),
            seed,
            seed_at: now(),
            valid: true,
            ..Default::default()
        }
    }
    fn owned(id: &str, manual: bool) -> Owned {
        Owned {
            app: id.into(),
            path: String::new(),
            sha: String::new(),
            position: Point::default(),
            manual,
            since: 0,
        }
    }
    #[test]
    fn pin_and_context_menu_are_excluded() {
        let mut a = app("7zip", 99.0);
        a.policy = Policy::Never;
        let mut b = app("chat", 90.0);
        b.pinned = true;
        assert!(
            rank(&[a, b], &[], &Config::default(), now())
                .iter()
                .all(|r| !r.eligible)
        );
    }
    #[test]
    fn manual_choice_beats_frequency_and_pin() {
        let mut a = app("a", 1.0);
        a.policy = Policy::Always;
        a.pinned = true;
        let apps = vec![a, app("b", 100.0)];
        let cfg = Config {
            slots: 1,
            ..Default::default()
        };
        assert_eq!(
            select(
                &apps,
                &rank(&apps, &[], &cfg, now()),
                &State::default(),
                &cfg,
                true
            ),
            ["a"]
        );
    }
    #[test]
    fn no_churn_below_threshold() {
        let apps = vec![app("a", 10.0), app("b", 14.0)];
        let cfg = Config {
            slots: 1,
            ..Default::default()
        };
        let s = State {
            owned: vec![owned("a", false)],
            ..Default::default()
        };
        assert_eq!(
            select(&apps, &rank(&apps, &[], &cfg, now()), &s, &cfg, false),
            ["a"]
        );
    }
    #[test]
    fn at_most_one_replacement() {
        let apps = vec![app("a", 2.0), app("b", 3.0), app("c", 40.0), app("d", 50.0)];
        let cfg = Config {
            slots: 2,
            ..Default::default()
        };
        let s = State {
            owned: vec![owned("a", false), owned("b", false)],
            ..Default::default()
        };
        let r = select(&apps, &rank(&apps, &[], &cfg, now()), &s, &cfg, false);
        assert_eq!(r.iter().filter(|id| **id == "c" || **id == "d").count(), 1);
    }
    #[test]
    fn manual_position_is_retained() {
        let apps = vec![app("a", 2.0), app("b", 90.0)];
        let cfg = Config {
            slots: 1,
            ..Default::default()
        };
        let s = State {
            owned: vec![owned("a", true)],
            ..Default::default()
        };
        assert_eq!(
            select(&apps, &rank(&apps, &[], &cfg, now()), &s, &cfg, false),
            ["a"]
        );
    }
    #[test]
    fn independent_days_required() {
        let mut a = app("a", 0.0);
        a.seed_at = 0;
        let ds = vec![Day {
            app: "a".into(),
            date: now() / 86400 * 86400,
            sessions: 500,
            launches: 500,
            seconds: 80000,
        }];
        assert!(
            !rank(
                &[a],
                &ds,
                &Config {
                    min_days: 3,
                    ..Default::default()
                },
                now()
            )[0]
            .eligible
        );
    }
    #[test]
    fn launch_requests_alone_never_create_demand() {
        let mut app = app("system-started", 0.0);
        app.seed_at = 0;
        let days = vec![Day {
            app: app.id.clone(),
            date: now() / 86400 * 86400,
            launches: 999,
            ..Default::default()
        }];
        let result = rank(&[app], &days, &Config::default(), now());
        assert!(!result[0].eligible);
        assert_eq!(result[0].score, 0.0);
    }
    #[test]
    fn invalid_settings_rejected() {
        let c = Config {
            slots: 0,
            ..Default::default()
        };
        assert!(c.validate().is_err());
        let c = Config {
            advantage: f64::NAN,
            ..Default::default()
        };
        assert!(c.validate().is_err());
    }
    #[test]
    fn clearing_statistics_does_not_remove_existing_shortcuts() {
        let apps = vec![app("a", 0.0)];
        let cfg = Config::default();
        let state = State {
            owned: vec![owned("a", false)],
            ..Default::default()
        };
        assert_eq!(
            select(&apps, &rank(&apps, &[], &cfg, now()), &state, &cfg, false),
            ["a"]
        );
    }
    #[test]
    fn fresh_profile_requires_opt_in() {
        let cfg = Config::default();
        assert!(!cfg.automatic && !cfg.collect && !cfg.autostart);
    }
    #[test]
    fn old_configuration_migrates_without_resetting_choices() {
        let cfg: Config = serde_json::from_str(
            r#"{"automatic":true,"collect":true,"autostart":true,"slots":6,"zone_col":7}"#,
        )
        .unwrap();
        assert!(cfg.automatic && cfg.collect && cfg.autostart);
        assert_eq!(cfg.slots, 6);
        assert_eq!(cfg.zone_col, 7);
        assert_eq!(cfg.language, "en");
        assert!(cfg.respect_taskbar_pins);
    }
}
