use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_TITLE: usize = 120;
const MAX_BODY: usize = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct QuietHours {
    pub enabled: bool,
    pub from: String,
    pub to: String,
}

impl Default for QuietHours {
    fn default() -> Self {
        Self {
            enabled: false,
            from: "22:00".to_string(),
            to: "08:00".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// Master switch, toggled by "Pause reminders". Survives restarts.
    pub enabled: bool,
    /// Minutes past the hour at which to remind, e.g. [20, 50].
    pub minutes: Vec<u32>,
    pub title: String,
    pub body: String,
    pub sound: bool,
    /// Default duration of the Snooze action, in minutes.
    pub snooze_minutes: u32,
    pub auto_start: bool,
    pub weekdays_only: bool,
    /// Reminders are suppressed inside this window (may wrap past midnight).
    pub quiet_hours: QuietHours,
    /// macOS only: print the countdown next to the menu bar icon.
    pub show_time_in_menu_bar: bool,
    /// Take over every screen with a full-screen break window when a reminder fires.
    pub overlay: bool,
    /// How long the break window stays up, in seconds.
    pub overlay_seconds: u32,
    /// Internal: whether the settings window has been shown at least once.
    pub has_launched: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            minutes: vec![50],
            title: "Time to stand up".to_string(),
            body: "Get up, stretch and move around for a couple of minutes.".to_string(),
            sound: true,
            snooze_minutes: 60,
            auto_start: true,
            weekdays_only: false,
            quiet_hours: QuietHours::default(),
            show_time_in_menu_bar: false,
            overlay: true,
            overlay_seconds: 60,
            has_launched: false,
        }
    }
}

fn is_time_string(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 5 || bytes[2] != b':' {
        return false;
    }
    let hours: u32 = match value[0..2].parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let minutes: u32 = match value[3..5].parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    hours <= 23 && minutes <= 59
}

fn clean_text(value: &str, fallback: &str, max: usize) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    trimmed.chars().take(max).collect()
}

impl Config {
    /// Coerces anything read from disk (or sent by the settings window) into a
    /// valid config, so no other code has to defend against bad values.
    pub fn sanitized(mut self) -> Self {
        let defaults = Config::default();

        self.minutes.retain(|m| *m <= 59);
        self.minutes.sort_unstable();
        self.minutes.dedup();

        self.title = clean_text(&self.title, &defaults.title, MAX_TITLE);
        self.body = clean_text(&self.body, &defaults.body, MAX_BODY);
        self.snooze_minutes = self.snooze_minutes.clamp(1, 24 * 60);
        // A break shorter than 5s is not a break; longer than 15 min is a trap.
        self.overlay_seconds = self.overlay_seconds.clamp(5, 15 * 60);

        if !is_time_string(&self.quiet_hours.from) {
            self.quiet_hours.from = defaults.quiet_hours.from.clone();
        }
        if !is_time_string(&self.quiet_hours.to) {
            self.quiet_hours.to = defaults.quiet_hours.to;
        }
        self
    }

    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(raw) => match serde_json::from_str::<Config>(&raw) {
                Ok(config) => config.sanitized(),
                Err(error) => {
                    eprintln!("[config] {} is not valid, using defaults: {error}", path.display());
                    Config::default()
                }
            },
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("[config] could not read {}: {error}", path.display());
                }
                Config::default()
            }
        }
    }

    /// Writes via a temp file + rename, so a crash mid-write cannot truncate it.
    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                eprintln!("[config] could not create {}: {error}", parent.display());
                return;
            }
        }
        let json = match serde_json::to_string_pretty(self) {
            Ok(value) => format!("{value}\n"),
            Err(error) => {
                eprintln!("[config] could not serialise: {error}");
                return;
            }
        };
        let temp: PathBuf = path.with_extension("json.tmp");
        if let Err(error) = fs::write(&temp, json) {
            eprintln!("[config] could not write {}: {error}", temp.display());
            return;
        }
        if let Err(error) = fs::rename(&temp, path) {
            eprintln!("[config] could not replace {}: {error}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_out_of_range_minutes_and_sorts() {
        let config = Config {
            minutes: vec![50, 20, 90, 20, 0],
            ..Default::default()
        }
        .sanitized();
        assert_eq!(config.minutes, vec![0, 20, 50]);
    }

    #[test]
    fn falls_back_on_blank_text_and_bad_times() {
        let config = Config {
            title: "   ".to_string(),
            quiet_hours: QuietHours {
                enabled: true,
                from: "25:00".to_string(),
                to: "08:00".to_string(),
            },
            ..Default::default()
        }
        .sanitized();
        assert_eq!(config.title, "Time to stand up");
        assert_eq!(config.quiet_hours.from, "22:00");
        assert_eq!(config.quiet_hours.to, "08:00");
    }

    #[test]
    fn clamps_snooze() {
        let config = Config { snooze_minutes: 0, ..Default::default() }.sanitized();
        assert_eq!(config.snooze_minutes, 1);
        let config = Config { snooze_minutes: 99_999, ..Default::default() }.sanitized();
        assert_eq!(config.snooze_minutes, 1440);
    }

    #[test]
    fn clamps_overlay_seconds() {
        let config = Config { overlay_seconds: 0, ..Default::default() }.sanitized();
        assert_eq!(config.overlay_seconds, 5);
        let config = Config { overlay_seconds: 99_999, ..Default::default() }.sanitized();
        assert_eq!(config.overlay_seconds, 900);
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let config: Config = serde_json::from_str(r#"{"minutes":[7],"nope":true}"#).unwrap();
        assert_eq!(config.minutes, vec![7]);
        assert_eq!(config.snooze_minutes, 60);
        assert!(config.enabled);
    }
}
