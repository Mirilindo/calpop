//! Week conventions of the LC_TIME locale, read from glibc.
//! Must be called after setlocale() (GTK does it at start-up).

/// (first day of the week, 0 = Monday; minimal days of week 1).
/// Falls back to ISO 8601 (Monday, 4) outside glibc or on odd data.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn week_rules() -> (u32, u32) {
    use std::ffi::{c_char, c_int};

    // <langinfo.h> items the libc crate doesn't expose (glibc ABI values).
    const NL_TIME_WEEK_1STDAY: c_int = 0x20066;
    const NL_TIME_WEEK_1STWEEK: c_int = 0x20067;
    const NL_TIME_FIRST_WEEKDAY: c_int = 0x20068;

    unsafe extern "C" {
        fn nl_langinfo(item: c_int) -> *const c_char;
    }

    unsafe {
        let first = nl_langinfo(NL_TIME_FIRST_WEEKDAY);
        let min = nl_langinfo(NL_TIME_WEEK_1STWEEK);
        if first.is_null() || min.is_null() {
            return (0, 4);
        }
        let (first, min) = (*first as u8 as u32, *min as u8 as u32);
        // week-1stday is a date packed into the pointer value:
        // 19971130 = a Sunday, 19971201 = a Monday.
        let origin = nl_langinfo(NL_TIME_WEEK_1STDAY) as usize;
        if !(1..=7).contains(&first) || !(1..=7).contains(&min) {
            return (0, 4);
        }
        let origin = if origin == 19971201 { 0 } else { 6 };
        ((origin + first - 1) % 7, min)
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn week_rules() -> (u32, u32) {
    (0, 4)
}

/// Switches day and month names (LC_TIME) to `name`, after GTK has set the
/// locale from the environment. Returns false, changing nothing, when the
/// C library doesn't have that locale (not installed: see `locale -a`).
/// Done here rather than through LC_TIME/LC_ALL in the environment, which
/// would have to be set before GTK starts, so before the config is read.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn set_time(name: &str) -> bool {
    use std::ffi::{CString, c_char, c_int};

    const LC_TIME: c_int = 2; // glibc

    unsafe extern "C" {
        fn setlocale(category: c_int, locale: *const c_char) -> *const c_char;
    }

    let Ok(name) = CString::new(name) else { return false };
    // An unknown name leaves the current LC_TIME as it was.
    unsafe { !setlocale(LC_TIME, name.as_ptr()).is_null() }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn set_time(_name: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::set_time;

    /// One test only: the locale is global to the process.
    #[test]
    fn switches_month_names() {
        assert!(!set_time("xx_XX.UTF-8"));
        assert!(!set_time("bad\0name"));
        let november = || {
            gtk4::glib::DateTime::from_local(2026, 11, 1, 12, 0, 0.0).unwrap().format("%B").unwrap().to_string()
        };
        assert!(set_time("C"));
        assert_eq!(november(), "November");
        // Only where French is installed (it is on the author's machines).
        if set_time("fr_FR.UTF-8") {
            assert_eq!(november(), "novembre");
        }
        assert!(set_time("C"));
    }
}
