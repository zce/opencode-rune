//! Object-safe host boundary for system notifications and actionable permission/question toasts.

use opencode_kit::client::Server;
use opencode_kit::servers::ServerKey;
use rune::Tx;
use serde_json::Value;

use crate::{Kinds, Notifier};

/// An inline answer submitted from an actionable notification button or reply field.
#[derive(Clone, Debug, PartialEq)]
pub enum NotificationReply<'a> {
    /// Permission decision: `"once"`, `"always"`, or `"reject"`.
    Permission(&'a str),
    /// Question/form field answers JSON object (`{ field: value }`).
    Question(Value),
}

/// Object-safe host boundary for system notifications, actionable toasts, and plugin notifications.
///
/// Every entrypoint in `opencode-feature-notifications` operates on non-generic
/// `&dyn NotificationsHost` / `&mut dyn NotificationsHost`.
pub trait NotificationsHost: opencode_kernel::AppEnv {
    /// Per-window notifier state (read).
    fn notifier(&self) -> &Notifier;
    /// Per-window notifier state (write — callers only invoke when mutating).
    fn notifier_mut(&mut self) -> &mut Notifier;

    /// Which notification kinds are enabled (`Settings › Notifications`).
    fn notification_prefs(&self) -> Kinds {
        Kinds::default()
    }
    /// Live server connection, when on a live source.
    fn live_server(&self) -> Option<&Server> {
        None
    }

    /// Whether the app window is currently focused (defaults to `rune_notify::app_focused()`).
    fn is_window_focused(&self) -> bool {
        rune_notify::app_focused()
    }
    /// Brings this window forward after a click on one of its notifications. `token` is the
    /// server's activation token when it sent one (on Wayland the compositor needs it to let
    /// the window take focus).
    fn activate_window(&mut self, token: Option<&str>) {
        let _ = token;
        rune_notify::bring_to_front();
    }
    /// Unique identifier of this window, recorded on posted toasts so button clicks route back.
    fn window_id(&self) -> &str {
        ""
    }
    /// Whether another window with `window_id` is still alive.
    fn window_exists(&self, window_id: &str) -> bool {
        let _ = window_id;
        false
    }

    /// Server key the window is currently checked out on.
    fn active_server_key(&self) -> ServerKey {
        ServerKey::sidecar()
    }
    /// Base URL of the app's own local sidecar server, if known (for legacy address-based clicks).
    fn own_server_base(&self) -> Option<String> {
        None
    }

    /// Title of session `session_id`, if loaded.
    fn session_title(&self, session_id: &str) -> Option<String> {
        let _ = session_id;
        None
    }
    /// Root session ID for `session_id` (or `session_id` itself for root/unknown sessions).
    fn root_session_id<'a>(&'a self, session_id: &'a str) -> &'a str {
        session_id
    }
    /// Whether `session_id` is a root session (no parent, or not yet loaded).
    fn is_root_session(&self, session_id: &str) -> bool {
        self.root_session_id(session_id) == session_id
    }
    /// Whether any direct child session of `session_id` is currently running.
    fn has_running_child(&self, session_id: &str) -> bool {
        let _ = session_id;
        false
    }
    /// Whether `session_id` (or its root) is muted and should not post notifications.
    fn is_session_silenced(&self, session_id: &str) -> bool {
        let _ = session_id;
        false
    }
    /// Whether `session_id` automatically accepts permission requests (suppresses permission toasts).
    fn auto_accepts_permission(&self, session_id: &str) -> bool {
        let _ = session_id;
        false
    }

    /// Whether `session_id` has an open tab on the active server.
    fn is_tab_open(&self, session_id: &str) -> bool {
        let _ = session_id;
        false
    }
    /// Display title for `session_id` when it has an open tab on the active server (`None` if no tab).
    fn tab_title(&self, session_id: &str) -> Option<String> {
        self.is_tab_open(session_id)
            .then(|| self.session_title(session_id).unwrap_or_default())
    }
    /// Whether any open tab in this window matches `root_session_id` (used when waiting on
    /// pending requests for an actionable toast button click).
    fn has_tab_for_root(&self, root_session_id: &str) -> bool {
        self.is_tab_open(root_session_id)
    }

    /// Whether pending request `request_id` is currently loaded in this window.
    fn has_pending_request(&self, request_id: &str) -> bool {
        let _ = request_id;
        false
    }
    /// Triggers a read of pending permission/form requests for `session_id`.
    fn read_requests(&mut self, session_id: &str) {
        let _ = session_id;
    }

    /// Opens `session_id` on `server` after a notification click (closing Settings, switching
    /// servers if needed, and navigating to the session).
    fn open_session_from_notification(&mut self, server: &ServerKey, session_id: &str, tx: &mut Tx) {
        let _ = (server, session_id, tx);
    }
    /// Submits an inline permission decision or question/form reply triggered from a toast button.
    fn reply_permission_or_question(&mut self, request_id: &str, reply: NotificationReply<'_>) {
        let _ = (request_id, reply);
    }
    /// Marks `session_id` as read locally (optional host hook for callers coordinating read state).
    fn mark_read(&mut self, session_id: &str) {
        let _ = session_id;
    }
    /// Displays an in-app notification requested by a plugin (`Notifications:Notify`).
    fn show_plugin_notification(&mut self, title: String, body: String, session_id: Option<&str>) {
        let _ = (title, body, session_id);
    }
    /// Dispatches a click on an OS notification created by a browser extension (`chrome.notifications`).
    fn on_extension_notification_clicked(&mut self, ext_id: &str, notif_id: &str) {
        let _ = (ext_id, notif_id);
    }
    /// Whether this window's browser engine runs extensions, so an extension toast's click can
    /// reach them when the window that posted it has gone.
    fn takes_extension_clicks(&self) -> bool {
        false
    }
}
