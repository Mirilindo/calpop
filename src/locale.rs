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
