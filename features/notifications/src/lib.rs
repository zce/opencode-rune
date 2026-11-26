//! System notifications, as OpenCode Desktop posts them (`shell/notifications/notification.tsx` and
//! the desktop platform's `notify`): when an agent finishes (off by default), when a permission is
//! asked (on), when the agent asks a question or an MCP server asks for input (on) and when a run
//! fails (off), for a session with an open tab, and only while the window isn't focused. A click
//! brings the app forward and opens that session. At most 40 stay in the system's list; posting
//! another takes the oldest out.
//!
//! Only a live window with a person at it posts (`NotificationsHost::persists`), through
//! `rune_notify`: Notification Center on macOS, toasts on Windows (`identify` gives the app its
//! name and icon there). On `wasm32` and Linux where `rune_notify::available()` is `false`,
//! platform calls are safe no-ops.

#![forbid(unsafe_code)]

pub mod actions;
pub mod host;

use std::collections::VecDeque;
use std::sync::Mutex;

use opencode_kit::servers::ServerKey;
use rune::{Tx, Waker};
use serde_json::Value;

pub use actions::{Ask, Choice, Outcome};
pub use host::{NotificationReply, NotificationsHost};
pub use opencode_runtime::{NotifyKind, NotifyKind as Kind, NotifyKinds, NotifyKinds as Kinds};

/// Desktop's `MAX_ACTIVE_NOTIFICATIONS`.
pub const CAP: usize = 40;
/// Permission requests already posted, so a repeated `permission.asked` doesn't post twice.
const ASKED: usize = 500;

/// One notification to post: what it says and the session a click opens.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// The same for the same request (`req-<id>`) or the same session's run (`run-<id>`), so a
    /// repeat replaces it and an answer anywhere takes it out ([`answered`]).
    pub id: String,
    pub kind: Kind,
    /// Desktop's title for what asked: usually `kind`'s, "Input requested" for an MCP server's form.
    pub title: &'static str,
    pub body: String,
    pub session: String,
    /// What its buttons answer ([`actions`]): a permission's decisions or a question's answers.
    pub ask: Option<actions::Ask>,
    /// The window that posted it, which its buttons' answers go to.
    pub window: String,
}

/// Where notifications go: the system, or a custom sink.
pub trait Sink {
    /// Whether the window is focused right now; nothing posts while it is.
    fn focused(&self) -> bool;
    fn post(&mut self, note: &Note, server: &str);
    fn remove(&mut self, id: &str);
    /// Posts a raw OS notification (used by browser extensions' `chrome.notifications`, titled with
    /// the extension's name so it never impersonates the app).
    fn post_raw(&mut self, _id: &str, _title: &str, _body: &str, _info: &[(&str, &str)]) -> bool {
        false
    }
}

/// Notification sink initialization lifecycle.
#[derive(Default)]
enum SinkState {
    /// System notification support has not been probed yet.
    #[default]
    Uninitialized,
    /// Probed (or explicitly cleared) with no active notification sink.
    Unavailable,
    /// Connected to the system notification center or a custom sink.
    Ready(Box<dyn Sink>),
}

enum SinkEvent {
    Probe { available: bool },
    Install(Option<Box<dyn Sink>>),
}

enum SinkEffect {
    None,
    ConnectSystem,
}

fn step_sink(state: &mut SinkState, event: SinkEvent) -> SinkEffect {
    match event {
        SinkEvent::Probe { available } => {
            if !matches!(state, SinkState::Uninitialized) {
                return SinkEffect::None;
            }
            if available {
                *state = SinkState::Ready(Box::new(System));
                SinkEffect::ConnectSystem
            } else {
                *state = SinkState::Unavailable;
                SinkEffect::None
            }
        }
        SinkEvent::Install(sink) => {
            *state = match sink {
                Some(sink) => SinkState::Ready(sink),
                None => SinkState::Unavailable,
            };
            SinkEffect::None
        }
    }
}

#[derive(Default)]
pub struct Notifier {
    state: SinkState,
}

/// Clicks from any thread, for whichever window they're for ([`actions::take`]).
pub(crate) static CLICKS: Mutex<Vec<rune_notify::Click>> = Mutex::new(Vec::new());
/// Every window's waker, so a click wakes the one it's for.
static WAKERS: Mutex<Vec<Waker>> = Mutex::new(Vec::new());

/// What every window and every server's events share, as Desktop's one `NotificationCoordinator`
/// for all its servers (`shell/notifications/coordinator.ts`, `runtime/server/runtime.tsx:55`):
/// the requests already posted, claimed as `<server>\0<request id>`, and the notifications
/// posted this run, oldest first, at most [`CAP`] in the system's list.
#[derive(Default)]
pub struct Coordinator {
    /// Posted ids, oldest first, with the server they're from.
    shown: VecDeque<(String, String)>,
    asked: VecDeque<String>,
    /// Browser-extension toasts, oldest first: `(os id, extension id, its notification id, the
    /// window whose engine posted it)`. Kept apart from `shown` so an extension can never push one
    /// of OpenCode's own toasts out.
    ext_shown: VecDeque<(String, String, String, String)>,
    /// Extension toasts the cap pushed out, (window, extension id, notification id), until that window's poll.
    ext_evicted: Vec<(String, String, String)>,
}

/// Browser-extension toasts the app keeps in the system's list at once, across all extensions.
/// Official sets none (`extension-api/notifications.ts`: one Electron `Notification` each, the OS
/// keeps them); here they share the app's list with permission and question toasts, so they get a
/// quarter of `CAP`: room for a few extensions to have a couple each.
pub const EXT_CAP: usize = 10;

thread_local! {
    static COORDINATOR: std::cell::RefCell<Coordinator> = std::cell::RefCell::default();
}

impl Coordinator {
    /// Claims request `id` of `server` for one notification; false when some window already did.
    fn claim(&mut self, server: &str, id: &str) -> bool {
        let key = format!("{server}\0{id}");
        if self.asked.contains(&key) {
            return false;
        }
        if self.asked.len() >= ASKED {
            self.asked.pop_front();
        }
        self.asked.push_back(key);
        true
    }
}

impl Notifier {
    /// Whether this window posts to the system's notifications.
    pub fn shows(&self) -> bool {
        matches!(self.state, SinkState::Ready(_))
    }

    /// Replaces the notification sink directly (for custom platform sinks or harnesses).
    pub fn set_sink(&mut self, sink: Option<Box<dyn Sink>>) {
        step_sink(&mut self.state, SinkEvent::Install(sink));
    }

    /// A click on a notification, from any thread: it waits for its window's poll.
    fn click(click: rune_notify::Click) {
        CLICKS.lock().unwrap_or_else(|e| e.into_inner()).push(click);
        for wake in WAKERS.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            wake();
        }
    }

    fn post(&mut self, note: Note, server: &str, host_focused: bool) {
        let SinkState::Ready(sink) = &mut self.state else {
            return;
        };
        if host_focused || sink.focused() {
            return;
        }
        COORDINATOR.with_borrow_mut(|c| {
            // The same id replaces its toast in place: it isn't counted twice.
            c.shown.retain(|(id, _)| *id != note.id);
            if c.shown.len() >= CAP
                && let Some((oldest, _)) = c.shown.pop_front()
            {
                sink.remove(&oldest);
            }
            sink.post(&note, server);
            c.shown.push_back((note.id, server.to_owned()));
        });
    }

    /// Takes notification `id` out of the system's list, if this run posted it.
    fn take_out(&mut self, id: &str) {
        let SinkState::Ready(sink) = &mut self.state else {
            return;
        };
        COORDINATOR.with_borrow_mut(|c| {
            let before = c.shown.len() + c.ext_shown.len();
            c.shown.retain(|(shown, _)| shown != id);
            c.ext_shown.retain(|(shown, ..)| shown != id);
            if c.shown.len() + c.ext_shown.len() != before {
                sink.remove(id);
            }
        });
    }
}

/// Takes every browser-extension toast out of the system's list (Settings > Notifications turned
/// off, or the engine that posted them is gone), answering `(extension id, notification id)` of each.
/// A closed window's extension toasts go too: no engine is left to hear their clicks.
pub fn clear_all_extensions(host: &mut dyn NotificationsHost) -> Vec<(String, String)> {
    if COORDINATOR.with_borrow(|c| c.ext_shown.is_empty()) {
        return Vec::new();
    }
    let window = host.window_id().to_owned();
    let gone: Vec<String> = COORDINATOR.with_borrow(|c| {
        c.ext_shown
            .iter()
            .map(|(.., owner)| owner)
            .filter(|owner| **owner != window && !host.window_exists(owner))
            .cloned()
            .collect()
    });
    let SinkState::Ready(sink) = &mut host.notifier_mut().state else {
        return Vec::new();
    };
    COORDINATOR.with_borrow_mut(|c| {
        let mut cleared = Vec::new();
        c.ext_shown.retain(|(os_id, ext, notif, owner)| {
            if *owner != window && !gone.contains(owner) {
                return true;
            }
            sink.remove(os_id);
            if *owner == window {
                cleared.push((ext.clone(), notif.clone()));
            }
            false
        });
        cleared
    })
}

/// The extension toasts the cap pushed out that this window's engine posted, to tell it they
/// closed (`notifications.onClosed`, and out of its `getAll`). A gone window's are dropped.
pub fn take_extension_evicted(host: &dyn NotificationsHost) -> Vec<(String, String)> {
    let window = host.window_id();
    COORDINATOR.with_borrow_mut(|c| {
        let mut mine = Vec::new();
        c.ext_evicted.retain(|(owner, ext, notif)| {
            if owner == window {
                mine.push((ext.clone(), notif.clone()));
                return false;
            }
            host.window_exists(owner)
        });
        mine
    })
}

/// Text an extension supplies, without the bidi controls that could reorder it to pass for the
/// app's or another extension's words (`U+061C`, `U+200E`/`F`, `U+202A`-`U+202E`, `U+2066`-`U+2069`).
pub fn strip_bidi(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'))
        .collect()
}

/// Request `id` was answered (here, in another window, from its toast, or the server settled
/// it): its notification goes, as the dock's does.
pub fn answered(host: &mut dyn NotificationsHost, id: &str) {
    host.notifier_mut().take_out(&format!("req-{id}"));
}

/// A session's run notification goes once the session has been looked at.
pub fn seen(host: &mut dyn NotificationsHost, session: &str) {
    host.notifier_mut().take_out(&format!("run-{session}"));
}

/// The app quits: the requests of its own server and extension toasts end with it, so their notifications go.
pub fn quitting(host: &mut dyn NotificationsHost) {
    let own_base = host.own_server_base();
    let SinkState::Ready(sink) = &mut host.notifier_mut().state else {
        return;
    };
    let own = ServerKey::sidecar();
    COORDINATOR.with_borrow_mut(|c| {
        c.shown.retain(|(id, server)| {
            let ends = id.starts_with("req-") && click_server(own_base.as_deref(), server) == own;
            if ends {
                sink.remove(id);
            }
            !ends
        });
        for (id, ..) in c.ext_shown.drain(..) {
            sink.remove(&id);
        }
    });
}

/// Whether Settings > Notifications has at least one notification kind enabled AND the OS sink can post.
pub fn extension_notifications_allowed(host: &dyn NotificationsHost) -> bool {
    let prefs = host.notification_prefs();
    host.notifier().shows() && (prefs.agent || prefs.permissions || prefs.questions || prefs.errors)
}

/// Posts a browser-extension OS notification through the app's native notification sink
/// (`extension-api/notifications.ts:24-50`), respecting Settings > Notifications and the
/// extensions' own cap (`EXT_CAP`, apart from OpenCode's toasts; the oldest extension toast goes,
/// for its window to report closed: [`take_extension_evicted`]). Answers whether it was shown.
pub fn post_extension(
    host: &mut dyn NotificationsHost,
    os_id: &str,
    title: &str,
    body: &str,
    ext_id: &str,
    notif_id: &str,
) -> bool {
    if !extension_notifications_allowed(host) {
        return false;
    }
    let window = host.window_id().to_owned();
    let SinkState::Ready(sink) = &mut host.notifier_mut().state else {
        return false;
    };
    let info = [
        ("kind", "extension"),
        ("ext_id", ext_id),
        ("ext_notif_id", notif_id),
        ("window", window.as_str()),
    ];
    let (title, body) = (strip_bidi(title), strip_bidi(body));
    COORDINATOR.with_borrow_mut(|c| {
        c.ext_shown.retain(|(id, ..)| id != os_id);
        while c.ext_shown.len() >= EXT_CAP
            && let Some((oldest, ext, notif, owner)) = c.ext_shown.pop_front()
        {
            sink.remove(&oldest);
            c.ext_evicted.push((owner, ext, notif));
        }
        let posted = sink.post_raw(os_id, &title, &body, &info);
        if posted {
            c.ext_shown.push_back((os_id.to_owned(), ext_id.to_owned(), notif_id.to_owned(), window.clone()));
        }
        posted
    })
}

/// Removes a browser-extension OS notification posted under `os_id`.
pub fn clear_extension(host: &mut dyn NotificationsHost, os_id: &str) {
    host.notifier_mut().take_out(os_id);
}

/// What `ty` posts, if anything: the kind, its title, the session a click opens and the body.
fn decide(host: &dyn NotificationsHost, ty: &str, data: &Value, sid: &str) -> Option<(Kind, &'static str, String, String)> {
    // A muted session, or one under a muted root, posts nothing (`notification.tsx`'s `muted`).
    if host.is_session_silenced(sid) {
        return None;
    }
    let prefs = host.notification_prefs();
    match ty {
        "session.execution.succeeded" if prefs.agent => {
            if host.has_running_child(sid) || !host.is_root_session(sid) {
                return None;
            }
            let title = host.tab_title(sid)?;
            Some((Kind::Agent, Kind::Agent.title(), sid.to_owned(), if title.is_empty() { sid.to_owned() } else { title }))
        }
        "session.execution.failed" if prefs.errors => {
            if !host.is_root_session(sid) {
                return None;
            }
            let title = host.tab_title(sid)?;
            let body = if title.is_empty() {
                opencode_i18n::t!("notification.session.error.fallbackDescription").to_owned()
            } else {
                title
            };
            Some((Kind::Error, Kind::Error.title(), sid.to_owned(), body))
        }
        "permission.asked" | "form.created" => {
            let (kind, title, asker) = request(ty, data, sid)?;
            if host.is_session_silenced(&asker) || !prefs.get(kind) || (ty == "permission.asked" && host.auto_accepts_permission(&asker)) {
                return None;
            }
            // The asking session's own tab, else its root's: a subagent's request waits there.
            let target = if host.tab_title(&asker).is_some() {
                asker.clone()
            } else {
                host.root_session_id(&asker).to_owned()
            };
            host.tab_title(&target)?;
            let body = host.session_title(&asker).filter(|t| !t.is_empty()).unwrap_or(asker);
            Some((kind, title, target, body))
        }
        _ => None,
    }
}

/// A request's kind, title and asking session (Desktop's `handleRequest`): a permission, or a
/// form that is a question, an MCP server's elicitation or a web search provider's permission.
fn request(ty: &str, data: &Value, sid: &str) -> Option<(Kind, &'static str, String)> {
    let form = if data["form"].is_object() { &data["form"] } else { data };
    let session = form["sessionID"].as_str().or(data["sessionID"].as_str()).unwrap_or(sid).to_owned();
    if ty == "permission.asked" {
        return Some((Kind::Permission, Kind::Permission.title(), session));
    }
    match form["metadata"]["kind"].as_str()? {
        "question" => Some((Kind::Question, Kind::Question.title(), session)),
        "mcp-elicitation" => Some((Kind::Question, opencode_i18n::t!("notification.elicitation.title"), session)),
        "websearch.provider" => Some((Kind::Permission, Kind::Permission.title(), session)),
        _ => None,
    }
}

/// Posts the notification event `(ty, data)` for session `sid` calls for, if any: each window with
/// the session's tab decides, and the first to decide posts it, once for the event.
///
/// `claim` receives whether this window decided to post and returns `true` when this window won
/// the per-event claim across windows (e.g. `StreamEvent::take(&e.notice, want)`).
pub fn notify_event(host: &mut dyn NotificationsHost, ty: &str, data: &Value, sid: &str, claim: &mut dyn FnMut(bool) -> bool) -> bool {
    if !host.notifier().shows() {
        return false;
    }
    if matches!(ty, "permission.asked" | "form.created") {
        let id = if data["form"].is_object() { &data["form"]["id"] } else { &data["id"] };
        let id = id.as_str().unwrap_or("");
        let server = host.active_server_key().as_str().to_owned();
        if id.is_empty() || !COORDINATOR.with_borrow_mut(|c| c.claim(&server, id)) {
            return false;
        }
    }
    let decided = decide(host, ty, data, sid);
    if !claim(decided.is_some()) {
        return false;
    }
    let Some((kind, title, session, body)) = decided else {
        return false;
    };
    // A request's toast says what it asks and carries its answers; a run's is per session.
    let (id, body, ask) = match actions::ask(ty, data) {
        Some(ask) => (format!("req-{}", ask.request()), actions::body(&body, &ask), Some(ask)),
        None => (format!("run-{session}"), body, None),
    };
    // The click opens the session on its own server (`openNotificationSession`).
    let server = host.active_server_key().as_str().to_owned();
    let window = host.window_id().to_owned();
    let focused = host.is_window_focused();
    let note = Note { id, kind, title, body, session, ask, window };
    host.notifier_mut().post(note, &server, focused);
    true
}

/// Delivers a plugin's notification (`Notifications:Notify`): skipped when `session` is muted
/// (itself or through its root), otherwise handed to [`NotificationsHost::show_plugin_notification`].
/// Returns `true` when shown.
pub fn notify_plugin(host: &mut dyn NotificationsHost, title: String, body: String, session: Option<&str>) -> bool {
    if let Some(id) = session
        && host.is_session_silenced(id)
    {
        return false;
    }
    host.show_plugin_notification(title, body, session);
    true
}

/// Answers an actionable toast click (`permission` decision or `question` reply) against `host`.
pub fn on_action(host: &mut dyn NotificationsHost, click: &mut rune_notify::Click) -> actions::Outcome {
    actions::act(host, click)
}

/// Opens clicked notifications' sessions, and connects the system the first time a live window
/// polls.
pub fn poll(host: &mut dyn NotificationsHost, tx: &mut Tx) -> bool {
    if matches!(host.notifier().state, SinkState::Uninitialized) && host.persists() {
        let available = rune_notify::available();
        if matches!(step_sink(&mut host.notifier_mut().state, SinkEvent::Probe { available }), SinkEffect::ConnectSystem) {
            connect_system(host);
        }
    }
    let mut opened = false;
    for mut click in actions::take(host) {
        if click.get("kind") == Some("extension") {
            let ext_id = click.get("ext_id").unwrap_or("").to_owned();
            let notif_id = click.get("ext_notif_id").unwrap_or("").to_owned();
            if !ext_id.is_empty() && !notif_id.is_empty() {
                if host.is_live() {
                    host.activate_window(click.activation_token());
                }
                host.notifier_mut().take_out(&click.id);
                host.on_extension_notification_clicked(&ext_id, &notif_id);
                opened = true;
            }
            continue;
        }
        // A button answers where the app is, without bringing it forward.
        match on_action(host, &mut click) {
            actions::Outcome::Answered => {
                opened = true;
                continue;
            }
            actions::Outcome::Wait => {
                CLICKS.lock().unwrap_or_else(|e| e.into_inner()).push(click);
                continue;
            }
            actions::Outcome::Open => {}
        }
        let (session, from) = (click.get("session").unwrap_or(""), click.get("server").unwrap_or(""));
        if session.is_empty() {
            continue;
        }
        if host.is_live() {
            host.activate_window(click.activation_token());
        }
        let own_base = host.own_server_base();
        let server = click_server(own_base.as_deref(), from);
        host.open_session_from_notification(&server, session, tx);
        opened = true;
    }
    opened
}

/// The server a click's notification came from: its key, or, from a build that named servers by
/// address, the app's own server's address.
fn click_server(own_base: Option<&str>, from: &str) -> ServerKey {
    if from.is_empty() || own_base == Some(from) {
        return ServerKey::sidecar();
    }
    ServerKey::new(from)
}

fn connect_system(host: &dyn NotificationsHost) {
    if let Some(waker) = host.waker() {
        WAKERS.lock().unwrap_or_else(|e| e.into_inner()).push(waker);
    }
    rune_notify::on_click(Notifier::click);
}

struct System;

impl Sink for System {
    fn focused(&self) -> bool {
        rune_notify::app_focused()
    }

    fn post(&mut self, note: &Note, server: &str) {
        let n = rune_notify::Notification::new(&note.id, note.title)
            .body(&note.body)
            .info("session", &note.session)
            .info("server", server)
            .info("window", &note.window);
        rune_notify::post(&note.ask.as_ref().map_or(n.clone(), |ask| actions::buttons(n, ask)));
    }

    fn remove(&mut self, id: &str) {
        rune_notify::remove(&[id]);
    }

    fn post_raw(&mut self, id: &str, title: &str, body: &str, info: &[(&str, &str)]) -> bool {
        let mut n = rune_notify::Notification::new(id, title).body(body);
        for &(k, v) in info {
            n = n.info(k, v);
        }
        rune_notify::post(&n)
    }
}

/// A notification's link, opened with the app (`deep_links`): its session opens as a click's
/// does. On Windows every click comes this way, also the one that starts the app after it quit.
pub fn clicked(click: &rune_notify::Click) {
    Notifier::click(click.clone());
}

/// Gives a windowed run the app's identity before its window opens, writing nothing: on Windows
/// the `AppUserModelID` its taskbar button groups under (the Start menu shortcut carries it too)
/// and its toasts carry. The setup registered the toasts' name and icon and the app's URL scheme
/// for the install (`win_install::register`); [`settle_identity`] finds out after the first frame
/// whether this is that install. macOS takes all of it from the bundle.
#[cfg(target_os = "windows")]
pub fn identify(product: &win_install::Product, headless: bool) {
    if headless {
        return;
    }
    if let Err(e) = rune_notify::adopt(&rune_notify::Identity::new(product.id(), product.name())) {
        eprintln!("notifications: {e}");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn identify(product: &win_install::Product, headless: bool) {
    let _ = (product, headless);
}

/// After the first frame, off the UI thread: the registered install's toasts open the app's
/// scheme, so a toast left in the notification center starts it again; another copy (a zip, a
/// build folder, a harness run) reads only, and its clicks reach it while it runs. Only the
/// registered install writes, and only its own scheme when it went missing (`repair_scheme`). Then the toast activator is served (`rune_notify::serve_activations`).
#[cfg(target_os = "windows")]
pub fn settle_identity(product: &win_install::Product, headless: bool) {
    static SETTLED: std::sync::Once = std::sync::Once::new();
    if headless {
        return;
    }
    let product = product.clone();
    SETTLED.call_once(move || settle_once(product));
}

#[cfg(target_os = "windows")]
fn settle_once(product: win_install::Product) {
    let spawned = std::thread::Builder::new().name("opencode-identity".into()).spawn(move || {
        let started = rune::time::Instant::now();
        let Some(root) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(std::path::Path::to_path_buf)) else {
            return;
        };
        let found = win_install::settle_identity(&product, &root);
        // The registered install keeps its scheme pointed at itself; another copy, the exe renamed
        // or a scratch `RUNE_DIRS_ROOT` run writes nothing (`repair_scheme` checks each).
        if let Some(done) = win_install::repair_scheme(&product, &root) {
            eprintln!("notifications: {done}");
        }
        if found == win_install::Identity::Installed
            && let Err(e) = rune_notify::adopt(&rune_notify::Identity::new(product.id(), product.name()).activation(product.scheme()))
        {
            eprintln!("notifications: {e}");
        }
        // A toast's button reaches this process straight away where the install registered the
        // activator; elsewhere the registration is inert and buttons act through the toast's event.
        if let Err(e) = rune_notify::serve_activations(&product.toast_activator()) {
            eprintln!("notifications: toast activator: {e}");
        }
        eprintln!("notifications: {} ({found:?}), settled in {} µs off the UI thread", product.id(), started.elapsed().as_micros());
    });
    if let Err(e) = spawned {
        eprintln!("notifications: {e}");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn settle_identity(product: &win_install::Product, headless: bool) {
    let _ = (product, headless);
}

/// A launch COM made for a toast's button after the app quit (`LocalServer32 ... --toast-activated`,
/// which the install registers with the activator; COM adds `-Embedding`): it takes the
/// activation (a few ms after the class is up) and starts as the app does, its first window
/// answering the click. The app's own server ended with it, so its requests did too; a request
/// the window no longer finds opens its session instead.
pub fn take_activation(args: &[String], product: &win_install::Product) {
    if !args.iter().any(|a| a == "--toast-activated" || a.eq_ignore_ascii_case("-Embedding")) {
        return;
    }
    if let Some(click) = rune_notify::wait_activation(&product.toast_activator(), std::time::Duration::from_secs(5)) {
        Notifier::click(click);
    }
}
