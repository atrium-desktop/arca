//! Human-readable formatting for sizes and timestamps.
//!
//! Kept dependency-free on purpose: the timestamp conversion is a plain
//! civil-date calculation (UTC), good enough for a file list.

/// `"512 B"`, `"1.5 KB"`, `"2.0 MB"`, …
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes < TB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    }
}

/// `"2026-07-18 12:11"` (UTC) for a Unix timestamp; empty for `ts <= 0`.
pub fn format_time(ts: i64) -> String {
    if ts <= 0 {
        return String::new();
    }
    let days = ts.div_euclid(86400);
    let secs = ts.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        y,
        m,
        d,
        secs / 3600,
        (secs % 3600) / 60
    )
}

/// Days since the Unix epoch → (year, month, day). Howard Hinnant's
/// civil-from-days algorithm; handles pre-epoch dates correctly.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(2 * 1024 * 1024), "2.0 MB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.0 GB");
        assert_eq!(format_size(2 * 1024u64.pow(4)), "2.0 TB");
    }

    #[test]
    fn times() {
        assert_eq!(format_time(0), "");
        assert_eq!(format_time(-5), "");
        assert_eq!(format_time(86400), "1970-01-02 00:00");
        // 2024-02-29 12:34 UTC
        assert_eq!(format_time(1_709_210_040), "2024-02-29 12:34");
        // 2000-01-01 00:00 UTC
        assert_eq!(format_time(946_684_800), "2000-01-01 00:00");
    }

    #[test]
    fn civil_roundtrip_leap_years() {
        // 1900 is not a leap year, 2000 is.
        assert_eq!(civil_from_days(-25567), (1900, 1, 1));
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(10956), (1999, 12, 31));
        assert_eq!(civil_from_days(10957), (2000, 1, 1));
        assert_eq!(civil_from_days(11016), (2000, 2, 29));
    }
}
