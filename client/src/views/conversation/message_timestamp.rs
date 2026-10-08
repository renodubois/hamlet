//! Display-only timestamp labels in the viewer's timezone.
use chrono::{DateTime, TimeZone};

pub(super) struct MessageTimestamp {
    pub label: String,
    pub exact: String,
}

pub(super) fn format_timestamp<Tz: TimeZone>(
    created_at: &str,
    now: &DateTime<Tz>,
) -> MessageTimestamp {
    let Ok(created) = DateTime::parse_from_rfc3339(created_at) else {
        // API decoding rejects invalid timestamps; retain a readable fallback for fixtures.
        return MessageTimestamp {
            label: created_at.into(),
            exact: created_at.into(),
        };
    };
    let elapsed = now.clone().signed_duration_since(created);
    let created = created.with_timezone(&now.timezone());
    let seconds = elapsed.num_seconds();
    let label = if created > *now {
        "just now".into()
    } else if Some(created.date_naive()) == now.date_naive().pred_opt() {
        format!("yesterday at {}", created.naive_local().format("%-I:%M %p"))
    } else if created.date_naive() != now.date_naive() {
        created
            .naive_local()
            .format("%m/%d/%Y %-I:%M %p")
            .to_string()
    } else if seconds < 60 {
        "just now".into()
    } else {
        let (count, unit) = if seconds < 3600 {
            (seconds / 60, "minute")
        } else {
            (seconds / 3600, "hour")
        };
        format!("{count} {unit}{} ago", if count == 1 { "" } else { "s" })
    };
    MessageTimestamp {
        label,
        exact: created
            .naive_local()
            .format("%m/%d/%Y %-I:%M:%S %p")
            .to_string(),
    }
}

#[cfg(test)]
#[path = "tests/message_timestamp.rs"]
mod tests;
