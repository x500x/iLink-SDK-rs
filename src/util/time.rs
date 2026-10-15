//! Time helpers.

/// Current wall-clock time in milliseconds since the Unix epoch.
pub fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Current wall-clock time formatted as an ISO-8601 (UTC) string.
///
/// Rendered to millisecond precision to match JavaScript's `toISOString()`.
pub fn now_iso8601() -> String {
    let ms = now_millis();
    let secs = (ms / 1000) as i64;
    let millis = ms % 1000;
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);

    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Howard Hinnant's `civil_from_days` algorithm: days since 1970-01-01 becomes
/// `(year, month, day)` in the proleptic Gregorian calendar.
fn civil_from_days(z: i64) -> (i64, u64, u64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u64, d as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_shape() {
        let s = now_iso8601();
        assert_eq!(s.len(), 24, "got {s}");
        assert!(s.ends_with('Z'), "got {s}");
        assert!(s.starts_with("20"), "got {s}");
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[10..11], "T");
    }

    #[test]
    fn epoch_round_trip() {
        // 1970-01-01T00:00:00.000Z
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-01-01T00:00:00Z (days since epoch = 20454)
        assert_eq!(civil_from_days(20454), (2026, 1, 1));
        // 2000-03-01 (leap-year edge)
        assert_eq!(civil_from_days(11017), (2000, 3, 1));
        // 2024-02-29 (leap day, valid)
        assert_eq!(civil_from_days(19782), (2024, 2, 29));
    }

    #[test]
    fn known_timestamp() {
        // 2026-01-01T00:00:00.000Z == 1767225600000 ms
        let secs = 1767225600_i64 / 1000 * 1000 / 1000;
        let _ = secs;
        let ms = 1_767_225_600_000_u128;
        let days = (ms / 1000 / 86400) as i64;
        assert_eq!(civil_from_days(days), (2026, 1, 1));
    }

    #[test]
    fn pre_epoch_days_work() {
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }
}
