//! Desktop `App` entrypoints and kernel [`Feature`] registration for Settings (commands,
//! overlays, and per-frame polling).

use super::{SETTINGS_PAGES, SETTINGS_SCREEN};
use opencode_feature_settings::{
    ABOUT_NOTICES_OVERLAY_ID, OVERLAY_ID, PROVIDER_REMOVAL_OVERLAY_ID, Scheme, Settings,
    WORKTREE_DELETE_OVERLAY_ID, back_to_app, change_settings, close_settings, open_settings,
    open_settings_at, pages, search, set_fold_user, set_scheme, set_show_agent, settings_key,
    toggle_settings,
};
use opencode_kernel::{CommandEntry, Feature, KernelBuilder, OverlayRegistration};
use opencode_kit::model::Project;
use rune::{KeyEvent, Tx};

use crate::App;
use crate::keymap::Bind;

impl App {
    pub fn settings_open(&self) -> bool {
        self.kernel.slots.get(&SETTINGS_SCREEN).is_open()
    }

    pub fn set_settings_open(&mut self, open: bool) {
        self.kernel.slots.get_mut(&SETTINGS_SCREEN).set_open(open);
    }

    pub fn dark(&self, cx: &rune::Cx<App>) -> bool {
        self.settings.is_dark(cx.system_contrast().map(|c| c.window), cx.system_dark())
    }

    pub fn open_settings(&mut self) {
        open_settings(self);
    }

    pub fn open_settings_at(&mut self, page: pages::Page) {
        open_settings_at(self, page);
    }

    pub fn close_settings(&mut self) {
        close_settings(self);
    }

    pub(crate) fn change_settings(&mut self, f: impl FnOnce(&mut Settings)) {
        change_settings(self, f);
    }

    pub fn set_show_agent(&mut self, on: bool) {
        set_show_agent(self, on);
    }

    pub fn set_fold_user(&mut self, on: bool) {
        set_fold_user(self, on);
    }

    pub fn set_scheme(&mut self, scheme: Scheme, tx: &mut Tx) {
        set_scheme(self, scheme, tx);
    }

    pub fn settings_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        settings_key(self, ev, tx)
    }

    pub fn toggle_settings(&mut self, tx: &mut Tx) {
        toggle_settings(self, tx);
    }

    pub fn zoom_by(&mut self, dir: i32, tx: &mut Tx) {
        tx.instant();
        self.set_zoom(dir);
    }

    /// Sets the window's zoom to `level` (Appearance › Zoom), clamped as the zoom keys clamp it.
    pub fn set_zoom_level(&mut self, level: f32) {
        *self.zoom = level.clamp(crate::shortcuts::ZOOM_MIN, crate::shortcuts::ZOOM_MAX);
        self.gestures.zoom_pinch.leveled();
        if self.persists() {
            crate::tabs::save_zoom(*self.zoom);
        }
    }

    pub fn show_settings_page(&mut self, page: pages::Page) {
        pages::show_page(self, page);
    }

    pub fn open_settings_page(&mut self, page: pages::Page, tx: &mut Tx) {
        pages::open_page(self, page, tx);
    }

    pub fn edit_project(&mut self, dir: &str, tx: &mut Tx) {
        pages::project::edit_project(self, dir, tx);
    }

    pub fn apply_project(&mut self, updated: Project) {
        match self.projects.iter_mut().find(|p| p.id == updated.id) {
            Some(p) => *p = updated,
            None => self.projects.push(updated),
        }
    }
}

/// Kernel feature registration for the Settings screen, overlays, and per-frame poll.
pub struct SettingsFeature;

impl Feature<App> for SettingsFeature {
    fn id(&self) -> &'static str {
        "feature.settings"
    }

    fn register(&self, builder: &mut KernelBuilder<App>) {
        builder
            .register_slot(&SETTINGS_SCREEN)
            .register_slot(&SETTINGS_PAGES);
        builder.add_command(
            CommandEntry::<App>::from_key(
                "settings",
                opencode_i18n::key!("command.settings.open"),
                crate::commands::order::SETTINGS,
                |s, tx| s.toggle_settings(tx),
            )
            .bind_id(Bind::Settings.id()),
        );
        // Flips Text rendering from the palette, to compare renderers on the same screen.
        builder.add_command(CommandEntry::<App>::from_key(
            "text-rendering.cycle",
            opencode_i18n::key!("rune.textRendering.cycle"),
            crate::commands::order::TEXT_RENDERING_CYCLE,
            |s, _| s.set_text_renderer(s.settings.text_renderer.cycle()),
        ));
        builder.register_overlay(
            OverlayRegistration::<App>::new(ABOUT_NOTICES_OVERLAY_ID)
                .with_render(crate::overlays::layer::ACTION_DIALOG, |s, cx| {
                    let p = crate::palette::pal(s.dark(cx));
                    pages::about::notices_dialog(s, p, cx)
                })
                .above(crate::terminal::PASTE_OVERLAY_ID)
                .with_dismiss(|s, tx| {
                    tx.instant();
                    pages::about::close_notices(s);
                }),
        );
        builder.register_overlay(
            OverlayRegistration::<App>::new(WORKTREE_DELETE_OVERLAY_ID)
                .with_render(crate::overlays::layer::ACTION_DIALOG, |s, cx| {
                    let p = crate::palette::pal(s.dark(cx));
                    pages::worktrees::dialog(s, p, cx)
                })
                .above(crate::terminal::PASTE_OVERLAY_ID)
                .with_dismiss(|s, tx| {
                    let attempt = match &s.kernel.slots.get(&SETTINGS_PAGES).worktrees.delete.phase {
                        pages::worktrees::Phase::ConfirmingOne { attempt, .. }
                        | pages::worktrees::Phase::ConfirmingMany { attempt, .. } => Some(*attempt),
                        _ => None,
                    };
                    if let Some(attempt) = attempt {
                        tx.instant();
                        pages::worktrees::dispatch(s, pages::worktrees::Event::ConfirmCancelled { attempt });
                    }
                }),
        );
        builder.register_overlay(
            OverlayRegistration::<App>::new(OVERLAY_ID)
                // Full-page Settings renders inside `shell`'s main panel (not as an overlay), so its
                // key handler runs last in `key_band::LOW`, after both switchers.
                .with_key_in(crate::overlays::key_band::LOW, |s, ev, tx| s.settings_key(ev, tx))
                .with_dismiss(|s, tx| back_to_app(s, tx)),
        );
        builder.register_overlay(
            OverlayRegistration::<App>::new(PROVIDER_REMOVAL_OVERLAY_ID)
                .with_render(crate::overlays::layer::ELEVATED_DIALOG, |s, cx| {
                    if s.settings_open() {
                        let p = crate::palette::pal(s.dark(cx));
                        pages::providers::confirmation(s, p, cx)
                    } else {
                        Vec::new()
                    }
                })
                .above("dialog.servers")
                .with_dismiss(|s, tx| {
                    pages::providers::step_accounts(s, pages::providers::Event::Cancel, tx);
                }),
        );
        use crate::tick::stage;
        builder.schedule(stage::INGEST_LOCAL, "settings.search", |s, _, tx| search::poll(s, tx));
        builder.schedule(stage::INGEST_LOCAL, "settings.servers_unavailable", |s, _, tx| {
            pages::servers::unavailable::poll(s, tx);
            false
        });
        builder.schedule(stage::DERIVE_TIMELINE, "settings.worktrees", |s, _, tx| pages::worktrees::poll(s, tx));
        builder.schedule(stage::REACT_STATUS, "settings.project", |s, _, tx| pages::project::poll(s, tx));
        builder.schedule(stage::REACT_STATUS, "settings.about", |s, now, tx| pages::about::poll(s, now, tx));
        // Precaution: shares `SETTINGS_PAGES.project` with `settings.project`.
        builder.schedule(stage::REACT_STATUS, "settings.projects", |s, _, _| pages::projects::poll(s)).after("settings.project");
    }
}
