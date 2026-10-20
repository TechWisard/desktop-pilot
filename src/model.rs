use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
pub fn norm(s: &str) -> String {
    let expanded = expand_env(s).replace('/', "\\");
    let ordinary = if let Some(unc) = expanded.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        expanded.trim_start_matches(r"\\?\").to_owned()
    };
    ordinary.to_lowercase()
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
/// URI payloads are not Windows paths: preserve their case, separators and
/// percent encoding. Only HTTP(S) hostnames have an additional known case rule.
pub fn target_identity(target: &str) -> String {
    if let Some((scheme, payload)) = target.split_once(':')
        && scheme.len() > 1
        && scheme.as_bytes()[0].is_ascii_alphabetic()
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
    {
        // A single ASCII letter followed by ':' is a Windows drive prefix.
        // Other valid schemes may have opaque payloads without '//', e.g. mailto.
        let Some(rest) = payload.strip_prefix("//").filter(|_| {
            scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")
        }) else {
            return format!("{}:{payload}", scheme.to_ascii_lowercase());
        };
        let boundary = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, suffix) = rest.split_at(boundary);
        let authority = if let Some((credentials, host)) = authority.rsplit_once('@') {
            format!("{credentials}@{}", host.to_lowercase())
        } else {
            authority.to_lowercase()
        };
        return format!("{}://{authority}{suffix}", scheme.to_ascii_lowercase());
    }
    norm(target)
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
            target_identity(&self.target),
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
    /// Stable filesystem identity; absent only for profiles created by older versions.
    #[serde(default)]
    pub file_id: Option<String>,
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
    // Group retained rows once. Scanning every day's history for every shortcut
    // made a large catalog quadratic and blocked regular Settings refreshes.
    let mut history: HashMap<&str, Vec<&Day>> = HashMap::new();
    for day in days
        .iter()
        .filter(|day| day.date <= time && time.saturating_sub(day.date) < 90 * 86400)
    {
        history.entry(day.app.as_str()).or_default().push(day);
    }
    let mut output = Vec::new();
    for app in apps {
        let seed_is_valid =
            app.seed.is_finite() && app.seed > 0.0 && app.seed_at > 0 && app.seed_at <= time;
        let mut score = if seed_is_valid {
            app.seed
                * 2f64.powf(
                    -(time.saturating_sub(app.seed_at) as f64) / (86400.0 * cfg.half_life_days),
                )
        } else {
            0.0
        };
        let mut active_days = HashSet::new();
        let (mut launches, mut seconds) = (0u32, 0u32);
        for d in history.get(app.id.as_str()).into_iter().flatten() {
            let age = time.saturating_sub(d.date) as f64 / 86400.0;
            let value = 2.0 * f64::from(d.sessions.min(12))
                + (f64::from(d.seconds.min(3600)) / 60.0).ln_1p() * 0.4;
            score += value * 2f64.powf(-age / cfg.half_life_days);
            if d.sessions > 0 || d.seconds >= 10 {
                active_days.insert(d.date);
            }
            launches = launches.saturating_add(d.launches);
            seconds = seconds.saturating_add(d.seconds);
        }
        let enough = active_days.len() >= cfg.min_days
            || (seed_is_valid && time.saturating_sub(app.seed_at) <= 30 * 86400);
        let (eligible, reason) = if !app.valid {
            (false, "Исходный ярлык или цель недоступны")
        } else if app.policy == Policy::Never {
            (false, "Исключено вручную")
        } else if app.policy == Policy::Always {
            (true, "Постоянно: ручной выбор")
        } else if cfg.respect_taskbar_pins && app.pinned {
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
    output.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.name.cmp(&b.name))
            .then(a.id.cmp(&b.id))
    });
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
    let score = |id: &str| scores.get(id).map_or(0.0, |r| r.score);
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
    let mut seen = HashSet::new();
    let mut result: Vec<String> = state
        .owned
        .iter()
        .filter(|o| {
            allowed(&o.app)
                || by_id.get(o.app.as_str()).is_some_and(|a| {
                    a.valid && a.policy == Policy::Auto && !(cfg.respect_taskbar_pins && a.pinned)
                })
        })
        .filter(|o| seen.insert(o.app.clone()))
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
            .min_by(|(_, a), (_, b)| score(a).total_cmp(&score(b)))
            .map(|(i, _)| i);
        if let Some(i) = worst {
            result.remove(i);
        } else {
            break;
        }
    }
    let mut changes = 0;
    for candidate in ranking
        .iter()
        .filter(|r| r.eligible && by_id.contains_key(r.id.as_str()))
    {
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
            .min_by(|(_, a), (_, b)| score(a).total_cmp(&score(b)))
            .map(|(i, id)| (i, score(id)));
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
            file_id: None,
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
    #[test]
    fn disabling_pin_filter_takes_effect_before_a_rescan() {
        let mut pinned = app("pinned", 20.0);
        pinned.pinned = true;
        let cfg = Config {
            respect_taskbar_pins: false,
            ..Default::default()
        };
        assert!(rank(&[pinned.clone()], &[], &cfg, now())[0].eligible);
        pinned.seed = 0.0;
        let state = State {
            owned: vec![owned("pinned", false)],
            ..Default::default()
        };
        assert_eq!(
            select(
                &[pinned.clone()],
                &rank(&[pinned], &[], &cfg, now()),
                &state,
                &cfg,
                false
            ),
            ["pinned"]
        );
    }
    #[test]
    fn invalid_or_future_history_cannot_unlock_placement() {
        let timestamp = now();
        for (seed, seed_at) in [
            (f64::NAN, timestamp),
            (f64::INFINITY, timestamp),
            (-4.0, timestamp),
            (10.0, timestamp + 86400),
        ] {
            let a = App {
                id: "invalid".into(),
                valid: true,
                seed,
                seed_at,
                ..Default::default()
            };
            let ranked = rank(&[a], &[], &Config::default(), timestamp);
            assert!(!ranked[0].eligible);
            assert_eq!(ranked[0].score, 0.0);
        }
    }
    #[test]
    fn aggregate_counters_saturate_and_extreme_dates_do_not_overflow() {
        let days = vec![
            Day {
                app: "a".into(),
                date: now(),
                launches: u32::MAX,
                seconds: u32::MAX,
                ..Default::default()
            },
            Day {
                app: "a".into(),
                date: now() - 86400,
                launches: 1,
                seconds: 1,
                ..Default::default()
            },
            Day {
                app: "a".into(),
                date: i64::MIN,
                sessions: 900,
                ..Default::default()
            },
        ];
        let ranking = rank(&[app("a", 0.0)], &days, &Config::default(), now());
        assert_eq!(ranking[0].launches, u32::MAX);
        assert_eq!(ranking[0].seconds, u32::MAX);
        assert!(ranking[0].score.is_finite());
    }
    #[test]
    fn indexed_history_preserves_scores_filters_and_order_at_time_boundaries() {
        let time = 100 * 86400;
        let cfg = Config {
            min_days: 2,
            ..Default::default()
        };
        let apps: Vec<_> = [
            ("alpha", 0.0, 0, Policy::Auto, false),
            ("beta", 100.0, time - 86400, Policy::Auto, false),
            ("future", 999.0, time + 1, Policy::Auto, false),
            ("manual", 0.0, 0, Policy::Always, true),
            ("never", 200.0, time, Policy::Never, false),
            ("pinned", 150.0, time, Policy::Auto, true),
        ]
        .into_iter()
        .map(|(id, seed, seed_at, policy, pinned)| App {
            id: id.into(),
            name: id.into(),
            seed,
            seed_at,
            policy,
            pinned,
            valid: true,
            ..Default::default()
        })
        .collect();
        let days: Vec<_> = [
            ("alpha", time - 86400, 2, 1, 60),
            ("beta", time, 0, u32::MAX, 3600),
            ("alpha", time - 2 * 86400, 20, 2, 7200),
            ("missing", time, u32::MAX, u32::MAX, u32::MAX),
            ("alpha", time - 86400, 1, 3, 30),
            ("beta", time - 90 * 86400, 99, 99, 99),
            ("beta", time + 1, 99, 99, 99),
            ("alpha", i64::MIN, 99, 99, 99),
            ("beta", time - 90 * 86400 + 1, 1, 1, 10),
            ("future", time, 0, 100, 0),
        ]
        .into_iter()
        .map(|(app, date, sessions, launches, seconds)| Day {
            app: app.into(),
            date,
            sessions,
            launches,
            seconds,
        })
        .collect();
        let ranking = rank(&apps, &days, &cfg, time);
        assert_eq!(
            ranking
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            ["never", "pinned", "beta", "alpha", "future", "manual"]
        );
        let alpha = &ranking[3];
        let expected_alpha = (4.0 + 2.0f64.ln() * 0.4) * 2.0f64.powf(-1.0 / 14.0)
            + (24.0 + 61.0f64.ln() * 0.4) * 2.0f64.powf(-2.0 / 14.0)
            + (2.0 + 1.5f64.ln() * 0.4) * 2.0f64.powf(-1.0 / 14.0);
        assert!((alpha.score - expected_alpha).abs() < 1e-12);
        assert_eq!((alpha.days, alpha.launches, alpha.seconds), (2, 6, 7290));
        assert!(alpha.eligible);
        let beta = &ranking[2];
        let expected_beta = 100.0 * 2.0f64.powf(-1.0 / 14.0)
            + 61.0f64.ln() * 0.4
            + (2.0 + (10.0f64 / 60.0).ln_1p() * 0.4) * 2.0f64.powf(-(90.0 - 1.0 / 86400.0) / 14.0);
        assert!((beta.score - expected_beta).abs() < 1e-12);
        assert_eq!(
            (beta.days, beta.launches, beta.seconds),
            (2, u32::MAX, 3610)
        );
        assert!(beta.eligible);
        assert!(!ranking[0].eligible && !ranking[1].eligible && !ranking[4].eligible);
        assert_eq!(
            (ranking[4].score, ranking[4].days, ranking[4].launches),
            (0.0, 0, 100)
        );
        assert!(ranking[5].eligible);
    }
    #[test]
    fn old_duplicate_ownership_does_not_consume_extra_slots() {
        let apps = [app("a", 10.0), app("b", 20.0)];
        let cfg = Config {
            slots: 2,
            ..Default::default()
        };
        let state = State {
            owned: vec![owned("a", false), owned("a", false)],
            ..Default::default()
        };
        assert_eq!(
            select(&apps, &rank(&apps, &[], &cfg, now()), &state, &cfg, false).len(),
            2
        );
    }
    #[test]
    fn incomplete_ranking_does_not_panic_or_choose_unknown_apps() {
        let apps = [app("a", 10.0), app("b", 20.0)];
        let state = State {
            owned: vec![owned("a", false), owned("b", false)],
            ..Default::default()
        };
        let cfg = Config {
            slots: 1,
            ..Default::default()
        };
        assert_eq!(select(&apps, &[], &state, &cfg, false).len(), 1);
        let unknown = Ranked {
            id: "foreign".into(),
            name: "Foreign".into(),
            score: 100.0,
            days: 7,
            launches: 0,
            seconds: 0,
            eligible: true,
            reason: String::new(),
        };
        assert!(select(&[], &[unknown], &State::default(), &cfg, true).is_empty());
    }
    #[test]
    fn windows_unc_normalization_and_url_identity_are_distinct() {
        assert_eq!(
            norm(r"\\?\UNC\Server\Share\App.exe"),
            norm(r"\\server\share\APP.exe")
        );
        assert_eq!(
            target_identity("HTTPS://Example.COM/Path?token=Ab%20CD"),
            "https://example.com/Path?token=Ab%20CD"
        );
        assert_ne!(
            target_identity("https://example.com/Path"),
            target_identity("https://example.com/path")
        );
        assert_ne!(
            target_identity("https://User:Pass@EXAMPLE.COM/x"),
            target_identity("https://user:pass@example.com/x")
        );
        assert_eq!(
            target_identity(r"C:\APP.EXE"),
            target_identity(r"c:\app.exe")
        );
    }
    #[test]
    fn protocol_shortcuts_preserve_case_sensitive_parameters_and_percent_sequences() {
        let original = "obsidian://open?vault=Research&file=Topic%20A%20B&token=Secret";
        assert_eq!(target_identity(original), original);
        assert_eq!(
            target_identity("OBSIDIAN://open?vault=Research&file=Topic"),
            "obsidian://open?vault=Research&file=Topic"
        );
        assert_ne!(
            target_identity(original),
            target_identity("obsidian://open?vault=research&file=topic%20a%20b&token=secret")
        );
        assert_eq!(
            target_identity("custom://open?value=%PATH%"),
            "custom://open?value=%PATH%"
        );
    }
    #[test]
    fn opaque_uri_payloads_are_distinct_from_windows_drive_paths() {
        assert_eq!(
            target_identity("MAILTO:Name@Example.com?subject=Topic%20A&body=%PATH%"),
            "mailto:Name@Example.com?subject=Topic%20A&body=%PATH%"
        );
        assert_ne!(
            target_identity("mailto:Name@Example.com?subject=Topic"),
            target_identity("mailto:name@example.com?subject=topic")
        );
        for (source, expected) in [
            ("MS-SETTINGS:Display", "ms-settings:Display"),
            (
                "OneNote:Notebook/Section%20A",
                "onenote:Notebook/Section%20A",
            ),
            (
                "Custom+App-1.2:CaseSensitive",
                "custom+app-1.2:CaseSensitive",
            ),
        ] {
            assert_eq!(target_identity(source), expected);
        }
        assert_eq!(
            target_identity(r"C:\Folder\App.EXE"),
            target_identity(r"c:\folder\app.exe")
        );
        assert_eq!(
            target_identity("C:Relative.EXE"),
            target_identity("c:relative.exe")
        );
        assert_eq!(
            target_identity("1invalid:Payload"),
            norm("1invalid:Payload")
        );
    }
    #[test]
    fn old_owned_shortcuts_migrate_without_claiming_an_identity() {
        let previous = r#"{"app":"app","path":"C:\\Desktop\\App.lnk","sha":"sha","position":{"x":22,"y":2},"manual":false,"since":1}"#;
        let owned: Owned = serde_json::from_str(previous).unwrap();
        assert_eq!(owned.app, "app");
        assert!(owned.file_id.is_none());
    }
}
