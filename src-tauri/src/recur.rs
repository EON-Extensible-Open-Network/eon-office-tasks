// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! Dates and repeating tasks, without a date library.
//!
//! Dates are civil dates (`YYYY-MM-DD`) in the person's own calendar; a task's
//! due date has no time zone, which is what "due Friday" means. Repetition is
//! the RFC 5545 RRULE subset the roadmap names: `FREQ=DAILY|WEEKLY|MONTHLY|
//! YEARLY`, `INTERVAL`, `BYDAY` (weekly), `BYMONTHDAY` (monthly). Anything
//! else is refused, not half-understood.

// Calendar arithmetic stays far inside every integer type's range for any
// date a person schedules; the casts are the algorithm's, not data's.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub y: i32,
    pub m: u32,
    pub d: u32,
}

impl Date {
    pub fn parse(text: &str) -> Option<Date> {
        let mut parts = text.split('-');
        let y = parts.next()?.parse().ok()?;
        let m = parts.next()?.parse().ok()?;
        let d = parts.next()?.parse().ok()?;
        if parts.next().is_some()
            || text.len() != 10
            || !(1..=12).contains(&m)
            || d == 0
            || d > days_in_month(y, m)
        {
            return None;
        }
        Some(Date { y, m, d })
    }

    /// Days since 1970-01-01 (Howard Hinnant's algorithm).
    pub fn days(self) -> i64 {
        let y = i64::from(self.y) - i64::from(self.m <= 2);
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let m = i64::from(self.m);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.d) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    pub fn from_days(z: i64) -> Date {
        let z = z + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let y = (yoe + era * 400 + i64::from(m <= 2)) as i32;
        Date { y, m, d }
    }

    /// 0 = Monday ... 6 = Sunday.
    pub fn weekday(self) -> u32 {
        ((self.days() + 3).rem_euclid(7)) as u32
    }

    #[must_use]
    pub fn add_days(self, n: i64) -> Date {
        Date::from_days(self.days() + n)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.y, self.m, self.d)
    }
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

const DAYS: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub freq: Freq,
    pub interval: u32,
    /// Weekly: the weekdays, 0 = Monday.
    pub by_day: Vec<u32>,
    /// Monthly: the day of the month.
    pub by_month_day: Option<u32>,
}

impl Rule {
    pub fn parse(text: &str) -> Result<Rule, String> {
        let mut freq = None;
        let mut interval = 1;
        let mut by_day = Vec::new();
        let mut by_month_day = None;
        for part in text
            .trim()
            .trim_start_matches("RRULE:")
            .split(';')
            .filter(|p| !p.is_empty())
        {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| format!("`{part}` is not KEY=VALUE"))?;
            match key {
                "FREQ" => {
                    freq = Some(match value {
                        "DAILY" => Freq::Daily,
                        "WEEKLY" => Freq::Weekly,
                        "MONTHLY" => Freq::Monthly,
                        "YEARLY" => Freq::Yearly,
                        _ => return Err(format!("FREQ={value} is not supported")),
                    });
                }
                "INTERVAL" => {
                    interval = value
                        .parse()
                        .ok()
                        .filter(|n| (1..=366).contains(n))
                        .ok_or("INTERVAL is 1-366")?;
                }
                "BYDAY" => {
                    for day in value.split(',') {
                        let at = DAYS
                            .iter()
                            .position(|d| *d == day)
                            .ok_or_else(|| format!("BYDAY={day} is not supported"))?;
                        by_day.push(at as u32);
                    }
                    by_day.sort_unstable();
                    by_day.dedup();
                }
                "BYMONTHDAY" => {
                    by_month_day = Some(
                        value
                            .parse()
                            .ok()
                            .filter(|n| (1..=31).contains(n))
                            .ok_or("BYMONTHDAY is 1-31")?,
                    );
                }
                _ => return Err(format!("{key} is not supported")),
            }
        }
        let freq = freq.ok_or("FREQ is required")?;
        if !by_day.is_empty() && freq != Freq::Weekly {
            return Err("BYDAY is supported with FREQ=WEEKLY only".into());
        }
        if by_month_day.is_some() && freq != Freq::Monthly {
            return Err("BYMONTHDAY is supported with FREQ=MONTHLY only".into());
        }
        Ok(Rule {
            freq,
            interval,
            by_day,
            by_month_day,
        })
    }

    /// The first occurrence strictly after `after`, for a series anchored at `start`.
    pub fn next(&self, start: Date, after: Date) -> Date {
        let n = i64::from(self.interval);
        match self.freq {
            Freq::Daily => {
                let gap = after.days() - start.days();
                let steps = if gap < 0 { 0 } else { gap / n + 1 };
                start.add_days(steps * n)
            }
            Freq::Weekly => {
                let days = if self.by_day.is_empty() {
                    vec![start.weekday()]
                } else {
                    self.by_day.clone()
                };
                // Weeks are counted from the Monday of the start's week.
                let week0 = start.add_days(-i64::from(start.weekday()));
                let mut day = after.add_days(1).max(start);
                loop {
                    let week = (day.days() - week0.days()).div_euclid(7);
                    if week % n == 0 && days.contains(&day.weekday()) {
                        return day;
                    }
                    day = day.add_days(1);
                }
            }
            Freq::Monthly => {
                let want = self.by_month_day.unwrap_or(start.d);
                let mut k = 0i64;
                loop {
                    let months = i64::from(start.m) - 1 + k * n;
                    let y = start.y + months.div_euclid(12) as i32;
                    let m = months.rem_euclid(12) as u32 + 1;
                    // A month without the day (31 in April) is skipped, as RFC 5545 says.
                    if want <= days_in_month(y, m) {
                        let candidate = Date { y, m, d: want };
                        if candidate > after && candidate >= start {
                            return candidate;
                        }
                    }
                    k += 1;
                }
            }
            Freq::Yearly => {
                let mut k = 0i32;
                loop {
                    let y = start.y + k * self.interval as i32;
                    if start.d <= days_in_month(y, start.m) {
                        let candidate = Date {
                            y,
                            m: start.m,
                            d: start.d,
                        };
                        if candidate > after {
                            return candidate;
                        }
                    }
                    k += 1;
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    fn d(text: &str) -> Date {
        Date::parse(text).unwrap_or(Date { y: 0, m: 1, d: 1 })
    }

    #[test]
    fn dates_round_trip_and_know_their_weekday() {
        for text in [
            "1970-01-01",
            "2000-02-29",
            "2026-11-19",
            "2100-12-31",
            "1969-07-20",
        ] {
            assert_eq!(Date::from_days(d(text).days()).to_string(), text);
        }
        assert_eq!(d("2026-11-19").weekday(), 3, "a Thursday");
        assert!(Date::parse("2026-02-29").is_none());
        assert!(Date::parse("2026-13-01").is_none());
    }

    #[test]
    fn daily_and_every_other_day() {
        let r = Rule::parse("FREQ=DAILY").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.next(d("2026-11-19"), d("2026-11-19")), d("2026-11-20"));
        let r = Rule::parse("FREQ=DAILY;INTERVAL=2").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.next(d("2026-11-19"), d("2026-11-22")), d("2026-11-23"));
    }

    #[test]
    fn weekly_on_chosen_days_and_fortnightly() {
        let r = Rule::parse("FREQ=WEEKLY;BYDAY=MO,WE,FR").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.next(d("2026-11-16"), d("2026-11-19")), d("2026-11-20"));
        assert_eq!(r.next(d("2026-11-16"), d("2026-11-20")), d("2026-11-23"));
        let r = Rule::parse("FREQ=WEEKLY;INTERVAL=2;BYDAY=TU").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.next(d("2026-11-17"), d("2026-11-17")), d("2026-12-01"));
    }

    #[test]
    fn monthly_skips_months_without_the_day() {
        let r = Rule::parse("FREQ=MONTHLY;BYMONTHDAY=31").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.next(d("2026-01-31"), d("2026-01-31")), d("2026-03-31"));
        assert_eq!(r.next(d("2026-01-31"), d("2026-03-31")), d("2026-05-31"));
        let r = Rule::parse("FREQ=MONTHLY").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.next(d("2026-11-19"), d("2026-11-19")), d("2026-12-19"));
    }

    #[test]
    fn yearly_including_leap_days() {
        let r = Rule::parse("FREQ=YEARLY").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.next(d("2024-02-29"), d("2024-02-29")), d("2028-02-29"));
        assert_eq!(r.next(d("2026-11-19"), d("2026-11-19")), d("2027-11-19"));
    }

    #[test]
    fn anything_unsupported_is_refused() {
        for bad in [
            "FREQ=HOURLY",
            "FREQ=DAILY;COUNT=3",
            "FREQ=MONTHLY;BYDAY=MO",
            "INTERVAL=2",
            "FREQ=WEEKLY;BYDAY=XX",
        ] {
            assert!(Rule::parse(bad).is_err(), "{bad}");
        }
    }
}
