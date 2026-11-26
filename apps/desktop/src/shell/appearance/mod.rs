//! Settings › Appearance, as Desktop has it (`settings/appearance/appearance.tsx`): colour
//! scheme, theme (`opencode_ui::themes`), the UI, code and terminal fonts, interface zoom and Reduce
//! motion (System / On / Off). The font rows take any family name, as Desktop's do: one of the
//! faces the app bundles picks it, and any other installed family stands before the default,
//! which Rune loads from the system by name.

pub mod background;
pub mod knobs;
pub mod language;
pub mod text_rendering;

use std::cell::Cell;

use rune::Tx;

use crate::App;
use crate::popup::{self, Menu};

pub use opencode_feature_settings::{CODE_FONTS, ReduceMotion, UI_FONTS, families, font_index};

/// Where `App::reduced_motion` comes from: `--reduced-motion`, the setting and macOS's
/// Reduce motion, which only the view can read (`note_system`).
#[derive(Debug, Default)]
pub struct Motion {
    flag: bool,
    system: Cell<bool>,
    applied: bool,
}

impl Motion {
    pub fn new(flag: bool) -> Motion {
        Motion { flag, ..Motion::default() }
    }

    /// The view's reading of the system preference, for the next poll.
    pub fn note_system(&self, on: bool) {
        self.system.set(on);
    }
}

/// The weights the bundled mono faces answer, as Desktop's `@font-face` rules declare them
/// (`packages/app/src/index.css:42-57`): Plex Mono's Text cut every weight below 600, its Bold cut
/// the rest. Without them inline code, which Desktop sets at the medium weight 530
/// (`session-ui/src/components/markdown.css:431-441`), matched the Bold cut. Commit Mono, which
/// Desktop doesn't bundle, splits its Regular and Bold cuts the same way.
pub const FACE_WEIGHTS: &[rune::text::FaceWeights] = &[
    rune::text::FaceWeights { family: "IBM Plex Mono", weight: 450.0, range: (100.0, 599.0) },
    rune::text::FaceWeights { family: "IBM Plex Mono", weight: 700.0, range: (600.0, 900.0) },
    rune::text::FaceWeights { family: "CommitMono", weight: 400.0, range: (100.0, 599.0) },
    rune::text::FaceWeights { family: "CommitMono", weight: 700.0, range: (600.0, 900.0) },
];

/// The families a window opens with: the saved settings (a fixture run's are the defaults),
/// with `--ui-font` and `--code-font` applied as `apply_args` applies them.
pub fn launch_families() -> (&'static str, &'static str) {
    crate::init_features();
    let mut settings = if crate::arg("--fixture").is_some() { Default::default() } else { crate::settings::load() };
    apply_font_args(&mut settings);
    let (sans, mono) = families(&settings);
    // Once per launch: the window's spec keeps them for the app's life.
    (Box::leak(sans.into_boxed_str()), Box::leak(mono.into_boxed_str()))
}

/// `--ui-font <name>` and `--code-font <name>` pick a row's font by name ("SF Pro").
fn apply_font_args(settings: &mut crate::settings::Settings) {
    if let Some(name) = crate::arg("--ui-font") {
        settings.ui_font = font_index(&UI_FONTS, Some(&name));
    }
    if let Some(name) = crate::arg("--code-font") {
        settings.code_font = font_index(&CODE_FONTS, Some(&name));
    }
}

/// `--color-theme <id>` paints a run in one of Desktop's themes (`themes::all`); `--ui-font` and
/// `--code-font` pick its fonts; `--text-renderer <name>` sets Text rendering
/// (`text_rendering::TextRenderer::name`).
pub fn apply_args(app: &mut App) {
    let args = rune::extra_args();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1));
    if let Some(id) = value("--color-theme") {
        app.settings.theme = id.clone();
    }
    if let Some(name) = value("--text-renderer") {
        app.settings.text_renderer = crate::text_rendering::TextRenderer::parse(Some(name));
    }
    apply_font_args(&mut app.settings);
}

/// Picks up a change in macOS's Reduce motion.
pub fn poll(s: &mut App) -> bool {
    let system = s.appearance_state.motion.system.get();
    if system == s.appearance_state.motion.applied {
        return false;
    }
    s.appearance_state.motion.applied = system;
    s.apply_reduce_motion()
}

impl App {
    /// Applies saved appearance settings that live outside `Settings`: the theme, the language,
    /// Reduce motion and the terminal font knob.
    pub fn apply_appearance(&mut self) {
        crate::language::apply(self);
        crate::themes::set_active(&self.settings.theme);
        crate::keymap::overrides::apply(&self.settings.keybinds);
        self.apply_reduce_motion();
        self.diag.knobs.set(&crate::terminal::TERM_FONT, self.settings.terminal_font as f32);
        crate::terminal::set_custom_font(&self.settings.terminal_font_name);
        rune::spell::set_languages(&self.settings.spellcheck_languages);
    }

    /// The terminal's font: choice `i`, or `name`, an installed family, before the default.
    pub fn set_terminal_font(&mut self, i: usize, name: &str, tx: &mut Tx) {
        tx.instant();
        let name = name.to_owned();
        crate::terminal::set_custom_font(&name);
        self.change_settings(|s| {
            s.terminal_font = i;
            s.terminal_font_name = name;
        });
        self.diag.knobs.set(&crate::terminal::TERM_FONT, i as f32);
    }

    /// Switches the UI font in this frame: every window lays its text out again in it.
    pub fn set_ui_font(&mut self, i: usize, tx: &mut Tx) {
        self.set_ui_font_named(i, "", tx);
    }

    /// The UI font: choice `i`, or `name`, an installed family, before the default.
    pub fn set_ui_font_named(&mut self, i: usize, name: &str, tx: &mut Tx) {
        tx.instant();
        let name = name.to_owned();
        self.change_settings(|s| {
            s.ui_font = i.min(UI_FONTS.len() - 1);
            s.ui_font_name = name;
        });
        let (sans, mono) = families(&self.settings);
        tx.set_font_families(sans, mono);
    }

    /// Switches the code font in this frame.
    pub fn set_code_font(&mut self, i: usize, tx: &mut Tx) {
        self.set_code_font_named(i, "", tx);
    }

    /// The code font: choice `i`, or `name`, an installed family, before the default.
    pub fn set_code_font_named(&mut self, i: usize, name: &str, tx: &mut Tx) {
        tx.instant();
        let name = name.to_owned();
        self.change_settings(|s| {
            s.code_font = i.min(CODE_FONTS.len() - 1);
            s.code_font_name = name;
        });
        let (sans, mono) = families(&self.settings);
        tx.set_font_families(sans, mono);
    }

    /// Sets `reduced_motion` from the flag, the setting and the system; true when it changed.
    pub fn apply_reduce_motion(&mut self) -> bool {
        let on = self.appearance_state.motion.flag
            || match self.settings.reduce_motion {
                ReduceMotion::System => self.appearance_state.motion.applied,
                ReduceMotion::On => true,
                ReduceMotion::Off => false,
            };
        let changed = on != *self.appearance_state.reduced_motion;
        *self.appearance_state.reduced_motion = on;
        changed
    }

    pub fn set_reduce_motion(&mut self, m: ReduceMotion) {
        self.change_settings(|s| s.reduce_motion = m);
        self.apply_reduce_motion();
    }

    /// Every theme in a menu from the Theme row's button, the current one checked.
    pub fn open_theme_menu(&mut self, tx: &mut Tx) {
        let themes = crate::themes::all();
        let menu = Menu::picker(opencode_i18n::t!("settings.general.row.theme.title"), themes.iter().map(|t| (t.id.clone(), t.name.clone())), &themes[crate::themes::index(&self.settings.theme)].id);
        popup::open(self, menu, "theme-select", tx, |s: &mut App, picked, tx| {
            if let Some(id) = picked {
                s.set_theme(id, tx);
            }
        });
    }

    /// Swaps the theme in this frame, as Desktop does: no crossfade.
    pub fn set_theme(&mut self, id: &str, tx: &mut Tx) {
        tx.instant();
        let id = crate::themes::all()[crate::themes::index(id)].id.clone();
        crate::themes::set_active(&id);
        self.change_settings(|s| s.theme = id);
    }
}
