//! Settings (⌘, or the palette's Open settings): Desktop's routed two-pane screen
//! (`settings/shell.tsx`, `navigation.tsx`, `settings.css`): a 328 pt rail of grouped pages
//! beside a 720 pt column with the open page, the whole screen scrolling under each page's
//! sticky header. Preferences persist through host-configured storage in Desktop's `settings.v3`
//! shape where they share a name.
//!
//! All state transitions, search indexing, key handlers, job execution, and poll loops operate on
//! non-generic `&dyn SettingsHost` / `&mut dyn SettingsHost`. Generic bounds appear strictly on
//! thin `rune::El` view callbacks delegating in one line to non-generic functions.

#![forbid(unsafe_code)]

pub mod host;
mod interface_mode;
mod mode_card;
mod navigation_mode;
pub mod jobs;
pub mod pages;
pub mod prefs;
pub mod search;
pub mod shell;
pub mod stored;
pub mod types;

use std::collections::BTreeMap;

use opencode_ui::popup::PopupHost;
use rune::{Color, ColorScheme, Direction, El, Key, KeyEvent, Shadow, TextStyle, Tx, col, row, rule, text};
use rune_kit::Switch;
use serde_json::{Value, json};
use opencode_ui::palette::{Pal, kit};

pub use host::{
    AboutInfo, CommandItem, HttpServer, PluginPageMeta, PluginSearchItem, PluginServerInfo, ProviderRow, ServerEntry, ServerKind, SettingsConfig, SettingsHost, build_channel,
    build_version, configure,
};
pub use jobs::{SettingsJob, SettingsReply, SettingsRequest, apply_reply, execute_request};
pub use prefs::{BackgroundActivity, Choice, FollowUp, Navigation, Organize, QuestionSubmitKey, SOUNDS, Sounds, StartIn, TabGrouping, TimelineWidth, TitlebarPosition, Worktree};
pub use types::{
    CODE_FONTS, DockPlacement, Font, NotifyKind, NotifyKinds, ReduceMotion, Scheme, TERMINAL_FONTS, TabLayout, TerminalMarkShell, TextRenderer, UI_FONTS, ZOOM_LEVELS, ZOOM_MAX, ZOOM_MIN,
    css_family, custom_name, font_index, step_zoom,
};

/// Overlay stack identifier for the Settings screen (`opencode_kernel::OverlayStack`).
pub const OVERLAY_ID: &str = "dialog.settings";
/// Overlay stack identifier for Settings › Providers' disconnect / remove confirmation.
pub const PROVIDER_REMOVAL_OVERLAY_ID: &str = "dialog.settings.provider_removal";
/// Overlay stack identifier for Settings › Worktrees' delete confirmation.
pub const WORKTREE_DELETE_OVERLAY_ID: &str = "dialog.settings.worktree_delete";
/// Overlay stack identifier for Settings › About's third-party notices dialog.
pub const ABOUT_NOTICES_OVERLAY_ID: &str = "dialog.settings.about_notices";

/// Whether the Settings screen is open in this window (separate from persisted [`Settings`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsScreen {
    #[default]
    Closed,
    Open,
}

impl SettingsScreen {
    pub fn is_open(self) -> bool {
        matches!(self, Self::Open)
    }

    pub fn set_open(&mut self, open: bool) {
        *self = if open { Self::Open } else { Self::Closed };
    }
}

pub use prefs::question_composer_available;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// The composer's agent picker (`general.showCustomAgents`); the agent still applies when it's hidden.
    pub show_agent: bool,
    pub scheme: Scheme,
    /// Fold prompts past 12 lines behind Read more (`fold.rs`); Experimental, off unless turned on.
    pub fold_user: bool,
    /// The pane a new terminal opens in: `"side"` (default) or `"bottom"`.
    pub terminal_placement: DockPlacement,
    /// Which events post a system notification, with Desktop's defaults.
    pub notify: NotifyKinds,
    /// Answer every permission request "once" as it arrives (Desktop's `permissions.autoApprove`).
    pub auto_accept: bool,
    /// Projects whose own Auto-accept permissions overrides that default, by project id.
    pub project_auto_accept: BTreeMap<String, bool>,
    /// Desktop's colour theme id (`opencode_ui::themes`); `oc-2` is OpenCode's own.
    pub theme: String,
    pub reduce_motion: ReduceMotion,
    /// Appearance › Text rendering: how Rune rasterises text ([`TextRenderer`]).
    pub text_renderer: TextRenderer,
    /// The terminal's font, an index into [`TERMINAL_FONTS`].
    pub terminal_font: usize,
    /// Indexes into [`UI_FONTS`] and [`CODE_FONTS`], saved by name.
    pub ui_font: usize,
    pub code_font: usize,
    /// An installed family typed into a font field that none of its choices is (Desktop's font
    /// rows take any name), which then stands before the default; empty for a choice.
    pub ui_font_name: String,
    pub code_font_name: String,
    pub terminal_font_name: String,
    /// Rune records the last minute of frames for ⌘⇧⌥R (`glitch`); on by default.
    pub record_glitches: bool,
    /// Desktop's Interface layout (`appearance.tabLayout`): top tabs unless changed. The shell
    /// renders [`Settings::effective_tab_layout`].
    pub tab_layout: TabLayout,
    /// The navigation prototype's Tabs or List (`appearance.navigation`, #64668).
    pub navigation: Navigation,
    /// The navigation prototype's Group tabs (`appearance.tabGrouping`, anomalyco/opencode#66350).
    /// The shell reads [`Settings::tab_grouping`].
    pub tab_grouping: TabGrouping,
    /// One side panel open state for every session tab instead of one per tab (`general.sharedSidePanel`, #64950).
    pub shared_side_panel: bool,
    /// Questions answer from the composer instead of Classic (Desktop's `general.questionComposer`,
    /// Settings › Experimental › New question UI, `opencode#66825`). Only local and dev builds offer
    /// it, on by default there.
    pub question_composer: Option<bool>,
    /// Experimental side tab groups (`general.tabGroups`, #64407): opt-in (`false` by default).
    pub tab_groups: Option<bool>,
    /// Experimental › Sidebar sections (`rune.settings`): opt-in.
    pub sidebar_sections: Option<bool>,
    /// Ease a live delivery into place: the Pending label, the tray's room and joined prompts.
    pub smooth_timeline: bool,
    /// macOS: Option in the terminal is Meta (Esc and the key, for a shell's word motions) instead
    /// of typing the layout's character. Off by default.
    pub terminal_option_meta: bool,
    /// The shells the terminal starts with prompt marks ([`TerminalMarkShell::id`]),
    /// each turned on by the user; none by default.
    pub terminal_marks: Vec<String>,
    /// The display language Settings picked (Desktop's `language` store), an `opencode_i18n`
    /// locale id; `None` follows the system's languages.
    pub language: Option<String>,
    /// Where a new session starts (`workspaces.startIn`).
    pub start_in: StartIn,
    /// Which checkout of the current project a new session uses (`workspaces.worktree`).
    pub worktree: Worktree,
    /// Where each project's last session started (`workspaces.lastUsed`): `"default"` (its main
    /// checkout or an existing worktree) or `"workspace"` (a new worktree).
    pub last_used: BTreeMap<String, String>,
    /// What Enter does while the agent works (`general.followUpBehavior`).
    pub follow_up: FollowUp,
    /// The key that moves a question dock on or submits it (`general.questionSubmitKey`).
    pub question_submit: QuestionSubmitKey,
    /// Ctrl-scroll and trackpad pinches zoom the window (Desktop's `pinchZoom`); off by default.
    pub pinch_zoom: bool,
    /// Underline misspelt words while typing (Desktop's `spellcheck`); on by default.
    pub spellcheck: bool,
    /// Keep background service running after quit (`keep-running.ts`, opencode#66499); default true.
    pub keep_service_running: bool,
    /// First-quit background-service notice answered/skipped (`background-notice-machine.ts`).
    pub background_notice_shown: bool,
    /// The dictionaries a text field's Spellcheck Languages menu chose (Desktop's
    /// `spellcheckLanguages`); none follows the system languages.
    pub spellcheck_languages: Vec<String>,
    /// A narrow window's diffs wrap long lines rather than scroll (the review plugin's `mobileDiff.wrap`).
    pub mobile_diff_wrap: bool,
    /// Where a narrow window keeps its titlebar (`general.mobileTitlebarPosition`).
    pub mobile_titlebar: TitlebarPosition,
    /// The ChatGPT plan welcome has shown (Desktop's `chatgpt-plan-welcome.v1`).
    pub chatgpt_welcome_seen: bool,
    /// Experimental › Organize vertical sidebar (`general.sidebarOrganize`).
    pub sidebar_organize: Organize,
    /// Notifications › Sound effects (`sounds`).
    pub sounds: Sounds,
    /// Shortcuts the user changed (`keybinds`): command id to keybind, "none" for unbound.
    pub keybinds: BTreeMap<String, String>,
    /// Keep screen active as the app kept it before the Pairing plugin: read once to move it into
    /// the plugin's store.
    pub keep_screen_active: bool,
    /// Preferences › Timeline: what of a turn's work shows (`general.timelineDetail`), the
    /// column (`timelineWidth`), where background work shows (`backgroundActivity`) and
    /// whether a finished turn lists its changed files (`showTurnDiffs`).
    pub timeline_detail: opencode_runtime::detail::Detail,
    pub timeline_width: TimelineWidth,
    pub background_activity: BackgroundActivity,
    pub turn_diffs: bool,
    pub wrap_code_blocks: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            // Desktop shows the composer's agent picker by default (`settings/model.tsx:308`,
            // `showCustomAgents: true`).
            show_agent: true,
            scheme: Scheme::default(),
            fold_user: false,
            terminal_placement: DockPlacement::Side,
            notify: NotifyKinds::default(),
            auto_accept: false,
            project_auto_accept: BTreeMap::new(),
            theme: opencode_ui::themes::DEFAULT.into(),
            reduce_motion: ReduceMotion::default(),
            text_renderer: TextRenderer::default(),
            terminal_font: 3,
            ui_font: 0,
            code_font: 0,
            ui_font_name: String::new(),
            code_font_name: String::new(),
            terminal_font_name: String::new(),
            record_glitches: true,
            tab_layout: TabLayout::default(),
            navigation: Navigation::default(),
            tab_grouping: TabGrouping::default(),
            shared_side_panel: false,
            question_composer: None,
            tab_groups: None,
            sidebar_sections: None,
            smooth_timeline: false,
            terminal_option_meta: false,
            terminal_marks: Vec::new(),
            language: None,
            start_in: StartIn::default(),
            worktree: Worktree::default(),
            last_used: BTreeMap::new(),
            follow_up: FollowUp::default(),
            question_submit: QuestionSubmitKey::default(),
            pinch_zoom: false,
            spellcheck: true,
            keep_service_running: true,
            background_notice_shown: false,
            spellcheck_languages: Vec::new(),
            // The Review plugin's store starts wrapping (`gui-plugins/src/review/index.ts:46-48`).
            mobile_diff_wrap: true,
            mobile_titlebar: TitlebarPosition::default(),
            chatgpt_welcome_seen: false,
            sidebar_organize: Organize::default(),
            sounds: Sounds::default(),
            keybinds: BTreeMap::new(),
            keep_screen_active: false,
            timeline_detail: opencode_runtime::detail::Detail::default(),
            timeline_width: TimelineWidth::default(),
            background_activity: BackgroundActivity::default(),
            turn_diffs: false,
            wrap_code_blocks: false,
        }
    }
}

impl Settings {
    pub fn to_json(&self) -> Value {
        let mut v = self.to_json_base();
        if let Some(m) = v.as_object_mut() {
            m.insert("keepServiceRunning".into(), json!(self.keep_service_running));
            m.insert("backgroundNoticeShown".into(), json!(self.background_notice_shown));
            if let Some(on) = self.tab_groups { m.insert("tabGroups".into(), json!(on)); }
        if let Some(on) = self.question_composer { m.insert("questionComposer".into(), json!(on)); }
            if let Some(on) = self.sidebar_sections { m.insert("sidebarSections".into(), json!(on)); }
        }
        v
    }

    /// Experimental › Sidebar sections: off unless turned on.
    pub fn sidebar_sections(&self) -> bool {
        self.sidebar_sections.unwrap_or(false)
    }

    /// Experimental › Tab groups (`general.tabGroups`, #64407): opt-in (`false` by default) in
    /// every build while preserving any explicit `Some(true)` / `Some(false)` saved by the user.
    pub fn tab_groups(&self) -> bool {
        self.tab_groups.unwrap_or(false)
    }

    fn to_json_base(&self) -> Value {
        json!({
            "showAgentPicker": self.show_agent,
            "appearance": self.scheme.name(),
            "foldUserMessages": self.fold_user,
            "terminalPlacement": self.terminal_placement.name(),
            "notifications": self.notify.to_json(),
            "permissions": { "autoApprove": self.auto_accept, "projects": self.project_auto_accept },
            "theme": self.theme,
            "reduceMotion": self.reduce_motion.name(),
            "textRenderer": self.text_renderer.name(),
            "terminalFont": self.terminal_font,
            "uiFont": if self.ui_font_name.is_empty() { UI_FONTS[self.ui_font.min(UI_FONTS.len() - 1)].name } else { self.ui_font_name.as_str() },
            "codeFont": if self.code_font_name.is_empty() { CODE_FONTS[self.code_font.min(CODE_FONTS.len() - 1)].name } else { self.code_font_name.as_str() },
            "terminalFontName": self.terminal_font_name,
            "developer": { "rewind": self.record_glitches },
            "tabLayout": self.tab_layout.name(),
            "navigation": self.navigation.name(),
            "tabGrouping": self.tab_grouping.name(),
            "sharedSidePanel": self.shared_side_panel,
            "smoothTimeline": self.smooth_timeline,
            "terminal": { "optionAsMeta": self.terminal_option_meta, "marks": self.terminal_marks },
            "language": self.language,
            "workspaces": { "startIn": self.start_in.name(), "worktree": self.worktree.name(), "lastUsed": self.last_used },
            "followUpBehavior": self.follow_up.name(),
            "questionSubmitKey": self.question_submit.name(),
            "pinchZoom": self.pinch_zoom,
            "spellcheck": self.spellcheck,
            "spellcheckLanguages": self.spellcheck_languages,
            "mobileDiffWrap": self.mobile_diff_wrap,
            "mobileTitlebarPosition": self.mobile_titlebar.name(),
            "chatgptPlanWelcomeSeen": self.chatgpt_welcome_seen,
            "sidebarOrganize": self.sidebar_organize.name(),
            "sounds": self.sounds.to_json(),
            "keybinds": self.keybinds,
            "keepScreenActive": self.keep_screen_active,
            "timelineDetail": self.timeline_detail.to_json(),
            "timelineWidth": self.timeline_width.name(),
            "backgroundActivity": self.background_activity.name(),
            "showTurnDiffs": self.turn_diffs,
            "wrapCodeBlocks": self.wrap_code_blocks,
        })
    }

    pub fn from_json(v: &Value) -> Settings {
        let d = Settings::default();
        Settings {
            show_agent: v["showAgentPicker"].as_bool().unwrap_or(true),
            scheme: Scheme::ALL.into_iter().find(|s| Some(s.name()) == v["appearance"].as_str()).unwrap_or_default(),
            fold_user: v["foldUserMessages"].as_bool().unwrap_or(d.fold_user),
            terminal_placement: DockPlacement::parse(v["terminalPlacement"].as_str()),
            notify: NotifyKinds::from_json(&v["notifications"]),
            auto_accept: v["permissions"]["autoApprove"].as_bool().unwrap_or(false),
            project_auto_accept: v["permissions"]["projects"]
                .as_object()
                .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_bool()?))).collect())
                .unwrap_or_default(),
            theme: theme_id(v["theme"].as_str().unwrap_or_default()),
            reduce_motion: ReduceMotion::parse(v["reduceMotion"].as_str()),
            text_renderer: TextRenderer::parse(v["textRenderer"].as_str()),
            terminal_font: v["terminalFont"].as_u64().map_or(3, |i| (i as usize).min(TERMINAL_FONTS.len() - 1)),
            ui_font: font_index(&UI_FONTS, v["uiFont"].as_str()),
            // Previous versions wrote the old default names into `settings.json` even when
            // untouched; migrate them to the new default OpenMono (`opencode#64797`).
            code_font: font_index(&CODE_FONTS, v["codeFont"].as_str().filter(|&s| s != "IBM Plex Mono")),
            ui_font_name: custom_name(&UI_FONTS, v["uiFont"].as_str()),
            code_font_name: custom_name(&CODE_FONTS, v["codeFont"].as_str().filter(|&s| s != "IBM Plex Mono")),
            terminal_font_name: v["terminalFontName"]
                .as_str()
                .map(str::trim)
                .filter(|&s| s != "JetBrainsMono Nerd Font Mono")
                .unwrap_or("")
                .to_owned(),
            record_glitches: v["developer"]["rewind"].as_bool().unwrap_or(true),
            // Files from before the setting read as Desktop's default; Desktop nests it under
            // `appearance`, which here is the colour scheme.
            tab_layout: TabLayout::parse(v["tabLayout"].as_str().or_else(|| v["appearance"]["tabLayout"].as_str())),
            navigation: Navigation::parse(v["navigation"].as_str().or_else(|| v["appearance"]["navigation"].as_str())),
            tab_grouping: TabGrouping::parse(v["tabGrouping"].as_str().or_else(|| v["appearance"]["tabGrouping"].as_str())),
            shared_side_panel: v["sharedSidePanel"].as_bool().unwrap_or(false),
            question_composer: v["questionComposer"].as_bool(),
            tab_groups: v["tabGroups"].as_bool(),
            sidebar_sections: v["sidebarSections"].as_bool(),
            smooth_timeline: v["smoothTimeline"].as_bool().unwrap_or(false),
            terminal_option_meta: v["terminal"]["optionAsMeta"].as_bool().unwrap_or(false),
            terminal_marks: v["terminal"]["marks"]
                .as_array()
                .map(|a| a.iter().filter_map(|m| m.as_str().map(str::to_owned)).collect())
                .unwrap_or_default(),
            language: v["language"]
                .as_str()
                .filter(|l| opencode_i18n::locales().iter().any(|locale| locale.id() == *l))
                .map(str::to_owned),
            start_in: StartIn::parse(v["workspaces"]["startIn"].as_str()),
            worktree: match v["workspaces"]["worktree"].as_str() {
                Some(name) => Worktree::parse(Some(name)),
                None => Worktree::migrate(v["workspaces"]["defaultDestination"].as_str()),
            },
            // Older files wrote the main checkout as `"local"`.
            last_used: v["workspaces"]["lastUsed"]
                .as_object()
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| match v.as_str()? {
                            "default" | "local" => Some((k.clone(), "default".to_owned())),
                            "workspace" => Some((k.clone(), "workspace".to_owned())),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            follow_up: FollowUp::parse(v["followUpBehavior"].as_str()),
            question_submit: QuestionSubmitKey::parse(v["questionSubmitKey"].as_str()),
            pinch_zoom: v["pinchZoom"].as_bool().unwrap_or(d.pinch_zoom),
            spellcheck: v["spellcheck"].as_bool().unwrap_or(d.spellcheck),
            keep_service_running: v["keepServiceRunning"].as_bool().unwrap_or(d.keep_service_running),
            background_notice_shown: v["backgroundNoticeShown"].as_bool().unwrap_or(d.background_notice_shown),
            mobile_diff_wrap: v["mobileDiffWrap"].as_bool().unwrap_or(d.mobile_diff_wrap),
            mobile_titlebar: TitlebarPosition::parse(v["mobileTitlebarPosition"].as_str()),
            spellcheck_languages: v["spellcheckLanguages"]
                .as_array()
                .map(|a| a.iter().filter_map(|l| Some(l.as_str()?.to_owned())).collect())
                .unwrap_or_default(),
            chatgpt_welcome_seen: v["chatgptPlanWelcomeSeen"].as_bool().unwrap_or(false),
            sidebar_organize: Organize::parse(v["sidebarOrganize"].as_str()),
            sounds: if v["sounds"].is_object() { Sounds::from_json(&v["sounds"]) } else { Sounds::default() },
            keybinds: v["keybinds"]
                .as_object()
                .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned()))).collect())
                .unwrap_or_default(),
            keep_screen_active: v["keepScreenActive"].as_bool().unwrap_or(false),
            timeline_detail: opencode_runtime::detail::Detail::from_json(&v["timelineDetail"]),
            timeline_width: TimelineWidth::parse(v["timelineWidth"].as_str()),
            background_activity: BackgroundActivity::parse(v["backgroundActivity"].as_str()),
            turn_diffs: v["showTurnDiffs"].as_bool().unwrap_or(false),
            wrap_code_blocks: v["wrapCodeBlocks"].as_bool().unwrap_or(false),
        }
    }

    /// Experimental › Organize vertical sidebar › Consolidated (`navigationPrototype`, `settings/model.tsx`).
    pub fn navigation_prototype(&self) -> bool {
        self.sidebar_organize.effective() == Organize::Consolidated
    }

    /// The navigation prototype's List, else Tabs (`navigation`, `settings/model.tsx`).
    pub fn navigation(&self) -> Navigation {
        if self.navigation_prototype() { self.navigation } else { Navigation::Tabs }
    }

    /// How open tabs are grouped: only under Consolidated, else not at all (`tabGrouping`,
    /// `settings/model.tsx:538-540`).
    pub fn tab_grouping(&self) -> TabGrouping {
        if self.navigation_prototype() { self.tab_grouping } else { TabGrouping::None }
    }

    /// The shell layout (`tabLayout`, `settings/model.tsx`): List always uses the sidebar, and the
    /// prototype has no Inbox, so both resolve to side tabs.
    pub fn effective_tab_layout(&self) -> TabLayout {
        let forced = self.navigation_prototype() && (self.navigation == Navigation::List || self.tab_layout == TabLayout::Inbox);
        if forced { TabLayout::Vertical } else { self.tab_layout }
    }

    /// Where tabs sit under the prototype's Tabs mode (`tabPosition`), preserved while List forces side tabs.
    pub fn tab_position(&self) -> TabLayout {
        if self.tab_layout == TabLayout::Horizontal { TabLayout::Horizontal } else { TabLayout::Vertical }
    }

    /// The zoom as a whole percentage, as Appearance › Zoom names it.
    pub fn zoom_percent(&self, zoom: f32) -> String {
        format!("{}", (zoom * 100.0).round() as i32)
    }

    /// Whether the window resolves to dark mode given system contrast window colour and dark flag.
    pub fn is_dark(&self, system_contrast_window: Option<Color>, system_dark: bool) -> bool {
        if let Some(window) = system_contrast_window {
            return window.luminance() < 0.5;
        }
        match self.scheme {
            Scheme::System => system_dark,
            Scheme::Light => false,
            Scheme::Dark => true,
        }
    }
}

/// The sans and mono font family lists `settings` picks, as `AppSpec::families` and
/// `Tx::set_font_families` take them: a typed family before the default.
pub fn families(settings: &Settings) -> (String, String) {
    let pick = |custom: &str, chosen: &Font, base: &Font| {
        if custom.is_empty() {
            chosen.family.to_owned()
        } else {
            format!("{}, {}", css_family(custom), base.family)
        }
    };
    (
        pick(&settings.ui_font_name, &UI_FONTS[settings.ui_font.min(UI_FONTS.len() - 1)], &UI_FONTS[0]),
        pick(&settings.code_font_name, &CODE_FONTS[settings.code_font.min(CODE_FONTS.len() - 1)], &CODE_FONTS[0]),
    )
}

/// A saved theme id: one of Desktop's, or a plugin's (`<plugin>:<id>`), kept as written since
/// plugins register their themes after the settings load.
fn theme_id(id: &str) -> String {
    if id.contains(':') {
        return id.to_owned();
    }
    let all = opencode_ui::themes::all();
    all.get(opencode_ui::themes::index(id)).map_or_else(|| opencode_ui::themes::DEFAULT.to_owned(), |t| t.id.clone())
}

/// The preferences, from memory (the boot packet before the first frame): Desktop's
/// `settings.v3` and `language`, and Rune's own `rune.settings` ([`stored`]).
pub fn load() -> Settings {
    stored::load()
}

/// Written behind; an unchanged set writes nothing.
pub fn save(settings: &Settings) {
    stored::save(settings);
}

/// Opens Settings on its first page, Preferences.
pub fn open_settings(host: &mut dyn SettingsHost) {
    open_settings_at(host, pages::Page::General);
}

/// Opens Settings on `page`, as Desktop's `layout.settings(tab)` does.
pub fn open_settings_at(host: &mut dyn SettingsHost, page: pages::Page) {
    open_settings_inner(host, page, None);
}

/// Opens Settings on `page`, capturing the current focus in `tx` for `OverlayStack` return focus.
pub fn open_settings_tx(host: &mut dyn SettingsHost, page: pages::Page, tx: &mut Tx) {
    let return_focus = tx.focused().map(str::to_owned);
    open_settings_inner(host, page, return_focus);
}

fn open_settings_inner(host: &mut dyn SettingsHost, page: pages::Page, return_focus: Option<String>) {
    host.before_settings_open();
    // Settings' per-server pages start on the server of the page it opens over
    // (`SettingsSurface.server`'s `sourceServer`).
    let was_open = host.settings_open();
    if !was_open {
        let desired = host.desired_server_key();
        host.settings_pages_mut().server.key = desired;
    }
    // What plugins show in Settings (their rows on Plugins, their own pages) draws from their VMs.
    host.wake_settings_plugins();
    host.set_settings_open(true);
    if !was_open {
        host.on_overlay_opened(OVERLAY_ID, return_focus);
    }
    pages::show_page(host, page);
}

/// Closes Settings, clears active search / shortcut capture / field focus, and notifies `OverlayStack`.
pub fn close_settings(host: &mut dyn SettingsHost) {
    let was_open = host.settings_open();
    host.set_settings_open(false);
    search::clear(host);
    host.settings_pages_mut().shortcuts.capture = pages::shortcuts::Capture::Idle;
    host.settings_pages_mut().models_reveal = pages::models::Reveal::Idle;
    host.settings_pages_mut().console_expanded = false;
    host.settings_pages_mut().blur_fields();
    if was_open {
        host.on_overlay_closed(OVERLAY_ID);
    }
}

/// Changes the preferences and saves what the change touched over what's saved, so a launch
/// argument that set one for this run only (`--color-theme`, `--tab-layout`, `--ui-font`)
/// stays out of the saved preferences when another one is changed.
pub fn change_settings(host: &mut dyn SettingsHost, f: impl FnOnce(&mut Settings)) {
    let before = host.settings().to_json();
    f(host.settings_mut());
    persist_settings_diff(host.persists(), &before, host.settings());
}

pub(crate) fn persist_settings_diff(persists: bool, before: &Value, settings: &Settings) {
    if !persists {
        return;
    }
    let after = settings.to_json();
    let mut saved = load().to_json();
    if let (Some(after), Some(saved), Some(before)) = (after.as_object(), saved.as_object_mut(), before.as_object()) {
        for (k, v) in after.iter().filter(|(k, v)| before.get(*k) != Some(*v)) {
            saved.insert(k.clone(), v.clone());
        }
    }
    save(&Settings::from_json(&saved));
}

pub fn set_show_agent(host: &mut dyn SettingsHost, on: bool) {
    change_settings(host, |s| s.show_agent = on);
}

pub fn set_fold_user(host: &mut dyn SettingsHost, on: bool) {
    change_settings(host, |s| s.fold_user = on);
}

pub fn set_notify(host: &mut dyn SettingsHost, kind: NotifyKind, on: bool) {
    change_settings(host, |s| s.notify.set(kind, on));
}

pub fn set_scheme(host: &mut dyn SettingsHost, scheme: Scheme, tx: &mut Tx) {
    // The native window chrome follows the app's scheme at once (`ColorScheme::System` is the
    // desktop's own, which Rune resolves).
    tx.set_color_scheme(match scheme {
        Scheme::System => ColorScheme::System,
        Scheme::Light => ColorScheme::Light,
        Scheme::Dark => ColorScheme::Dark,
    });
    change_settings(host, |s| s.scheme = scheme);
}

pub fn set_terminal_placement(host: &mut dyn SettingsHost, placement: DockPlacement, tx: &mut Tx) {
    tx.instant();
    change_settings(host, |s| s.terminal_placement = placement);
}

/// Settings' keys: the shortcut being recorded takes every key; Escape steps back through
/// search and then leaves; mod+f focuses the search (`settings.search.focus`).
pub fn settings_key(host: &mut dyn SettingsHost, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if !host.settings_open() {
        return false;
    }
    if pages::shortcuts::key(host, ev, tx) || pages::filter_page_key(host, ev, tx) || search::key(host, ev, tx) {
        return true;
    }
    if ev.key == Key::Escape {
        tx.instant();
        if matches!(host.settings_pages().page, pages::Page::Project(..)) {
            pages::open_page(host, pages::Page::Projects, tx);
            return true;
        }
        back_to_app(host, tx);
        return true;
    }
    false
}

/// Escape or Back to app leaves Settings.
pub fn back_to_app(host: &mut dyn SettingsHost, tx: &mut Tx) {
    if host.try_navigate_back_from_settings(tx) {
        return;
    }
    close_settings(host);
    host.restore_focus_after_settings_close(tx);
}

/// Opens Settings on Preferences or closes it when already open.
pub fn toggle_settings(host: &mut dyn SettingsHost, tx: &mut Tx) {
    tx.instant();
    if host.settings_open() {
        close_settings(host);
    } else {
        open_settings_tx(host, pages::Page::General, tx);
        host.on_toggle_settings_opened(tx);
    }
}

/// Runs every Settings per-frame poll in one call (search reveal, server retry, worktree delete,
/// project blur-save, About wordmark scramble, and Projects inline rename blur).
pub fn poll(host: &mut dyn SettingsHost, now: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    changed |= search::poll(host, tx);
    changed |= pages::models::poll(host, tx);
    pages::servers::unavailable::poll(host, tx);
    changed |= pages::worktrees::poll(host, tx);
    changed |= pages::project::poll(host, tx);
    changed |= pages::about::poll(host, now, tx);
    changed |= pages::projects::poll(host);
    changed
}

/// `.settings-section-title`: 13 on 16 at 530.
const SECTION: TextStyle = TextStyle::new(13.0, 530.0).lh(16.0);
/// `settings-row-description`: 13 on 20 at 440.
const BODY: TextStyle = TextStyle::new(13.0, 440.0).lh(20.0);
/// `settings-row-title`: 13 on 16 at 530, -0.04 tracking.
const ROW_TITLE: TextStyle = TextStyle::new(13.0, 530.0).lh(16.0).tracking(-0.04);

/// The Settings screen in a panel `size` points large (0 when not measured yet).
pub fn page<H: SettingsHost + PopupHost + 'static>(s: &H, p: &Pal, size: (f32, f32)) -> El {
    shell::view(s, p, size)
}

/// `.settings-section`: its title, 16 above its list.
pub fn section(p: &Pal, title: &str, list: El) -> El {
    col().w_full().gap(16.0).child(text(title.to_owned()).font(SECTION).color(p.text)).child(list)
}

pub fn settings_card<'a, K: AsRef<str>, D: AsRef<str>>(p: &Pal, key: &str, items: impl IntoIterator<Item = (K, &'a str, D, El)>) -> El {
    list(p, key, items.into_iter().map(|(k, title, description, control)| setting(p, k.as_ref(), title, description.as_ref(), control)).collect())
}

/// `[data-component="settings-list"]`: a `layer-01` card with 8 pt corners, 20 pt in from each
/// side and a 0.5 pt `border-muted` ring inside; its rows split by `settings-row`'s 0.5 px
/// `border-base` bottom border, which takes a device pixel.
pub fn list(p: &Pal, key: &str, rows: Vec<El>) -> El {
    let n = rows.len();
    rows.into_iter().enumerate().fold(
        col().key(key.to_owned()).w_full().px(20.0).radius(8.0).fill(p.layer1).box_shadow(Pal::ring_in(p.border_muted)),
        |l, (i, r)| l.child(r).children((i + 1 < n).then(|| rule(0.5, p.border))),
    )
}

/// `SettingsRow`: its title and description beside its control, the control centred on them.
pub fn setting(p: &Pal, key: &str, title: &str, description: &str, control: El) -> El {
    setting_at(p, key, title, description, control, true)
}

/// A row; its control centred on the text, or at its top (`SettingsRow align="start"`). A row
/// settings search can reveal carries its `data-action` as `setting-<key>`.
pub fn setting_at(p: &Pal, key: &str, title: &str, description: &str, control: El, centred: bool) -> El {
    let about = (!description.is_empty()).then(|| text(description.to_owned()).key(format!("setting-{key}-about")).font(BODY).color(p.muted).wrap());
    setting_row(p, key, title, about, control, centred)
}

thread_local! {
    /// The page is 520 pt or narrower (`@container settings-panel (max-width: 520px)`): each
    /// row's control wraps under its words, at the row's end.
    static ROWS_WRAP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The screen is under 800 pt (`@container settings-screen (max-width: 799px)`).
    static COMPACT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Notes the page's width (`.settings-panel`'s, 0 at full size) for the rows this frame builds,
/// and whether the window is at least 640 wide.
pub(crate) fn note_panel_width(width: f32, sm: bool) {
    ROWS_WRAP.set((width > 0.0 && width <= 520.0) || !sm);
    COMPACT.set(width > 0.0);
}

/// The rows wrap their controls under their words (a page 520 wide or less, or a window under 640).
pub fn rows_wrap() -> bool {
    ROWS_WRAP.get()
}

/// The screen has the narrow layout: Back and the page menu over the page.
pub fn compact() -> bool {
    COMPACT.get()
}

/// A row whose description is built apart (a link after its words).
pub fn setting_with(p: &Pal, key: &str, title: &str, about: El, control: El, centred: bool) -> El {
    setting_row(p, key, title, Some(about), control, centred)
}

fn setting_row(p: &Pal, key: &str, title: &str, about: Option<El>, control: El, centred: bool) -> El {
    let copy = col()
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(8.0)
        .child(text(title.to_owned()).key("title").font(ROW_TITLE).color(p.text).wrap())
        // `margin-block: -3.5px` keeps its 20 pt lines on the title's 16 pt rhythm.
        .children(about.map(|a| a.key("about").my(-3.5)));
    if ROWS_WRAP.get() {
        return col()
            .key(format!("setting-{key}"))
            .w_full()
            .gap(16.0)
            .py(20.0)
            .child(copy.w_full())
            .child(row().key("control").w_full().justify_end().child(control));
    }
    let r = row().key(format!("setting-{key}")).w_full().gap(16.0).py(20.0);
    // `data-align="start"` is `align-items: flex-start` (`settings.css:819`): the control keeps
    // its own height beside a taller description rather than stretching to it.
    let r = if centred { r.center_y() } else { r.items_start() };
    r.child(copy).child(row().key("control").justify_end().child(control))
}

/// `settings-row-description` text, for a description built apart.
pub fn about(p: &Pal, words: &str) -> El {
    text(words.to_owned()).font(BODY).color(p.muted)
}

/// `a.settings-link`: accent, its hover ink under the pointer, opening `url`.
pub fn link<H: SettingsHost + 'static>(p: &Pal, key: &str, words: &str, url: &'static str) -> El {
    text(words.to_owned())
        .key(key.to_owned())
        .font(BODY)
        .color(p.accent)
        .link(rune::Link::new(0..words.len(), url).role("link").label(words.to_owned()).hover(rune::LinkPaint::new().color(p.accent_hover)))
        .on_link(|s: &mut H, href: &str, tx: &mut Tx| open_external(s, href, tx))
}

pub(crate) fn open_external(host: &mut dyn SettingsHost, url: &str, tx: &mut Tx) {
    host.open_external(url, tx);
}

/// Desktop's switch in a settings row (`switch.css`, compact, as the web draws it), with the 4 pt
/// padding a row gives it, named by the row's `title` and described by `about`.
pub fn switch(p: &Pal, key: &str, on: bool) -> El {
    Switch::new(on).web().build(kit(p)).key(key.to_owned()).padding(4.0).labelled_by("title").described_by("about")
}

/// `--v2-elevation-button-neutral`: a drop and the 0.5px ring outside the button, and in dark the
/// 0.5px lit top edge (`theme.css`).
pub(crate) fn neutral_lift(p: &Pal) -> [Shadow; 4] {
    let (y, blur) = if p.dark { (1.0, 2.0) } else { (1.0, 1.5) };
    let top = if p.dark { Shadow { y: -0.5, color: Color::WHITE.alpha(0.1), ..Shadow::default() } } else { Shadow::default() };
    [Shadow { y, blur, color: p.q.neutral_shadow, ..Shadow::default() }, Pal::ring_out(p.q.neutral_ring), top, Shadow::default()]
}

/// `--v2-elevation-input` (`packages/ui/src/styles/tokens/theme.css:115-117, 235-238`): identical
/// to `neutral_lift` in light; in dark, zero drop shadow, a 16% light 0.5px ring, and a 6% light top edge.
pub(crate) fn input_lift(p: &Pal) -> [Shadow; 4] {
    if !p.dark {
        return neutral_lift(p);
    }
    [
        Shadow::default(),
        Pal::ring_out(Color::WHITE.alpha(0x29 as f32 / 255.0)),
        Shadow { y: -0.5, color: Color::WHITE.alpha(0x0f as f32 / 255.0), ..Shadow::default() },
        Shadow::default(),
    ]
}

pub(crate) fn ui_direction() -> Direction {
    match opencode_i18n::direction() {
        opencode_i18n::Direction::Ltr => Direction::Ltr,
        opencode_i18n::Direction::Rtl => Direction::Rtl,
    }
}
