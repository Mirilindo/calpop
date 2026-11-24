//! Colors taken from a theme file, so the popup follows the desktop theme
//! (matugen, pywal, wallust… regenerate that file with each wallpaper).
//!
//! Three syntaxes are read, line by line:
//!   @define-color primary #adc6ff;     (GTK / Waybar)
//!   --primary: #adc6ff;                (CSS custom properties, e.g. pywal)
//!   accent = "#1e66f5"                 (TOML key = "value", e.g. Omarchy)
//! A color in config.toml written "@primary" is replaced by its value.
//! Values may themselves use @names (`alpha(@primary, 0.5)`).

use std::collections::HashMap;

pub type Palette = HashMap<String, String>;

pub fn parse(text: &str) -> Palette {
    let mut p = Palette::new();
    for line in text.lines() {
        let line = line.trim();
        let (name, value) = if let Some(rest) = line.strip_prefix("@define-color") {
            let rest = rest.trim_start();
            match rest.split_once(char::is_whitespace) {
                Some((n, v)) => (n, v),
                None => continue,
            }
        } else if let Some(rest) = line.strip_prefix("--") {
            match rest.split_once(':') {
                Some((n, v)) => (n.trim(), v),
                None => continue,
            }
        } else if let Some((n, v)) = line.split_once('=') {
            // TOML: only `key = "string"` lines; tables and numbers are skipped.
            let (n, v) = (n.trim(), v.trim());
            match v.strip_prefix('"').and_then(|v| v.split_once('"')) {
                Some((v, _)) if !n.is_empty() && n.chars().all(is_name_char) => (n, v),
                _ => continue,
            }
        } else {
            continue;
        };
        let value = value.split(';').next().unwrap_or("").trim();
        if !name.is_empty() && !value.is_empty() {
            p.insert(name.to_string(), value.to_string());
        }
    }
    p
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Replaces every `@name` in `value`. Unknown name → Err(name).
pub fn resolve(value: &str, palette: &Palette) -> Result<String, String> {
    let mut out = value.to_string();
    for _ in 0..16 {
        let Some(at) = out.find('@') else { return Ok(out) };
        let end = out[at + 1..].find(|c| !is_name_char(c)).map_or(out.len(), |i| at + 1 + i);
        let name = &out[at + 1..end];
        let Some(v) = palette.get(name) else { return Err(name.to_string()) };
        out = format!("{}{}{}", &out[..at], v, &out[end..]);
    }
    Err("(definitions too deeply nested)".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\
/* matugen */
@define-color primary #adc6ff;
@define-color on_primary   #102f60 ;
@define-color soft alpha(@primary, 0.5);
:root {
  --color1: #ff0000;
}
";

    #[test]
    fn parses_both_syntaxes() {
        let p = parse(FILE);
        assert_eq!(p["primary"], "#adc6ff");
        assert_eq!(p["on_primary"], "#102f60");
        assert_eq!(p["color1"], "#ff0000");
    }

    #[test]
    fn parses_omarchy_toml() {
        let p = parse("mode = \"light\"\n\naccent = \"#1e66f5\" # bleu\nn = 3\n[table]\n");
        assert_eq!(p["accent"], "#1e66f5");
        assert_eq!(p["mode"], "light");
        assert!(!p.contains_key("n"));
    }

    #[test]
    fn resolves_names() {
        let p = parse(FILE);
        assert_eq!(resolve("#123456", &p).unwrap(), "#123456");
        assert_eq!(resolve("@primary", &p).unwrap(), "#adc6ff");
        assert_eq!(resolve("@soft", &p).unwrap(), "alpha(#adc6ff, 0.5)");
        assert_eq!(resolve("mix(@primary, @color1, 0.3)", &p).unwrap(), "mix(#adc6ff, #ff0000, 0.3)");
        assert_eq!(resolve("@nope", &p), Err("nope".into()));
    }

    #[test]
    fn loops_do_not_hang() {
        let p = parse("@define-color a @b;\n@define-color b @a;\n");
        assert!(resolve("@a", &p).is_err());
    }
}
