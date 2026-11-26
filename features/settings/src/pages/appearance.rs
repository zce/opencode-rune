//! Settings › Appearance (`settings/appearance/appearance.tsx`): Color scheme, Theme (with Learn
//! more), the UI, Code and Terminal fonts as 220 pt fields in their own faces, Zoom and Reduce
//! motion, and this app's Text rendering.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use opencode_i18n::t;
use opencode_ui::popup::PopupHost;
use rune::{El, Family, Link, LinkPaint, Span, TextEdit, TextStyle, Tx, text};
use opencode_ui::palette::Pal;

use super::{BODY, header, page, select, select_some, text_field};
use crate::host::SettingsHost;
use crate::types::{CODE_FONTS, ReduceMotion, Scheme, TERMINAL_FONTS, TextRenderer, UI_FONTS, ZOOM_LEVELS};
use crate::{Settings, list, setting, setting_with};

/// Desktop's defaults, shown as each field's placeholder (`sansDefault`, `monoDefault`, `terminalDefault`).
const UI_DEFAULT: &str = "Inter";
const CODE_DEFAULT: &str = "IBM Plex Mono";
const TERMINAL_DEFAULT: &str = "JetBrainsMono Nerd Font Mono";

fn terminal_preview_family(st: &Settings) -> Family {
    let custom = st.terminal_font_name.trim();
    if !custom.is_empty() {
        static INTERNED: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
        let list = format!("{}, {TERMINAL_DEFAULT}", crate::css_family(custom));
        let map = INTERNED.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(mut guard) = map.lock() {
            if let Some(&cached) = guard.get(&list) {
                return Family::Named(cached);
            }
            if guard.len() < 32 {
                let leaked: &'static str = Box::leak(list.clone().into_boxed_str());
                guard.insert(list, leaked);
                return Family::Named(leaked);
            }
        }
        return Family::Named(TERMINAL_DEFAULT);
    }
    match st.terminal_font {
        0 => Family::Named("CommitMono, JetBrainsMono Nerd Font Mono"),
        1 => Family::Named("IBM Plex Mono, JetBrainsMono Nerd Font Mono"),
        2 => Family::SystemMono,
        _ => Family::Named(TERMINAL_DEFAULT),
    }
}

/// The three font fields as typed.
#[derive(Default)]
pub struct Fields {
    pub ui: TextEdit,
    pub code: TextEdit,
    pub terminal: TextEdit,
}

impl Fields {
    /// The fields for the saved fonts: empty for a default, its name otherwise.
    pub fn from(s: &Settings) -> Fields {
        let named = |custom: &str, name: &str, default: bool| TextEdit::new(if !custom.is_empty() { custom } else if default { "" } else { name });
        Fields {
            ui: named(&s.ui_font_name, UI_FONTS[s.ui_font.min(UI_FONTS.len() - 1)].name, s.ui_font == 0),
            code: named(&s.code_font_name, CODE_FONTS[s.code_font.min(CODE_FONTS.len() - 1)].name, s.code_font == 0),
            terminal: named(&s.terminal_font_name, TERMINAL_FONTS[s.terminal_font.min(TERMINAL_FONTS.len() - 1)], s.terminal_font == 3),
        }
    }

    pub fn blur(&mut self) {
        self.ui.focused = false;
        self.code.focused = false;
        self.terminal.focused = false;
    }
}

/// The bundled face a typed name is: matched without case or spaces.
fn find(names: &[&str], typed: &str) -> Option<usize> {
    let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
    let typed = squash(typed);
    names.iter().position(|n| squash(n) == typed)
}

fn ui_changed(s: &mut dyn SettingsHost, tx: &mut Tx) {
    let typed = s.settings_pages().fonts.ui.text.trim().to_owned();
    let names: Vec<&str> = UI_FONTS.iter().map(|f| f.name).collect();
    match if typed.is_empty() { Some(0) } else { find(&names, &typed) } {
        Some(i) => s.set_ui_font(i, "", tx),
        None => s.set_ui_font(0, &typed, tx),
    }
}

fn code_changed(s: &mut dyn SettingsHost, tx: &mut Tx) {
    let typed = s.settings_pages().fonts.code.text.trim().to_owned();
    let names: Vec<&str> = CODE_FONTS.iter().map(|f| f.name).collect();
    match if typed.is_empty() { Some(0) } else { find(&names, &typed) } {
        Some(i) => s.set_code_font(i, "", tx),
        None => s.set_code_font(0, &typed, tx),
    }
}

fn terminal_changed(s: &mut dyn SettingsHost, tx: &mut Tx) {
    let typed = s.settings_pages().fonts.terminal.text.trim().to_owned();
    let found = if typed.is_empty() {
        Some(3)
    } else {
        find(&TERMINAL_FONTS, &typed).or_else(|| find(&[TERMINAL_DEFAULT, "CommitMono"], &typed).map(|i| [3, 0][i]))
    };
    match found {
        Some(i) => s.set_terminal_font(i, "", tx),
        None => s.set_terminal_font(3, &typed, tx),
    }
}

fn pick_scheme(s: &mut dyn SettingsHost, id: &str, tx: &mut Tx) {
    crate::set_scheme(s, Scheme::parse(id), tx);
}

fn pick_theme(s: &mut dyn SettingsHost, id: &str, tx: &mut Tx) {
    s.set_theme(id, tx);
}

fn pick_zoom(s: &mut dyn SettingsHost, id: &str, tx: &mut Tx) {
    tx.instant();
    s.set_zoom_level(id.parse::<f32>().unwrap_or(100.0) / 100.0);
}

fn pick_reduce_motion(s: &mut dyn SettingsHost, id: &str, _tx: &mut Tx) {
    s.set_reduce_motion(ReduceMotion::parse(Some(id)));
}

fn pick_text_renderer(s: &mut dyn SettingsHost, id: &str, _tx: &mut Tx) {
    s.set_text_renderer(TextRenderer::parse(Some(id)));
}

pub fn view<H: SettingsHost + PopupHost + 'static>(s: &H, p: &Pal) -> El {
    let st = s.settings();
    let f = &s.settings_pages().fonts;
    let themes = opencode_ui::themes::all();
    let desc = t!("settings.general.row.theme.description");
    let more = t!("common.learnMore");
    let theme_text = format!("{desc} {more}");
    let more_start = desc.len() + 1;
    let theme_about = text(theme_text)
        .key("settings-theme-docs")
        .font(BODY)
        .color(p.muted)
        .wrap()
        .spans(vec![Span::new(more_start, more_start + more.len()).color(p.accent)])
        .link(Link::new(more_start..more_start + more.len(), "https://opencode.ai/docs/themes").role("link").label(more).hover(LinkPaint::new().color(p.accent_hover)))
        .on_link(|s: &mut H, url: &str, tx: &mut Tx| crate::open_external(s, url, tx));
    let current_zoom = s.zoom();
    let zoom_pct = st.zoom_percent(current_zoom);
    let mut levels: Vec<(String, String)> = ZOOM_LEVELS
        .iter()
        .map(|l| (format!("{}", (l * 100.0).round() as i32), opencode_i18n::percent(f64::from(*l))))
        .collect();
    if !levels.iter().any(|(id, _)| *id == zoom_pct) {
        levels.push((zoom_pct.clone(), opencode_i18n::percent(f64::from(current_zoom))));
        levels.sort_by_key(|(id, _)| id.parse::<i32>().unwrap_or(0));
    }
    let current_theme = themes
        .get(opencode_ui::themes::index(&st.theme))
        .map_or(opencode_ui::themes::DEFAULT, |t| t.id.as_str());
    let face = |family: Family| {
        let mut t = TextStyle::new(13.0, 440.0).lh(16.0).tracking(-0.04);
        t.family = family;
        t
    };
    let rows = vec![
        (
            "settings-color-scheme",
            setting(
                p,
                "settings-color-scheme",
                t!("settings.general.row.colorScheme.title"),
                t!("settings.general.row.colorScheme.description"),
                select(
                    s,
                    p,
                    "settings-color-scheme-select",
                    t!("settings.general.row.colorScheme.title"),
                    Scheme::ALL.iter().map(|c| (c.name().to_owned(), c.label().to_owned())).collect(),
                    st.scheme.name(),
                    pick_scheme,
                ),
            ),
        ),
        (
            "settings-theme",
            setting_with(
                p,
                "settings-theme",
                t!("settings.general.row.theme.title"),
                theme_about,
                select(
                    s,
                    p,
                    "settings-theme-select",
                    t!("settings.general.row.theme.title"),
                    themes.iter().map(|t| (t.id.clone(), t.name.clone())).collect(),
                    current_theme,
                    pick_theme,
                ),
                true,
            ),
        ),
        (
            "settings-ui-font",
            setting(
                p,
                "settings-ui-font",
                t!("settings.general.row.uiFont.title"),
                t!("settings.general.row.uiFont.description"),
                text_field::<H>(p, "settings-ui-font-input", t!("settings.general.row.uiFont.title"), UI_DEFAULT, &f.ui, face(Family::Sans), |st| &mut st.fonts.ui, ui_changed),
            ),
        ),
        (
            "settings-code-font",
            setting(
                p,
                "settings-code-font",
                t!("settings.general.row.font.title"),
                t!("settings.general.row.font.description"),
                text_field::<H>(p, "settings-code-font-input", t!("settings.general.row.font.title"), CODE_DEFAULT, &f.code, face(Family::Mono), |st| &mut st.fonts.code, code_changed),
            ),
        ),
        (
            "settings-terminal-font",
            setting(
                p,
                "settings-terminal-font",
                t!("settings.general.row.terminalFont.title"),
                t!("settings.general.row.terminalFont.description"),
                text_field::<H>(
                    p,
                    "settings-terminal-font-input",
                    t!("settings.general.row.terminalFont.title"),
                    TERMINAL_DEFAULT,
                    &f.terminal,
                    face(terminal_preview_family(st)),
                    |st| &mut st.fonts.terminal,
                    terminal_changed,
                ),
            ),
        ),
        (
            "settings-zoom",
            setting(
                p,
                "settings-zoom",
                t!("settings.appearance.row.zoom.title"),
                t!("settings.appearance.row.zoom.description"),
                select(s, p, "settings-zoom-select", t!("settings.appearance.row.zoom.title"), levels, &zoom_pct, pick_zoom),
            ),
        ),
        (
            "settings-reduce-motion",
            setting(
                p,
                "settings-reduce-motion",
                t!("settings.appearance.row.reduceMotion.title"),
                t!("settings.appearance.row.reduceMotion.description"),
                select(
                    s,
                    p,
                    "settings-reduce-motion-select",
                    t!("settings.appearance.row.reduceMotion.title"),
                    ReduceMotion::ALL.iter().map(|m| (m.name().to_owned(), m.label().to_owned())).collect(),
                    st.reduce_motion.name(),
                    pick_reduce_motion,
                ),
            ),
        ),
        (
            "settings-text-renderer",
            setting(
                p,
                "settings-text-renderer",
                t!("rune.textRendering.title"),
                &text_rendering_note(s),
                select_some(
                    s,
                    p,
                    "settings-text-renderer-select",
                    t!("rune.textRendering.title"),
                    TextRenderer::ALL
                        .iter()
                        .map(|r| match r.support() {
                            rune::SmoothingSupport::No(reason) => (r.name().to_owned(), format!("{} \u{2014} {reason}", r.label()), false),
                            _ => (r.name().to_owned(), r.label().to_owned(), true),
                        })
                        .collect(),
                    st.text_renderer.name(),
                    pick_text_renderer,
                ),
            ),
        ),
    ];
    let card = list(p, "appearance-list", super::rows(s, p, 20.0, rows));
    page(p, "settings-appearance-page", header(p, t!("settings.general.section.appearance"), t!("settings.appearance.description")), card)
}

/// Text rendering's description, and what the choice falls back to here when it does.
fn text_rendering_note(s: &dyn SettingsHost) -> String {
    match s.text_rendering_fallback_reason() {
        Some(reason) if !reason.is_empty() => format!("{} {reason}.", t!("rune.textRendering.description")),
        _ => t!("rune.textRendering.description").to_owned(),
    }
}
