mod calendar;
mod config;
mod locale;
mod theme;

use calendar::{Date, Month};
use config::Config;
use gtk4::{gdk, glib, prelude::*};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::{cell::Cell, path::PathBuf, rc::Rc};

const APP_ID: &str = "io.github.mirilindo.Calpop";
const USAGE: &str = "\
Usage: calpop [--config PATH]

Opens a month calendar under the bar. Running it again closes it,
so it can be used directly as Waybar's \"on-click\" command.

Options:
  -c, --config PATH   config file (default: $XDG_CONFIG_HOME/calpop/config.toml)
      --default-config  print a config file listing every setting with its default
  -h, --help          show this help
  -V, --version       show the version

Keys: ←/→ or h/l month, ↑/↓ or k/j year, Home or t today, Esc or q close.
Mouse wheel: month. Click outside: close.";

/// Week layout, resolved once GTK has set the locale.
#[derive(Clone, Copy)]
struct Weeks {
    first_day: u32,  // first column, 0 = Monday
    rule_start: u32, // first day of a week for numbering
    min_days: u32,   // days of week 1 that must fall in the new year
}

fn main() -> glib::ExitCode {
    let mut config_path: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" | "--config" => match args.next() {
                Some(p) => config_path = Some(PathBuf::from(p)),
                None => return fail("--config needs a path"),
            },
            "--default-config" => {
                print!("{}", include_str!("../examples/config.toml"));
                return glib::ExitCode::SUCCESS;
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return glib::ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("calpop {}", env!("CARGO_PKG_VERSION"));
                return glib::ExitCode::SUCCESS;
            }
            other => return fail(&format!("unknown argument '{other}' (see --help)")),
        }
    }

    // A popup this small gains nothing from the GPU renderers: cairo uses
    // ~9 MB less memory (Vulkan 36 → cairo 27 MB, measured) for the same
    // start-up time. The user's own GSK_RENDERER wins.
    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: still single-threaded, GLib and GTK are not started yet.
        unsafe { std::env::set_var("GSK_RENDERER", "cairo") };
    }

    // Single instance: a second launch only sends "activate" to the first
    // one, which then closes. That makes the Waybar click a toggle. This is
    // checked before reading the config, so a broken config.toml can't stop
    // the popup from closing.
    let app = gtk4::Application::new(Some(APP_ID), Default::default());
    if let Err(e) = app.register(None::<&gtk4::gio::Cancellable>) {
        return fail(&format!("can't register with D-Bus: {e}"));
    }
    if app.is_remote() {
        return app.run_with_args(&["calpop"]);
    }

    let path = config_path.unwrap_or_else(|| Config::default_dir().join("config.toml"));
    let mut cfg = match Config::load(&path) {
        Ok(c) => c,
        Err(e) => return fail(&e),
    };
    if let Err(e) = cfg.check() {
        return fail(&e);
    }
    let config_dir = path.parent().map(PathBuf::from).unwrap_or_default();
    for w in cfg.apply_theme(&config_dir) {
        eprintln!("calpop: {w}");
    }
    // Optional raw CSS next to the config file, applied after the generated one.
    let user_css = path
        .parent()
        .map(|dir| dir.join("style.css"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();

    app.connect_activate(move |app| {
        if !app.windows().is_empty() {
            app.quit();
            return;
        }
        // GTK has called setlocale(LC_ALL, "") by now: only the names'
        // language is left to change, then glibc can answer.
        if !cfg.locale.is_empty() && !locale::set_time(&cfg.locale) {
            eprintln!(
                "calpop: locale \"{}\" is not installed (see locale -a); using the system's",
                cfg.locale
            );
        }
        let (loc_first, loc_min) = locale::week_rules();
        let first_day = cfg.first_day_index().ok().flatten().unwrap_or(loc_first);
        let (rule_start, min_days) = match cfg.week_numbering.as_str() {
            "us" => (first_day, 1),
            "locale" => (first_day, loc_min),
            _ => (0, 4), // ISO 8601
        };
        // Wayland first: asking layer-shell on X11 prints a GTK CRITICAL.
        let wayland = gdk::Display::default()
            .is_some_and(|d| d.type_().name() == "GdkWaylandDisplay");
        if !wayland || !gtk4_layer_shell::is_supported() {
            eprintln!(
                "calpop: this desktop does not support the wlr-layer-shell protocol \
                 (GNOME, X11…), so the popup can't be placed. See the README, Requirements."
            );
            std::process::exit(1);
        }
        load_css(&cfg, &user_css);
        build_window(app, &cfg, Weeks { first_day, rule_start, min_days });
    });
    app.run_with_args(&["calpop"])
}

fn fail(msg: &str) -> glib::ExitCode {
    eprintln!("calpop: {msg}");
    glib::ExitCode::FAILURE
}

fn load_css(cfg: &Config, user_css: &str) {
    let c = &cfg.colors;
    let size = cfg.font.size.max(6);
    // Day cells: a circle 2.1× the font size (the "today" mark).
    let cell = size * 21 / 10;
    let family = if cfg.font.family.is_empty() {
        String::new()
    } else {
        format!("font-family: \"{}\";", cfg.font.family.replace('"', ""))
    };
    let css = format!(
        r#"
window.calpop {{ background: transparent; }}
/* Fully transparent = GTK draws nothing and the surface never maps. */
window.calpop-backdrop {{ background-color: rgba(0, 0, 0, 0.01); }}
.calpop-root {{
    background-color: alpha({bg}, {op});
    color: {fg};
    border: 1px solid {border};
    border-radius: {radius}px;
    padding: 10px 14px 8px 14px;
    font-size: {size}px;
    {family}
}}
.calpop-root button {{
    background: none; border: none; box-shadow: none; outline: none;
    color: inherit; text-shadow: none;
    min-width: 0; min-height: 0; padding: 4px 10px;
    border-radius: {btn_radius}px;
    transition: background-color 120ms, color 120ms;
}}
.calpop-root button:hover {{ background-color: {hover}; }}
.calpop-root .nav {{ font-size: {nav}px; padding: 0 12px; color: {muted}; }}
.calpop-root .nav:hover {{ color: {accent}; }}
.calpop-root .title {{ font-weight: bold; font-size: {title}px; }}
.calpop-root .weekday {{ color: {muted}; font-size: {small}px; font-weight: bold; padding: 6px 0 2px 0; }}
.calpop-root .week {{ color: alpha({muted}, 0.7); font-size: {small}px; padding-right: 6px; }}
.calpop-root .day {{ min-width: {cell}px; min-height: {cell}px; border-radius: 999px; }}
.calpop-root .day.weekend {{ color: {weekend}; }}
.calpop-root .day.other {{ color: alpha({muted}, 0.45); }}
.calpop-root .day.today {{ background-color: {accent}; color: {on_accent}; font-weight: bold; }}
.calpop-root .footer {{ color: {muted}; font-size: {small}px; margin-top: 6px; }}
.calpop-root .footer:hover {{ color: {accent}; }}
"#,
        bg = c.background,
        op = cfg.style.opacity.clamp(0.0, 1.0),
        fg = c.foreground,
        muted = c.muted,
        accent = c.accent,
        on_accent = c.on_accent,
        border = c.border,
        hover = c.hover,
        weekend = if c.weekend.is_empty() { &c.muted } else { &c.weekend },
        radius = cfg.style.radius,
        btn_radius = cfg.style.radius.min(12),
        nav = size * 3 / 2,
        title = size + 2,
        small = (size * 4 / 5).max(6),
    );

    let display = gdk::Display::default().expect("no display");
    // Above the user's gtk.css (priority USER), so a global theme can't
    // restyle our buttons; the user's own style.css comes last.
    let prio = gtk4::STYLE_PROVIDER_PRIORITY_USER + 1;
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&css);
    gtk4::style_context_add_provider_for_display(&display, &provider, prio);
    if !user_css.is_empty() {
        let extra = gtk4::CssProvider::new();
        extra.load_from_string(user_css);
        gtk4::style_context_add_provider_for_display(&display, &extra, prio + 1);
    }
}

fn today() -> Date {
    let now = glib::DateTime::now_local().expect("no local time");
    Date::new(now.year(), now.month() as u32, now.day_of_month() as u32)
}

/// Formats a date with the locale (LC_TIME) through GLib's strftime.
fn fmt(d: Date, pattern: &str) -> String {
    glib::DateTime::from_local(d.year, d.month as i32, d.day as i32, 12, 0, 0.0)
        .ok()
        .and_then(|dt| dt.format(pattern).ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Invisible full-screen surface under the popup (bar included): a click
/// anywhere outside the popup closes it, like a menu. Clicking the clock
/// again lands here too, so it still toggles.
/// The screen named in `position.output`, if any. Unknown name: warning,
/// then the compositor's choice.
fn find_monitor(name: &str) -> Option<gdk::Monitor> {
    if name.is_empty() {
        return None;
    }
    let list = gdk::Display::default()?.monitors();
    let monitors: Vec<gdk::Monitor> = (0..list.n_items())
        .filter_map(|i| list.item(i)?.downcast::<gdk::Monitor>().ok())
        .collect();
    let found = monitors.iter().find(|m| m.connector().as_deref() == Some(name)).cloned();
    if found.is_none() {
        let known: Vec<String> =
            monitors.iter().filter_map(|m| m.connector().map(|c| c.to_string())).collect();
        eprintln!("calpop: output \"{name}\" not found (screens: {}); using the default", known.join(", "));
    }
    found
}

fn build_backdrop(app: &gtk4::Application, monitor: Option<&gdk::Monitor>) {
    let back = gtk4::ApplicationWindow::new(app);
    back.add_css_class("calpop-backdrop");
    back.init_layer_shell();
    back.set_monitor(monitor);
    back.set_namespace(Some("calpop-backdrop"));
    back.set_layer(Layer::Top);
    back.set_keyboard_mode(KeyboardMode::None);
    for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        back.set_anchor(e, true);
    }
    back.set_exclusive_zone(-1); // cover the bar as well
    let click = gtk4::GestureClick::new();
    click.set_button(0); // any button
    let app = app.clone();
    click.connect_pressed(move |_, _, _, _| app.quit());
    back.set_child(Some(&gtk4::Box::new(gtk4::Orientation::Vertical, 0)));
    back.add_controller(click);
    back.present();
}

fn build_window(app: &gtk4::Application, cfg: &Config, w: Weeks) {
    let first_day = w.first_day;
    let win = gtk4::ApplicationWindow::new(app);
    win.add_css_class("calpop");
    win.set_resizable(false);
    win.init_layer_shell();
    let monitor = find_monitor(&cfg.position.output);
    win.set_monitor(monitor.as_ref());
    win.set_namespace(Some("calpop"));
    // Overlay, one level above the backdrop (Top): the popup stays on top
    // whatever order the two surfaces map in, so it can be shown first.
    win.set_layer(Layer::Overlay);
    // Exclusive: Esc and the arrow keys always reach the popup, whatever the
    // focus policy of the compositor (OnDemand lost them on Hyprland).
    win.set_keyboard_mode(KeyboardMode::Exclusive);

    let p = &cfg.position;
    let edge = if p.edge == "bottom" { Edge::Bottom } else { Edge::Top };
    win.set_anchor(edge, true);
    win.set_margin(edge, p.margin_y);
    match p.align.as_str() {
        "left" => {
            win.set_anchor(Edge::Left, true);
            win.set_margin(Edge::Left, p.margin_x);
        }
        "right" => {
            win.set_anchor(Edge::Right, true);
            win.set_margin(Edge::Right, p.margin_x);
        }
        _ => {}
    }

    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.add_css_class("calpop-root");

    // ‹  Novembre 2026  ›
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let prev = gtk4::Button::with_label("‹");
    let next = gtk4::Button::with_label("›");
    let title = gtk4::Button::with_label("");
    prev.add_css_class("nav");
    next.add_css_class("nav");
    title.add_css_class("title");
    title.set_hexpand(true);
    for b in [&prev, &title, &next] {
        b.set_focus_on_click(false);
        b.set_can_focus(false);
    }
    header.append(&prev);
    header.append(&title);
    header.append(&next);
    root.append(&header);

    let grid = gtk4::Grid::new();
    grid.set_column_homogeneous(false);
    // Not homogeneous: the weekday-name row keeps its natural (smaller) height.
    grid.set_row_homogeneous(false);
    grid.set_column_spacing(4);
    grid.set_row_spacing(cfg.style.row_gap);
    let col0 = i32::from(cfg.week_numbers);

    // Weekday names, from the locale: "Lun", "Mar"… / "Mon", "Tue"…
    let monday = Date::new(2024, 1, 1);
    for i in 0..7 {
        let d = monday.add_days(i64::from((i + first_day) % 7));
        let name = capitalize(fmt(d, "%a").trim_end_matches('.'));
        let lbl = gtk4::Label::new(Some(&name));
        lbl.add_css_class("weekday");
        grid.attach(&lbl, col0 + i as i32, 0, 1, 1);
    }

    let mut weeks = Vec::new();
    if cfg.week_numbers {
        for r in 0..6 {
            let lbl = gtk4::Label::new(None);
            lbl.add_css_class("week");
            lbl.set_xalign(1.0);
            grid.attach(&lbl, 0, r + 1, 1, 1);
            weeks.push(lbl);
        }
    }
    let mut days = Vec::new();
    for r in 0..6 {
        for c in 0..7 {
            let lbl = gtk4::Label::new(None);
            lbl.add_css_class("day");
            grid.attach(&lbl, col0 + c, r + 1, 1, 1);
            days.push(lbl);
        }
    }
    root.append(&grid);

    let footer = gtk4::Button::with_label("");
    footer.add_css_class("footer");
    footer.set_can_focus(false);
    if cfg.footer {
        root.append(&footer);
    }
    win.set_child(Some(&root));

    let shown = Rc::new(Cell::new(Month::of(today())));
    let render = {
        let (shown, title, footer) = (shown.clone(), title.clone(), footer.clone());
        Rc::new(move || {
            let m = shown.get();
            let now = today();
            footer.set_label(&capitalize(&fmt(now, "%A %-d %B %Y")));
            title.set_label(&capitalize(&fmt(Date::new(m.year, m.month, 1), "%OB %Y")));
            let start = m.grid_start(first_day);
            for (i, lbl) in days.iter().enumerate() {
                let d = start.add_days(i as i64);
                lbl.set_label(&d.day.to_string());
                lbl.set_css_classes(&["day"]);
                if d.month != m.month {
                    lbl.add_css_class("other");
                } else if d.weekday() >= 5 {
                    lbl.add_css_class("weekend");
                }
                if d == now {
                    lbl.add_css_class("today");
                }
            }
            for (r, lbl) in weeks.iter().enumerate() {
                // Number the row by its day that starts a week under the
                // rule (for ISO with a Sunday first column: the Monday).
                let row = start.add_days(7 * r as i64);
                let d = row.add_days(i64::from((w.rule_start + 7 - first_day) % 7));
                lbl.set_label(&d.week_number(w.rule_start, w.min_days).to_string());
            }
        })
    };
    render();

    // Left open past midnight: move the "today" mark and the footer.
    {
        let (render, last) = (render.clone(), Cell::new(today()));
        glib::timeout_add_seconds_local(30, move || {
            if today() != last.get() {
                last.set(today());
                render();
            }
            glib::ControlFlow::Continue
        });
    }

    let go = {
        let (shown, render) = (shown.clone(), render.clone());
        Rc::new(move |delta: Option<i32>| {
            shown.set(match delta {
                Some(n) => shown.get().shift(n),
                None => Month::of(today()),
            });
            render();
        })
    };

    {
        let go = go.clone();
        prev.connect_clicked(move |_| go(Some(-1)));
    }
    {
        let go = go.clone();
        next.connect_clicked(move |_| go(Some(1)));
    }
    {
        let go = go.clone();
        title.connect_clicked(move |_| go(None));
    }
    {
        let go = go.clone();
        footer.connect_clicked(move |_| go(None));
    }

    // One wheel notch = one month (DISCRETE: GTK sums high-resolution steps).
    let scroll = gtk4::EventControllerScroll::new(
        gtk4::EventControllerScrollFlags::VERTICAL | gtk4::EventControllerScrollFlags::DISCRETE,
    );
    {
        let go = go.clone();
        let sign = if cfg.reverse_scroll { -1.0 } else { 1.0 };
        scroll.connect_scroll(move |_, _, dy| {
            let dy = dy * sign;
            if dy > 0.0 {
                go(Some(1));
            } else if dy < 0.0 {
                go(Some(-1));
            }
            glib::Propagation::Stop
        });
    }
    win.add_controller(scroll);

    let keys = gtk4::EventControllerKey::new();
    {
        let win = win.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            // Lower case: Caps Lock or Shift must not disable q, h/j/k/l, t.
            match key.to_lower() {
                gdk::Key::Escape | gdk::Key::q => win.application().map_or((), |a| a.quit()),
                gdk::Key::Left | gdk::Key::h | gdk::Key::Page_Up => go(Some(-1)),
                gdk::Key::Right | gdk::Key::l | gdk::Key::Page_Down => go(Some(1)),
                gdk::Key::Up | gdk::Key::k => go(Some(-12)),
                gdk::Key::Down | gdk::Key::j => go(Some(12)),
                gdk::Key::Home | gdk::Key::t => go(None),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
    }
    win.add_controller(keys);

    win.present();
    build_backdrop(app, monitor.as_ref());
}
