//! UTC timestamps without a date-time dependency.

use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after 1970")
        .as_secs()
}

/// Current UTC time as `YYYYMMDD-HHMMSS`, safe for use in file names.
pub fn file_stamp() -> String {
    format_stamp(now_secs())
}

/// Current UTC time as `YYYY-MM-DD HH:MM:SS`, for records.
pub fn date_time() -> String {
    format_date_time(now_secs())
}

fn parts(unix_secs: u64) -> (i64, u32, u32, u64, u64, u64) {
    let (y, m, d) = civil_from_days((unix_secs / 86_400) as i64);
    let rem = unix_secs % 86_400;
    (y, m, d, rem / 3600, rem / 60 % 60, rem % 60)
}

fn format_stamp(unix_secs: u64) -> String {
    let (y, m, d, hh, mm, ss) = parts(unix_secs);
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

fn format_date_time(unix_secs: u64) -> String {
    let (y, m, d, hh, mm, ss) = parts(unix_secs);
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_instants() {
        assert_eq!(format_stamp(0), "19700101-000000");
        // 2000-02-29 12:34:56 UTC (leap day).
        assert_eq!(format_stamp(951_827_696), "20000229-123456");
        // 2026-11-05 13:00:00 UTC.
        assert_eq!(format_stamp(1_793_883_600), "20261105-130000");
        assert_eq!(format_date_time(951_827_696), "2000-02-29 12:34:56");
    }
}
