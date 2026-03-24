//! User display name, birthdate, age gate for NSFW catalog, birthday check.

use chrono::{Datelike, NaiveDate, Utc};

/// Returns age in full years if birthdate parses as YYYY-MM-DD; None if missing/invalid.
pub fn age_years_from_birthdate_iso(iso: Option<&str>) -> Option<u32> {
    let s = iso?.trim();
    if s.len() < 8 {
        return None;
    }
    let birth = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    let today = Utc::now().date_naive();
    if birth > today {
        return Some(0);
    }
    let mut age = today.year() - birth.year();
    let had_birthday = (today.month(), today.day()) >= (birth.month(), birth.day());
    if !had_birthday {
        age -= 1;
    }
    Some(age.max(0) as u32)
}

pub fn is_at_least_18(iso: Option<&str>) -> bool {
    age_years_from_birthdate_iso(iso).map(|a| a >= 18).unwrap_or(false)
}

/// True if today's calendar month/day matches birth (ignores year).
/// Short line for system prompts (local-only display name).
pub fn line_display_name_for_prompt(name: Option<&str>) -> String {
    let Some(n) = name.map(str::trim) else {
        return String::new();
    };
    if n.is_empty() {
        return String::new();
    }
    format!("\n\nThe user prefers to be called: {}.", n)
}

pub fn is_birthday_today(iso: Option<&str>) -> bool {
    let Some(s) = iso.map(str::trim) else {
        return false;
    };
    let Ok(birth) = NaiveDate::parse_from_str(s, "%Y-%m-%d") else {
        return false;
    };
    let today = Utc::now().date_naive();
    birth.month() == today.month() && birth.day() == today.day()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_reasonable() {
        assert!(is_at_least_18(Some("2000-01-01")));
        assert!(!is_at_least_18(Some("2015-01-01")));
    }
}
