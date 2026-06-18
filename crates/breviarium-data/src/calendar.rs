//! Pure liturgical calendar math: Gregorian Easter, temporal week keys,
//! sanctoral keys, and the per-date [`DateFacts`] the resolver builds on.
//!
//! This module is deliberately free of any catalog/data dependency — it only
//! turns a Gregorian date into liturgical coordinates. Source-candidate
//! selection (which needs the loaded catalog) lives in `catalog.rs`. The
//! functions here are lifted verbatim from the original monolithic resolver;
//! they are correct and irreducible, so they are moved rather than rewritten.

use crate::{DataError, DateFacts};
use chrono::{Datelike, NaiveDate, Weekday};

/// Computes Office date facts for a Gregorian date.
pub fn office_date_facts(date: NaiveDate) -> Result<DateFacts, DataError> {
    let gregorian_start = NaiveDate::from_ymd_opt(1582, 10, 15).expect("valid date");
    if date < gregorian_start {
        return Err(DataError::UnsupportedScope {
            message: "Office dates before October 15, 1582 are outside the Gregorian calendar"
                .to_string(),
        });
    }
    let temporal_week = temporal_week_key(date, false)?;
    let weekday = date.weekday();
    let day = liturgical_weekday_number(weekday);
    let temporal_stem = if temporal_week.starts_with("Nat") {
        temporal_week.clone()
    } else {
        format!("{temporal_week}-{day}")
    };
    Ok(DateFacts {
        date,
        weekday,
        easter: gregorian_easter(date.year()).ok_or_else(|| DataError::UnsupportedScope {
            message: format!("could not compute Easter for {}", date.year()),
        })?,
        temporal_week,
        temporal_stem,
        sanctoral_key: sanctoral_key(date),
    })
}

/// Gregorian Easter Sunday for `year` (Meeus/Butcher algorithm).
pub(crate) fn gregorian_easter(year: i32) -> Option<NaiveDate> {
    let golden_number = year % 19;
    let century = year / 100;
    let h = (century - century / 4 - (8 * century + 13) / 25 + 19 * golden_number + 15) % 30;
    let i = h - (h / 28) * (1 - (h / 28) * (29 / (h + 1)) * ((21 - golden_number) / 11));
    let j = (year + year / 4 + i + 2 - century + century / 4) % 7;
    let l = i - j;
    let month = 3 + (l + 40) / 44;
    let day = l + 28 - 31 * (month / 4);
    NaiveDate::from_ymd_opt(year, month as u32, day as u32)
}

/// Temporal week stem for a date (`Adv1`, `Nat25`, `Epi3`,
/// `Quadp1`, `Quad2`, `Pasc0`, `Pent07`, …). `mass` selects the Mass variant of
/// the post-Pentecost/Epiphany resumption naming.
pub(crate) fn temporal_week_key(date: NaiveDate, mass: bool) -> Result<String, DataError> {
    let year = date.year();
    let t = date.ordinal() as i32;
    let day = date.day() as i32;
    let month = date.month() as i32;
    let advent1 = advent1_ordinal(year)?;
    let christmas = ordinal_for(year, 12, 25)?;
    if t >= advent1 {
        if t < christmas {
            let n = 1 + (t - advent1) / 7;
            if month == 11 || day < 25 {
                return Ok(format!("Adv{n}"));
            }
        }
        return Ok(format!("Nat{day}"));
    }
    let ordtime = 6 + 7 - liturgical_weekday_number(weekday_for(year, 1, 6)?);
    if month == 1 && t < ordtime {
        return Ok(format!("Nat{day:02}"));
    }
    let easter = gregorian_easter(year).ok_or_else(|| DataError::UnsupportedScope {
        message: format!("could not compute Easter for {year}"),
    })?;
    let easter_ordinal = easter.ordinal() as i32;
    if t < easter_ordinal - 63 {
        return Ok(format!("Epi{}", (t - ordtime) / 7 + 1));
    }
    if t < easter_ordinal - 56 {
        return Ok("Quadp1".to_string());
    }
    if t < easter_ordinal - 49 {
        return Ok("Quadp2".to_string());
    }
    if t < easter_ordinal - 42 {
        return Ok("Quadp3".to_string());
    }
    if t < easter_ordinal {
        return Ok(format!("Quad{}", 1 + (t - (easter_ordinal - 42)) / 7));
    }
    if t < easter_ordinal + 56 {
        return Ok(format!("Pasc{}", (t - easter_ordinal) / 7));
    }
    let n = (t - (easter_ordinal + 49)) / 7;
    if n < 23 {
        return Ok(format!("Pent{n:02}"));
    }
    let wdist = (advent1 - t + 6) / 7;
    if wdist < 2 {
        return Ok("Pent24".to_string());
    }
    if n == 23 {
        return Ok("Pent23".to_string());
    }
    if mass {
        Ok(format!("PentEpi{}", 8 - wdist))
    } else {
        Ok(format!("Epi{}", 8 - wdist))
    }
}

fn advent1_ordinal(year: i32) -> Result<i32, DataError> {
    let christmas =
        NaiveDate::from_ymd_opt(year, 12, 25).ok_or_else(|| DataError::UnsupportedScope {
            message: format!("could not construct Christmas for {year}"),
        })?;
    let christmas_dow = match liturgical_weekday_number(christmas.weekday()) {
        0 => 7,
        day => day,
    };
    Ok(christmas.ordinal() as i32 - christmas_dow - 21)
}

fn ordinal_for(year: i32, month: u32, day: u32) -> Result<i32, DataError> {
    NaiveDate::from_ymd_opt(year, month, day)
        .map(|date| date.ordinal() as i32)
        .ok_or_else(|| DataError::UnsupportedScope {
            message: format!("could not construct {year:04}-{month:02}-{day:02}"),
        })
}

fn weekday_for(year: i32, month: u32, day: u32) -> Result<Weekday, DataError> {
    NaiveDate::from_ymd_opt(year, month, day)
        .map(|date| date.weekday())
        .ok_or_else(|| DataError::UnsupportedScope {
            message: format!("could not construct {year:04}-{month:02}-{day:02}"),
        })
}

/// Fixed-calendar `MM-DD` key, folding the Gregorian leap day so Feb 24–29 map
/// to the bissextile (leap-day) convention.
pub(crate) fn sanctoral_key(date: NaiveDate) -> String {
    let month = date.month();
    let mut day = date.day();
    if is_leap_year(date.year()) && month == 2 {
        if day == 24 {
            day = 29;
        } else if day > 24 {
            day -= 1;
        }
    }
    format!("{month:02}-{day:02}")
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0) && ((year % 100 != 0) || (year % 400 == 0))
}

/// Sunday = 0, Monday = 1, …, Saturday = 6 (liturgical weekday convention).
pub(crate) fn liturgical_weekday_number(weekday: Weekday) -> i32 {
    weekday.num_days_from_sunday() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn easter_known_dates() {
        assert_eq!(gregorian_easter(2026), Some(date(2026, 4, 5)));
        assert_eq!(gregorian_easter(2025), Some(date(2025, 4, 20)));
        assert_eq!(gregorian_easter(2000), Some(date(2000, 4, 23)));
    }

    #[test]
    fn facts_match_known_days() {
        // Cross-checked against reference tables for these dates.
        let jan1 = office_date_facts(date(2026, 1, 1)).unwrap();
        assert_eq!(jan1.temporal_week, "Nat01");
        assert_eq!(jan1.temporal_stem, "Nat01");
        assert_eq!(jan1.sanctoral_key, "01-01");

        let easter = office_date_facts(date(2026, 4, 5)).unwrap();
        assert_eq!(easter.temporal_week, "Pasc0");
        assert_eq!(easter.sanctoral_key, "04-05");
    }

    #[test]
    fn sanctoral_leap_fold() {
        assert_eq!(sanctoral_key(date(2024, 2, 24)), "02-29");
        assert_eq!(sanctoral_key(date(2024, 2, 25)), "02-24");
        assert_eq!(sanctoral_key(date(2025, 2, 24)), "02-24");
    }

    #[test]
    fn pre_gregorian_rejected() {
        assert!(office_date_facts(date(1500, 1, 1)).is_err());
    }
}
