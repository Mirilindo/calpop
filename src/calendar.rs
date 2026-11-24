//! Date arithmetic for the month grid. Pure Rust, no GTK, unit-tested.
//!
//! Days are counted from 1970-01-01 (day 0) with Howard Hinnant's
//! `days_from_civil` / `civil_from_days` algorithms (proleptic Gregorian).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u32, // 1..=12
    pub day: u32,   // 1..=31
}

impl Date {
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self { year, month, day }
    }

    pub fn to_days(self) -> i64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    pub fn from_days(z: i64) -> Self {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Self { year, month, day }
    }

    /// 0 = Monday … 6 = Sunday.
    pub fn weekday(self) -> u32 {
        // 1970-01-01 was a Thursday (3).
        ((self.to_days() + 3).rem_euclid(7)) as u32
    }

    pub fn add_days(self, n: i64) -> Self {
        Self::from_days(self.to_days() + n)
    }

    /// ISO 8601 week number (1..=53). Reference for the tests of `week_number`.
    #[cfg(test)]
    pub fn iso_week(self) -> u32 {
        // The ISO week belongs to the year of its Thursday.
        let thursday = self.add_days(3 - i64::from(self.weekday()));
        let jan1 = Date::new(thursday.year, 1, 1);
        ((thursday.to_days() - jan1.to_days()) / 7 + 1) as u32
    }

    /// First day (`week_start`, 0 = Monday) of the week holding this date.
    pub fn week_start(self, week_start: u32) -> Date {
        self.add_days(-i64::from((self.weekday() + 7 - week_start) % 7))
    }

    /// Week number under the CLDR/ICU model: weeks start on `week_start`,
    /// and week 1 is the first week with at least `min_days` days in the
    /// new year. ISO 8601 = (Monday, 4); US = (Sunday, 1).
    pub fn week_number(self, week_start: u32, min_days: u32) -> u32 {
        let week1 = |year: i32| {
            let jan1 = Date::new(year, 1, 1);
            let s = jan1.week_start(week_start);
            let days_in_year = 7 - (jan1.to_days() - s.to_days());
            if days_in_year >= i64::from(min_days) { s.to_days() } else { s.to_days() + 7 }
        };
        let s = self.week_start(week_start).to_days();
        let base = if s >= week1(self.year + 1) {
            week1(self.year + 1)
        } else if s < week1(self.year) {
            week1(self.year - 1)
        } else {
            week1(self.year)
        };
        ((s - base) / 7 + 1) as u32
    }
}

/// A month being displayed (navigation state).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Month {
    pub year: i32,
    pub month: u32,
}

impl Month {
    pub fn of(d: Date) -> Self {
        Self { year: d.year, month: d.month }
    }

    pub fn shift(self, months: i32) -> Self {
        let idx = self.year * 12 + self.month as i32 - 1 + months;
        Self { year: idx.div_euclid(12), month: (idx.rem_euclid(12) + 1) as u32 }
    }

    /// First cell of a 6-week grid: the `first_day` (0 = Monday) on or
    /// before the 1st of the month.
    pub fn grid_start(self, first_day: u32) -> Date {
        let first = Date::new(self.year, self.month, 1);
        let back = (first.weekday() + 7 - first_day) % 7;
        first.add_days(-i64::from(back))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for z in -800_000..800_000 {
            assert_eq!(Date::from_days(z).to_days(), z);
        }
    }

    #[test]
    fn weekdays() {
        assert_eq!(Date::new(1970, 1, 1).weekday(), 3); // jeudi
        assert_eq!(Date::new(2026, 11, 24).weekday(), 1); // mardi
        assert_eq!(Date::new(2000, 2, 29).weekday(), 1); // mardi
    }

    #[test]
    fn iso_weeks() {
        assert_eq!(Date::new(2026, 11, 24).iso_week(), 48);
        assert_eq!(Date::new(2021, 1, 3).iso_week(), 53); // dimanche → semaine 53 de 2020
        assert_eq!(Date::new(2024, 12, 30).iso_week(), 1); // lundi → semaine 1 de 2025
        assert_eq!(Date::new(2026, 1, 1).iso_week(), 1);
        assert_eq!(Date::new(2027, 1, 1).iso_week(), 53);
    }

    #[test]
    fn generic_iso_matches_iso() {
        for z in -200_000..200_000 {
            let d = Date::from_days(z);
            assert_eq!(d.week_number(0, 4), d.iso_week(), "{d:?}");
        }
    }

    #[test]
    fn us_weeks() {
        // Week 1 = the Sunday-week holding 1 January.
        assert_eq!(Date::new(2027, 1, 1).week_number(6, 1), 1); // vendredi
        assert_eq!(Date::new(2027, 1, 3).week_number(6, 1), 2); // dimanche
        assert_eq!(Date::new(2026, 12, 31).week_number(6, 1), 1); // même semaine que le 1/1/2027
        assert_eq!(Date::new(2026, 11, 24).week_number(6, 1), 48);
    }

    #[test]
    fn months() {
        let m = Month { year: 2026, month: 1 };
        assert_eq!(m.shift(-1), Month { year: 2025, month: 12 });
        assert_eq!(m.shift(12), Month { year: 2027, month: 1 });
        // Novembre 2026 commence un dimanche : la grille démarre lundi 26 octobre.
        let nov = Month { year: 2026, month: 11 };
        assert_eq!(nov.grid_start(0), Date::new(2026, 10, 26));
        assert_eq!(nov.grid_start(6), Date::new(2026, 11, 1));
    }
}

