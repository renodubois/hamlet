use super::format_timestamp;
use chrono::{DateTime, FixedOffset};

fn at(value: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(value).unwrap()
}

#[test]
fn calendar_dates_use_the_viewers_timezone_including_year_changes() {
    let now = at("2026-01-01T00:30:00-05:00");
    for (created, label) in [
        ("2026-01-01T05:00:00Z", "30 minutes ago"),
        ("2026-01-01T04:59:59Z", "yesterday at 11:59 PM"),
        ("2025-12-31T05:00:00Z", "yesterday at 12:00 AM"),
        ("2025-12-31T04:59:59Z", "12/30/2025 11:59 PM"),
    ] {
        assert_eq!(format_timestamp(created, &now).label, label, "{created}");
    }
    // A previous calendar date remains yesterday even when more than 24 hours old.
    assert_eq!(
        format_timestamp("2026-03-13T00:01:00Z", &at("2026-03-14T23:59:00Z")).label,
        "yesterday at 12:01 AM"
    );
    // UTC yesterday can be local today in a positive offset.
    assert_eq!(
        format_timestamp("2025-12-31T23:45:00Z", &at("2026-01-01T02:00:00+02:00")).label,
        "15 minutes ago"
    );
}

#[test]
fn exact_local_time_includes_seconds_and_formats_noon_and_midnight() {
    let now = at("2026-03-16T14:00:00-05:00");
    for (created, label, exact) in [
        (
            "2026-03-14T18:41:08Z",
            "03/14/2026 1:41 PM",
            "03/14/2026 1:41:08 PM",
        ),
        (
            "2026-03-14T05:00:00Z",
            "03/14/2026 12:00 AM",
            "03/14/2026 12:00:00 AM",
        ),
        (
            "2026-03-14T17:00:00Z",
            "03/14/2026 12:00 PM",
            "03/14/2026 12:00:00 PM",
        ),
        (
            "2026-03-16T19:00:00.001Z",
            "just now",
            "03/16/2026 2:00:00 PM",
        ),
        ("2027-01-01T05:00:08Z", "just now", "01/01/2027 12:00:08 AM"),
    ] {
        let display = format_timestamp(created, &now);
        assert_eq!(display.label, label, "{created}");
        assert_eq!(display.exact, exact, "{created}");
    }
}

#[test]
fn today_rounds_down_at_minute_and_hour_thresholds() {
    let now = at("2026-03-14T14:00:00+00:00");
    for (created, label) in [
        ("2026-03-14T14:00:00Z", "just now"),
        ("2026-03-14T13:59:00.001Z", "just now"),
        ("2026-03-14T13:59:00Z", "1 minute ago"),
        ("2026-03-14T13:58:00Z", "2 minutes ago"),
        ("2026-03-14T13:54:01Z", "5 minutes ago"),
        ("2026-03-14T13:00:00.001Z", "59 minutes ago"),
        ("2026-03-14T13:00:00Z", "1 hour ago"),
        ("2026-03-14T12:00:00.001Z", "1 hour ago"),
        ("2026-03-14T12:00:00Z", "2 hours ago"),
    ] {
        assert_eq!(format_timestamp(created, &now).label, label, "{created}");
    }
}
