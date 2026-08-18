use crate::config::{Config, QuietHours};
use chrono::{DateTime, Datelike, Duration, Local, Timelike, Weekday};
use serde::Serialize;

/// Snapshot of the schedule, shaped for the tray menu and the settings window.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    pub has_times: bool,
    /// Epoch milliseconds, or null when not snoozed.
    pub snoozed_until: Option<i64>,
    pub in_quiet_hours: bool,
    pub off_day: bool,
    pub next_fire_at: Option<i64>,
    pub next_fire_clock: Option<String>,
    pub next_fire_delay: Option<String>,
}

fn parse_hour_minute(value: &str) -> Option<u32> {
    let (hours, minutes) = value.trim().split_once(':')?;
    let hours: u32 = hours.parse().ok()?;
    let minutes: u32 = minutes.parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(hours * 60 + minutes)
}

/// Quiet hours may wrap past midnight (e.g. 22:00 -> 08:00).
pub fn in_quiet_hours(at: &DateTime<Local>, quiet: &QuietHours) -> bool {
    if !quiet.enabled {
        return false;
    }
    let from = parse_hour_minute(&quiet.from).unwrap_or(22 * 60);
    let to = parse_hour_minute(&quiet.to).unwrap_or(8 * 60);
    if from == to {
        return false;
    }
    let now = at.hour() * 60 + at.minute();
    if from < to {
        now >= from && now < to
    } else {
        now >= from || now < to
    }
}

pub fn is_active_day(at: &DateTime<Local>, config: &Config) -> bool {
    if !config.weekdays_only {
        return true;
    }
    !matches!(at.weekday(), Weekday::Sat | Weekday::Sun)
}

pub fn format_clock(at: &DateTime<Local>) -> String {
    format!("{:02}:{:02}", at.hour(), at.minute())
}

/// "in 2 h 5 min" style copy for the tray menu and the settings window.
pub fn format_delay(milliseconds: i64) -> String {
    let total = ((milliseconds as f64) / 60_000.0).round().max(0.0) as i64;
    if total < 1 {
        return "in less than a minute".to_string();
    }
    if total < 60 {
        return format!("in {total} min");
    }
    let hours = total / 60;
    let minutes = total % 60;
    if minutes == 0 {
        format!("in {hours} h")
    } else {
        format!("in {hours} h {minutes} min")
    }
}

fn top_of_hour(at: &DateTime<Local>) -> Option<DateTime<Local>> {
    at.with_minute(0)?.with_second(0)?.with_nanosecond(0)
}

/// Next moment a reminder will actually be delivered, accounting for snooze,
/// quiet hours and the weekday filter. `barrier` is the instant everything must
/// come strictly after — normally `now`, or the end of an active snooze.
pub fn next_fire_at(
    config: &Config,
    from: DateTime<Local>,
    barrier: DateTime<Local>,
) -> Option<DateTime<Local>> {
    if !config.enabled || config.minutes.is_empty() {
        return None;
    }

    let mut cursor = top_of_hour(&from)?;
    // Two weeks of hours is far more than any schedule needs; it only runs long
    // when quiet hours plus weekdays-only leave a big gap.
    for _ in 0..(24 * 15) {
        for minute in &config.minutes {
            // `with_minute` returns None inside a DST gap, which we simply skip.
            if let Some(slot) = cursor.with_minute(*minute) {
                if slot <= barrier
                    || !is_active_day(&slot, config)
                    || in_quiet_hours(&slot, &config.quiet_hours)
                {
                    continue;
                }
                return Some(slot);
            }
        }
        cursor = top_of_hour(&(cursor + Duration::hours(1)))?;
    }
    None
}

/// Identifies the exact minute a reminder belongs to, so we never fire twice for it.
pub fn slot_key(at: &DateTime<Local>) -> String {
    at.format("%Y-%m-%dT%H:%M").to_string()
}

/// Whether a reminder should be delivered right now.
pub fn should_fire(config: &Config, now: &DateTime<Local>, snoozed_until: Option<DateTime<Local>>) -> bool {
    if !config.enabled || !config.minutes.contains(&now.minute()) {
        return false;
    }
    if snoozed_until.is_some_and(|until| *now < until) {
        return false;
    }
    is_active_day(now, config) && !in_quiet_hours(now, &config.quiet_hours)
}

pub fn build_status(
    config: &Config,
    now: DateTime<Local>,
    snoozed_until: Option<DateTime<Local>>,
) -> Status {
    let active_snooze = snoozed_until.filter(|until| now < *until);
    let barrier = active_snooze.unwrap_or(now).max(now);
    let next = next_fire_at(config, now, barrier);

    Status {
        enabled: config.enabled,
        has_times: !config.minutes.is_empty(),
        snoozed_until: active_snooze.map(|at| at.timestamp_millis()),
        in_quiet_hours: in_quiet_hours(&now, &config.quiet_hours),
        off_day: !is_active_day(&now, config),
        next_fire_at: next.map(|at| at.timestamp_millis()),
        next_fire_clock: next.as_ref().map(format_clock),
        next_fire_delay: next
            .map(|at| format_delay(at.timestamp_millis() - now.timestamp_millis())),
    }
}

/// The single line describing state, reused by the tray title and tooltip.
pub fn status_line(status: &Status) -> String {
    if !status.enabled {
        return "Reminders are paused".to_string();
    }
    if !status.has_times {
        return "No reminder times set".to_string();
    }
    if let Some(until) = status.snoozed_until {
        if let Some(at) = DateTime::from_timestamp_millis(until) {
            return format!("Snoozed until {}", format_clock(&at.with_timezone(&Local)));
        }
    }
    match (&status.next_fire_clock, &status.next_fire_delay) {
        (Some(clock), Some(delay)) => format!("Next: {clock} ({delay})"),
        _ => "Nothing scheduled".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(text: &str) -> DateTime<Local> {
        DateTime::parse_from_rfc3339(text)
            .map(|value| value.with_timezone(&Local))
            .unwrap_or_else(|_| {
                let naive = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S")
                    .expect("valid test timestamp");
                Local.from_local_datetime(&naive).unwrap()
            })
    }

    fn config(minutes: Vec<u32>) -> Config {
        Config { minutes, ..Default::default() }
    }

    fn next(config: &Config, from: &str) -> String {
        let from = at(from);
        next_fire_at(config, from, from)
            .map(|value| format_clock(&value))
            .unwrap_or_else(|| "none".to_string())
    }

    #[test]
    fn fires_once_an_hour() {
        let config = config(vec![50]);
        assert_eq!(next(&config, "2026-08-18T12:00:00"), "12:50");
        assert_eq!(next(&config, "2026-08-18T12:50:30"), "13:50");
        assert_eq!(next(&config, "2026-08-18T12:51:00"), "13:50");
    }

    #[test]
    fn fires_twice_an_hour() {
        let config = config(vec![20, 50]);
        assert_eq!(next(&config, "2026-08-18T12:00:00"), "12:20");
        assert_eq!(next(&config, "2026-08-18T12:25:00"), "12:50");
        assert_eq!(next(&config, "2026-08-18T12:55:00"), "13:20");
    }

    #[test]
    fn skips_quiet_hours() {
        let mut config = config(vec![50]);
        config.quiet_hours = QuietHours {
            enabled: true,
            from: "22:00".to_string(),
            to: "08:00".to_string(),
        };
        assert_eq!(next(&config, "2026-08-18T21:00:00"), "21:50");
        assert_eq!(next(&config, "2026-08-18T21:55:00"), "08:50");
        assert_eq!(next(&config, "2026-08-18T23:00:00"), "08:50");
        assert_eq!(next(&config, "2026-08-19T03:00:00"), "08:50");
    }

    #[test]
    fn skips_weekends_when_asked() {
        let mut config = config(vec![50]);
        config.weekdays_only = true;
        // Saturday 2026-08-22 noon -> first weekday slot is Monday 00:50.
        let from = at("2026-08-22T12:00:00");
        let slot = next_fire_at(&config, from, from).expect("a monday slot");
        assert_eq!(slot.weekday(), Weekday::Mon);
        assert_eq!(format_clock(&slot), "00:50");
    }

    #[test]
    fn honours_the_snooze_barrier() {
        let config = config(vec![50]);
        let from = at("2026-08-18T12:00:00");
        let barrier = at("2026-08-18T14:30:00");
        let slot = next_fire_at(&config, from, barrier).expect("a slot");
        assert_eq!(format_clock(&slot), "14:50");
    }

    #[test]
    fn returns_nothing_when_paused_or_empty() {
        let mut paused = config(vec![50]);
        paused.enabled = false;
        assert_eq!(next(&paused, "2026-08-18T12:00:00"), "none");
        assert_eq!(next(&config(vec![]), "2026-08-18T12:00:00"), "none");
    }

    #[test]
    fn should_fire_only_inside_a_scheduled_minute() {
        let config = config(vec![50]);
        assert!(should_fire(&config, &at("2026-08-18T12:50:00"), None));
        assert!(should_fire(&config, &at("2026-08-18T12:50:59"), None));
        assert!(!should_fire(&config, &at("2026-08-18T12:49:59"), None));
        assert!(!should_fire(&config, &at("2026-08-18T12:51:00"), None));
    }

    #[test]
    fn should_fire_respects_snooze() {
        let config = config(vec![50]);
        let now = at("2026-08-18T12:50:00");
        assert!(!should_fire(&config, &now, Some(at("2026-08-18T13:30:00"))));
        assert!(should_fire(&config, &now, Some(at("2026-08-18T12:00:00"))));
    }

    #[test]
    fn delay_copy_reads_naturally() {
        assert_eq!(format_delay(0), "in less than a minute");
        assert_eq!(format_delay(5 * 60_000), "in 5 min");
        assert_eq!(format_delay(60 * 60_000), "in 1 h");
        assert_eq!(format_delay(125 * 60_000), "in 2 h 5 min");
    }

    #[test]
    fn status_line_prefers_the_most_specific_state() {
        let config = config(vec![50]);
        let now = at("2026-08-18T12:00:00");

        let paused = build_status(&Config { enabled: false, ..config.clone() }, now, None);
        assert_eq!(status_line(&paused), "Reminders are paused");

        let empty = build_status(&Config { minutes: vec![], ..config.clone() }, now, None);
        assert_eq!(status_line(&empty), "No reminder times set");

        let normal = build_status(&config, now, None);
        assert_eq!(status_line(&normal), "Next: 12:50 (in 50 min)");

        let snoozed = build_status(&config, now, Some(at("2026-08-18T13:15:00")));
        assert_eq!(status_line(&snoozed), "Snoozed until 13:15");
        // A snooze pushes the next delivery past the slots it covers.
        assert_eq!(snoozed.next_fire_clock.as_deref(), Some("13:50"));
    }
}
