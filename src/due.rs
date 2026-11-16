//! Due-date language parsing.
//!
//! This module is pure: it never reads the clock unless a caller asks for the
//! convenience wrappers, and it has no dependency on the TUI or file system.
//! Parsing relative expressions always takes an explicit `today`, which keeps
//! the behaviour testable and deterministic.

use chrono::{Datelike, Local, NaiveDate, Weekday};

/// Parse a due-date expression using the current local date.
pub fn resolve_relative_date(input: &str) -> Option<NaiveDate> {
    resolve_relative_date_from(input, Local::now().date_naive())
}

/// Parse a due-date expression relative to `today`.
///
/// Recognised forms are documented in the README: absolute dates, single-word
/// shortcuts, weekdays, `next`/`this` + weekday, signed offsets, `in N units`,
/// and month-name + day.
pub fn resolve_relative_date_from(input: &str, today: NaiveDate) -> Option<NaiveDate> {
    let cleaned = input.trim().to_lowercase();
    if cleaned.is_empty() {
        return None;
    }

    match cleaned.as_str() {
        "today" | "tod" | "t" => Some(today),
        "tomorrow" | "tmr" | "tmw" | "tom" => today.checked_add_signed(days(1)?),
        "next week" | "nw" | "nextweek" => today.checked_add_signed(days(7)?),
        _ => parse_compound(&cleaned, today),
    }
}

/// Parse a weekday name or abbreviation (already lowercased).
fn parse_weekday(s: &str) -> Option<Weekday> {
    match s {
        "sun" | "sunday" => Some(Weekday::Sun),
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tuesday" => Some(Weekday::Tue),
        "wed" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        _ => None,
    }
}

/// Try the multi-word and numeric forms that are not single-word shortcuts.
fn parse_compound(cleaned: &str, today: NaiveDate) -> Option<NaiveDate> {
    if let Some(rest) = cleaned.strip_prefix("this ")
        && let Some(weekday) = parse_weekday(rest)
    {
        return if today.weekday() == weekday {
            Some(today)
        } else {
            next_weekday(today, weekday)
        };
    }

    if let Some(rest) = cleaned.strip_prefix("next ")
        && let Some(weekday) = parse_weekday(rest)
    {
        return next_weekday(today, weekday);
    }

    parse_relative_offset(cleaned, today)
        .or_else(|| parse_in_offset(cleaned, today))
        .or_else(|| parse_month_day(cleaned, today))
        .or_else(|| parse_absolute(cleaned))
        .or_else(|| parse_weekday(cleaned).and_then(|weekday| next_weekday(today, weekday)))
}

/// Parse an absolute date in one of the supported separators.
fn parse_absolute(cleaned: &str) -> Option<NaiveDate> {
    ["%Y-%m-%d", "%Y/%m/%d", "%Y.%m.%d"]
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(cleaned, format).ok())
}

/// A relative unit of time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Days,
    Weeks,
    Months,
    Years,
}

impl Unit {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "d" | "day" | "days" => Some(Self::Days),
            "w" | "week" | "weeks" => Some(Self::Weeks),
            "m" | "month" | "months" => Some(Self::Months),
            "y" | "year" | "years" => Some(Self::Years),
            _ => None,
        }
    }
}

/// Apply a signed amount of `unit` to `date`.
fn shift(date: NaiveDate, amount: i64, unit: Unit) -> Option<NaiveDate> {
    match unit {
        Unit::Days => checked_days(date, amount),
        Unit::Weeks => checked_days(date, amount.checked_mul(7)?),
        Unit::Months => add_months(date, amount),
        Unit::Years => add_years(date, amount),
    }
}

/// Parse signed offsets such as `+3d`, `-2w`, `+1m`, `+1y`, or `+3 days`.
fn parse_relative_offset(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let s = s.trim();
    let (sign, rest) = if let Some(rest) = s.strip_prefix('+') {
        (1_i64, rest)
    } else {
        let rest = s.strip_prefix('-')?;
        (-1_i64, rest)
    };

    let number_end = rest
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(i, c)| i + c.len_utf8())?;
    let number: i64 = rest[..number_end].parse().ok()?;
    let unit = Unit::parse(rest[number_end..].trim())?;
    shift(today, sign.checked_mul(number)?, unit)
}

/// Parse `in N units`, accepting one optional leading minus sign.
fn parse_in_offset(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let rest = s.strip_prefix("in")?.trim_start();
    let mut parts = rest.split_whitespace();
    let number: i64 = parts.next()?.parse().ok()?;
    let unit = Unit::parse(parts.next()?)?;
    shift(today, number, unit)
}

/// Parse a month name plus day, rolling forward to the next valid year.
fn parse_month_day(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let mut parts = s.split_whitespace();
    let month = month_number(parts.next()?)?;
    let day: u32 = parts.next()?.parse().ok()?;

    // Scan a full leap cycle so `feb 29` finds the next leap year.
    for offset in 0..=8_u32 {
        let year = today.year().checked_add(offset as i32)?;
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
            && date >= today
        {
            return Some(date);
        }
    }
    None
}

/// Map a month name or abbreviation to its number.
fn month_number(s: &str) -> Option<u32> {
    match s.to_ascii_lowercase().as_str() {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

/// Return the strictly next occurrence of `target`.
fn next_weekday(from: NaiveDate, target: Weekday) -> Option<NaiveDate> {
    let current = from.weekday().num_days_from_monday();
    let wanted = target.num_days_from_monday();
    let diff = (wanted + 7 - current) % 7;
    let diff = if diff == 0 { 7 } else { diff };
    from.checked_add_signed(days(i64::from(diff))?)
}

/// Add a signed number of days, returning `None` on overflow.
fn checked_days(date: NaiveDate, amount: i64) -> Option<NaiveDate> {
    date.checked_add_signed(days(amount)?)
}

/// Build a `Duration` without panicking on out-of-range input.
fn days(amount: i64) -> Option<chrono::Duration> {
    chrono::Duration::try_days(amount)
}

/// Add calendar months, clamping to the last valid day of the target month.
fn add_months(date: NaiveDate, months: i64) -> Option<NaiveDate> {
    let total = i64::from(date.year())
        .checked_mul(12)?
        .checked_add(i64::from(date.month0()))?
        .checked_add(months)?;
    let year = i32::try_from(total.div_euclid(12)).ok()?;
    let month = u32::try_from(total.rem_euclid(12)).ok()?.checked_add(1)?;
    let day = date.day().min(last_day_of_month(year, month)?);
    NaiveDate::from_ymd_opt(year, month, day)
}

/// Add calendar years, clamping February 29 to February 28 when needed.
fn add_years(date: NaiveDate, years: i64) -> Option<NaiveDate> {
    let year = i64::from(date.year()).checked_add(years)?;
    let year = i32::try_from(year).ok()?;
    let day = date.day().min(last_day_of_month(year, date.month())?);
    NaiveDate::from_ymd_opt(year, date.month(), day)
}

/// Last valid day number for a year/month pair.
fn last_day_of_month(year: i32, month: u32) -> Option<u32> {
    let (next_year, next_month) = if month == 12 {
        (year.checked_add(1)?, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)?
        .pred_opt()
        .map(|date| date.day())
}

/// Parse a date from the start of `s`, consuming progressively more
/// whitespace-separated words. Returns the date and the byte offset just past
/// the matched text.
pub fn parse_due_token_from(s: &str, today: NaiveDate) -> Option<(NaiveDate, usize)> {
    let mut cursor = 0;
    while cursor < s.len() {
        let ch = s[cursor..].chars().next()?;
        if !ch.is_whitespace() {
            break;
        }
        cursor += ch.len_utf8();
    }

    while cursor < s.len() {
        let ch = s[cursor..].chars().next()?;
        if !ch.is_whitespace() {
            cursor += ch.len_utf8();
            continue;
        }

        if let Some(date) = resolve_relative_date_from(&s[..cursor], today) {
            return Some((date, cursor));
        }
        while cursor < s.len() {
            let ch = s[cursor..].chars().next()?;
            if !ch.is_whitespace() {
                break;
            }
            cursor += ch.len_utf8();
        }
    }

    resolve_relative_date_from(s, today).map(|date| (date, s.len()))
}

/// Extract one inline `due:` token from task text.
///
/// Candidates must start the text or be preceded by whitespace, so `overdue:`
/// and URLs are left alone. The last *parseable* candidate wins; if none parse,
/// the text is returned unchanged.
pub fn extract_inline_due(text: &str) -> (String, Option<NaiveDate>) {
    extract_inline_due_from(text, Local::now().date_naive())
}

/// [`extract_inline_due`] with an injected `today`.
pub fn extract_inline_due_from(text: &str, today: NaiveDate) -> (String, Option<NaiveDate>) {
    let candidates: Vec<usize> = text
        .match_indices("due:")
        .filter(|(index, _)| {
            *index == 0
                || text[..*index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace)
        })
        .map(|(index, _)| index)
        .collect();

    let Some(index) = candidates
        .into_iter()
        .rev()
        .find(|index| parse_due_token_from(&text[index + "due:".len()..], today).is_some())
    else {
        return (text.to_string(), None);
    };

    let after = &text[index + "due:".len()..];
    let Some((date, consumed)) = parse_due_token_from(after, today) else {
        return (text.to_string(), None);
    };

    let before = text[..index].trim_end();
    let remaining = after[consumed..].trim_start();
    let cleaned = match (before.is_empty(), remaining.is_empty()) {
        (true, _) => remaining.to_string(),
        (_, true) => before.to_string(),
        (false, false) => format!("{before} {remaining}"),
    };
    (cleaned, Some(date))
}
