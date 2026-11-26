//! Desktop host adapter for `opencode-feature-notifications` (`features/notifications`).
//!
//! `rune-notify` is a no-op (`available() == false`) on `wasm32` (it uses Notification Center on
//! macOS and Windows toasts on Windows, not the browser Notifications API), so the feature crate
//! and `rune-notify` are omitted on `wasm32`.

#[cfg(not(target_arch = "wasm32"))]
#[allow(unused_imports)]
pub use opencode_feature_notifications::{
    Ask, CAP, Choice, Coordinator, Kind, Kinds, Note, NotificationReply, NotificationsHost,
    Notifier, Outcome, Sink, actions, answered, clear_all_extensions, clear_extension, extension_notifications_allowed,
    host, notify_event, notify_plugin, on_action, poll, post_extension, quitting, seen, take_extension_evicted,
};

#[cfg(not(target_arch = "wasm32"))]
use opencode_kernel::{Feature, KernelBuilder, Slot};
#[cfg(not(target_arch = "wasm32"))]
use opencode_kit::client::Server;
#[cfg(not(target_arch = "wasm32"))]
use opencode_kit::servers::ServerKey;
#[cfg(not(target_arch = "wasm32"))]
use rune::Tx;

use crate::App;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) static NOTIFIER: Slot<Notifier> = Slot::new();

/// Whether this window posts to system notifications (always `false` on `wasm32`).
pub(crate) fn shows(s: &App) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        s.kernel.slots.get(&NOTIFIER).shows()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = s;
        false
    }
}

#[cfg(target_arch = "wasm32")]
pub fn answered(_: &mut App, _: &str) {}

#[cfg(target_arch = "wasm32")]
pub fn seen(_: &mut App, _: &str) {}

#[cfg(target_arch = "wasm32")]
pub fn quitting(_: &mut App) {}

#[cfg(not(target_arch = "wasm32"))]
impl NotificationsHost for App {
    fn notifier(&self) -> &Notifier {
        self.kernel.slots.get(&NOTIFIER)
    }

    fn notifier_mut(&mut self) -> &mut Notifier {
        self.kernel.slots.get_mut(&NOTIFIER)
    }

    fn notification_prefs(&self) -> Kinds {
        self.settings.notify
    }

    fn live_server(&self) -> Option<&Server> {
        match &*self.source {
            crate::Source::Live { server, .. } => Some(server),
            crate::Source::Fixture => None,
        }
    }

    fn is_window_focused(&self) -> bool {
        crate::windows::app_focused(self)
    }

    fn window_id(&self) -> &str {
        &self.window.id
    }

    fn window_exists(&self, window_id: &str) -> bool {
        crate::windows::handle(window_id).is_some()
    }

    fn active_server_key(&self) -> ServerKey {
        crate::servers::runtime::active(self).clone()
    }

    fn own_server_base(&self) -> Option<String> {
        crate::servers::runtime::own_server().map(|s| s.base)
    }

    fn session_title(&self, session_id: &str) -> Option<String> {
        self.session(session_id).map(|s| s.title.clone())
    }

    fn root_session_id<'a>(&'a self, session_id: &'a str) -> &'a str {
        self.root_of(session_id)
    }

    fn is_root_session(&self, session_id: &str) -> bool {
        self.session(session_id).is_none_or(|s| s.parent.is_none())
    }

    fn is_session_silenced(&self, session_id: &str) -> bool {
        crate::session_menu::preferences::silenced(self, crate::servers::runtime::active(self), session_id)
    }

    fn auto_accepts_permission(&self, session_id: &str) -> bool {
        self.auto_accepts(session_id)
    }

    fn is_tab_open(&self, session_id: &str) -> bool {
        let server = crate::servers::runtime::active(self);
        self.tabs.iter().any(|t| !t.is_draft() && t.id == session_id && t.server == *server)
    }

    fn tab_title(&self, session_id: &str) -> Option<String> {
        let server = crate::servers::runtime::active(self);
        let tab = self.tabs.iter().find(|t| !t.is_draft() && t.id == session_id && t.server == *server)?;
        let title = self.session(session_id).map_or(tab.title.as_str(), |s| s.title.as_str());
        Some(title.to_owned())
    }

    fn has_tab_for_root(&self, root_session_id: &str) -> bool {
        self.tabs.iter().any(|t| !t.is_draft() && t.id == root_session_id)
    }

    fn has_pending_request(&self, request_id: &str) -> bool {
        self.pending.iter().any(|p| p.id() == request_id)
    }

    fn read_requests(&mut self, session_id: &str) {
        App::read_requests(self, session_id);
    }

    fn activate_window(&mut self, token: Option<&str>) {
        #[cfg(target_os = "linux")]
        {
            if let Some(handle) = crate::windows::handle(&self.window.id) {
                match token.filter(|t| !t.is_empty()) {
                    Some(token) => handle.activate(token),
                    None => handle.focus(),
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = token;
            rune_notify::bring_to_front();
        }
    }

    fn open_session_from_notification(&mut self, server: &ServerKey, session_id: &str, tx: &mut Tx) {
        self.close_settings();
        crate::servers::runtime::switch(self, server);
        self.open(session_id, tx);
    }

    fn reply_permission_or_question(&mut self, request_id: &str, reply: NotificationReply<'_>) {
        match reply {
            NotificationReply::Permission(decision) => self.reply_permission(request_id, decision),
            NotificationReply::Question(answers) => {
                self.send_form(request_id, crate::question::reply::Action::Reply(answers));
            }
        }
    }

    fn show_plugin_notification(&mut self, title: String, body: String, _session_id: Option<&str>) {
        crate::status_toast::say(self, title, body);
    }

    fn on_extension_notification_clicked(&mut self, ext_id: &str, notif_id: &str) {
        self.workspace_panes.browser.host.send_extension_notification_event(
            ext_id.to_owned(),
            notif_id.to_owned(),
            true,
            true,
        );
    }

    fn takes_extension_clicks(&self) -> bool {
        let host = &self.workspace_panes.browser.host;
        host.engine_extensions() && matches!(host.status(), browser_pane::EngineStatus::Ready)
    }
}

/// Posts the notification event `e` calls for, if any (from `App::react`).
pub fn event(app: &mut App, e: &crate::events::StreamEvent, sid: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    notify_event(app, &e.ty, &e.data, sid, e.done.running_child, &mut |want| crate::events::StreamEvent::take(&e.notice, want));
    #[cfg(target_arch = "wasm32")]
    let _ = (app, e, sid);
}

/// A notification's link, opened with the app (`deep_links`).
#[cfg(not(target_arch = "wasm32"))]
pub fn clicked(_: &mut App, click: &rune_notify::Click) {
    opencode_feature_notifications::clicked(click);
}

/// Gives a windowed run the app's identity before its window opens.
#[cfg(not(target_arch = "wasm32"))]
pub fn identify() {
    let product = crate::launch::product();
    #[cfg(target_os = "linux")]
    if !crate::headless() {
        // The installed desktop entry's file name, where the notification server finds the app's
        // icon. The build's own name is what the notification shows.
        if let Err(e) = rune_notify::register(&rune_notify::Identity::new("opencode-rune", product.name())) {
            eprintln!("notifications: {e}");
        }
    }
    opencode_feature_notifications::identify(&product, crate::headless());
}

/// Settles Windows toast registration and starts serving toast activations off the UI thread.
pub fn settle_identity() {
    #[cfg(not(target_arch = "wasm32"))]
    opencode_feature_notifications::settle_identity(&crate::launch::product(), crate::headless());
}

/// Captures a COM toast-button activation when launched after quit.
#[cfg(not(target_arch = "wasm32"))]
pub fn take_activation(args: &[String]) {
    opencode_feature_notifications::take_activation(args, &crate::launch::product());
}

impl App {
    pub(crate) fn server_base(&self) -> &str {
        match &*self.source {
            crate::Source::Live { server, .. } => &server.base,
            crate::Source::Fixture => "",
        }
    }

    pub(crate) fn live_status(&self) -> String {
        format!("Live · {}", self.server_base().trim_start_matches("http://"))
    }
}

/// Kernel feature registration for system notifications and actionable toasts.
#[cfg(not(target_arch = "wasm32"))]
pub struct NotificationsFeature;

#[cfg(not(target_arch = "wasm32"))]
impl Feature<App> for NotificationsFeature {
    fn id(&self) -> &'static str {
        "feature.notifications"
    }

    fn register(&self, builder: &mut KernelBuilder<App>) {
        builder.register_slot(&NOTIFIER);
        builder.schedule(
            crate::tick::stage::INGEST_DISPATCH,
            "notifications.poll",
            |s, _, tx| poll(s, tx),
        );
    }
}
