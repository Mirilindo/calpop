//! Settings, read from `$XDG_CONFIG_HOME/calpop/config.toml`
//! (or `--config PATH`). Every key is optional.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// "monday" … "sunday" (any unambiguous prefix), or "locale".
    pub first_day: String,
    /// Week numbers in a left column.
    pub week_numbers: bool,
    /// "iso" (ISO 8601: Monday weeks, week 1 holds the first Thursday),
    /// "us" (week 1 holds 1 January, weeks start on `first_day`),
    /// or "locale" (the LC_TIME rule).
    pub week_numbering: String,
    /// Mouse wheel: false = down goes to the next month.
    pub reverse_scroll: bool,
    /// Full date of today under the grid (also a "back to today" button).
    pub footer: bool,
    /// Language of day and month names, e.g. "en_US.UTF-8".
    /// Empty = the system's LC_TIME. Must be installed (`locale -a`).
    pub locale: String,
    /// Optional theme file with `@define-color` or `--name:` lines; colors
    /// below can then be written "@name". `~` is expanded; a relative path
    /// starts from the config file's folder.
    pub theme: String,
    pub position: Position,
    pub font: Font,
    pub colors: Colors,
    pub style: Style,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Position {
    /// "top" or "bottom": the screen edge the bar sits on.
    pub edge: String,
    /// "left", "center" or "right".
    pub align: String,
    /// Gap from the left/right screen edge, in pixels (ignored for "center").
    pub margin_x: i32,
    /// Gap from the bar, in pixels.
    pub margin_y: i32,
    /// Screen to open on, by connector name ("DP-1", "eDP-1"…).
    /// Empty = the compositor's choice (usually the focused screen).
    pub output: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Font {
    /// Empty = the GTK default font.
    pub family: String,
    pub size: u32,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub background: String,
    pub foreground: String,
    /// Weekday names, week numbers, days of other months.
    pub muted: String,
    pub accent: String,
    pub on_accent: String,
    pub border: String,
    pub hover: String,
    /// Saturdays and Sundays of the month. Empty = same as `muted`.
    pub weekend: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Style {
    pub radius: u32,
    pub opacity: f64,
    /// Extra space between the rows of days, in pixels.
    pub row_gap: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            first_day: "monday".into(),
            week_numbers: false,
            week_numbering: "iso".into(),
            reverse_scroll: false,
            footer: true,
            locale: String::new(),
            theme: String::new(),
            position: Position::default(),
            font: Font::default(),
            colors: Colors::default(),
            style: Style::default(),
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self { edge: "top".into(), align: "center".into(), margin_x: 8, margin_y: 6, output: String::new() }
    }
}

impl Default for Font {
    fn default() -> Self {
        Self { family: String::new(), size: 14 }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            background: "#1e1f25".into(),
            foreground: "#e2e2e9".into(),
            muted: "#8e9099".into(),
            accent: "#adc6ff".into(),
            on_accent: "#102f60".into(),
            border: "#33353a".into(),
            hover: "#2b2d33".into(),
            weekend: String::new(),
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Self { radius: 16, opacity: 0.97, row_gap: 0 }
    }
}

impl Config {
    pub fn default_dir() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_default()
            .join("calpop")
    }

    /// Replaces "@name" colors with their value from the theme file.
    /// A relative `theme` path is taken from `config_dir`, the config file's
    /// folder (not the folder the bar was started from). Never fatal: on any
    /// problem the color falls back to its default and a warning is returned
    /// (a theme generator may be rewriting the file).
    pub fn apply_theme(&mut self, config_dir: &Path) -> Vec<String> {
        let mut warnings = Vec::new();
        let palette = if self.theme.is_empty() {
            crate::theme::Palette::new()
        } else {
            let path = config_dir.join(expand_tilde(&self.theme));
            match std::fs::read_to_string(&path) {
                Ok(text) => crate::theme::parse(&text),
                Err(e) => {
                    warnings.push(format!("theme {}: {e}", path.display()));
                    crate::theme::Palette::new()
                }
            }
        };
        let d = Colors::default();
        let c = &mut self.colors;
        for (key, value, default) in [
            ("background", &mut c.background, d.background),
            ("foreground", &mut c.foreground, d.foreground),
            ("muted", &mut c.muted, d.muted),
            ("accent", &mut c.accent, d.accent),
            ("on_accent", &mut c.on_accent, d.on_accent),
            ("border", &mut c.border, d.border),
            ("hover", &mut c.hover, d.hover),
            ("weekend", &mut c.weekend, d.weekend),
        ] {
            match crate::theme::resolve(value, &palette) {
                Ok(v) => *value = v,
                Err(name) => {
                    warnings.push(format!("colors.{key} = \"{value}\": @{name} not found, using {default}"));
                    *value = default;
                }
            }
        }
        warnings
    }

    /// A missing file gives the defaults; an invalid one is an error.
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// 0 = Monday … 6 = Sunday; None = take it from the locale.
    pub fn first_day_index(&self) -> Result<Option<u32>, String> {
        const DAYS: [&str; 7] =
            ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
        let v = self.first_day.trim().to_lowercase();
        if v == "locale" {
            return Ok(None);
        }
        let hits: Vec<usize> =
            (0..7).filter(|&i| !v.is_empty() && DAYS[i].starts_with(&v)).collect();
        match hits[..] {
            [i] => Ok(Some(i as u32)),
            _ => Err(format!(
                "first_day = \"{}\": expected monday … sunday, or locale",
                self.first_day
            )),
        }
    }

    pub fn check(&self) -> Result<(), String> {
        self.first_day_index()?;
        match self.week_numbering.as_str() {
            "iso" | "us" | "locale" => {}
            v => return Err(format!("week_numbering = \"{v}\": expected iso, us or locale")),
        }
        match self.position.edge.as_str() {
            "top" | "bottom" => {}
            v => return Err(format!("position.edge = \"{v}\": expected top or bottom")),
        }
        match self.position.align.as_str() {
            "left" | "center" | "right" => {}
            v => return Err(format!("position.align = \"{v}\": expected left, center or right")),
        }
        Ok(())
    }
}

pub fn expand_tilde(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_theme_is_next_to_the_config() {
        let dir = std::env::temp_dir().join(format!("calpop-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("colors.css"), "@define-color primary #123456;\n").unwrap();
        let mut cfg = Config { theme: "colors.css".into(), ..Config::default() };
        cfg.colors.accent = "@primary".into();
        let warnings = cfg.apply_theme(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(cfg.colors.accent, "#123456");
    }

    /// The documented example must list every key with its real default:
    /// adding an option or changing a default without documenting it fails here.
    #[test]
    fn example_is_the_defaults() {
        let text = include_str!("../examples/config.toml");
        let parsed: Config = toml::from_str(text).expect("example must parse");
        assert_eq!(parsed, Config::default());
        // Same key names, section by section, as the program knows them.
        fn keys(t: &toml::Table, prefix: &str, out: &mut Vec<String>) {
            for (k, v) in t {
                match v {
                    toml::Value::Table(sub) => keys(sub, &format!("{prefix}{k}."), out),
                    _ => out.push(format!("{prefix}{k}")),
                }
            }
        }
        let (mut doc, mut real) = (Vec::new(), Vec::new());
        keys(&toml::from_str(text).unwrap(), "", &mut doc);
        keys(&toml::Table::try_from(Config::default()).unwrap(), "", &mut real);
        doc.sort();
        real.sort();
        assert_eq!(doc, real, "examples/config.toml must list every setting");
    }
}
