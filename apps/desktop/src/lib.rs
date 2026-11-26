//! OpenCode Desktop in Rune: OpenCode's home and session pages in Rune, live against the local OpenCode server or
//! from a synthetic fixture. Live, it sends prompts, answers permission requests and questions,
//! stops runs and starts sessions (`--read-only` turns all of that off).

/// mimalloc commits each medium and large page whole when a thread heap takes it; on demand it
/// commits a page only as blocks are carved from it. Every thread that allocates holds such pages
/// for its size classes, mostly untouched: on Home this is 60-75 MB of commit charge that is never
/// written (private bytes ~205 MB to ~145 MB, the private working set unchanged).
#[cfg(not(target_arch = "wasm32"))]
#[allow(unsafe_code)]
fn commit_pages_on_demand() {
    unsafe extern "C" {
        fn mi_version() -> std::ffi::c_int;
        fn mi_option_set(option: std::ffi::c_int, value: std::ffi::c_long);
    }
    // `mi_option_page_commit_on_demand` is 40 in mimalloc 3.3's `mi_option_t` (mimalloc.h); a
    // later mimalloc may number it otherwise, so it's only set on the version it was read from.
    // SAFETY: plain calls into the linked mimalloc; an option set after start applies to the
    // pages taken from then on.
    unsafe {
        if mi_version() / 100 == 303 {
            mi_option_set(40, 1);
        }
    }
}

/// `App::rest`: mimalloc collects this thread's heap and purges every free page now; on Windows
/// the process heap compacts too.
#[cfg(not(target_arch = "wasm32"))]
#[allow(unsafe_code)]
pub(crate) fn purge_free_pages() {
    unsafe extern "C" {
        fn mi_collect(force: bool);
    }
    // SAFETY: a plain call into the linked mimalloc, on the UI thread.
    unsafe { mi_collect(true) }
    // The process heap (UI Automation, COM, the D3D runtime and driver) keeps freed pages
    // committed; compacting it gives its large free blocks back.
    #[cfg(windows)]
    // SAFETY: the process heap lives as long as the process; HeapCompact takes its lock.
    unsafe {
        windows_sys::Win32::System::Memory::HeapCompact(windows_sys::Win32::System::Memory::GetProcessHeap(), 0);
    }
}

// A browser's requests speak ureq's API too (ureq-web), so the request code is the same there.
#[cfg(target_arch = "wasm32")]
extern crate ureq_web as ureq;

pub use session_view::{
    agent_start, disclosure, fold, jump, line_reveal, odometer, palette, question_row,
    subagent_card as agent_rows_view, themes, turns,
};

mod compaction {
    pub use session_view::compaction::*;
}
mod copied {
    pub use session_view::copied::*;
}
mod turn_errors {
    pub use session_view::turn_errors::*;
}
mod working {
    pub use session_view::working::*;
}

#[allow(unused_imports)]
pub(crate) use opencode_platform::{askpass, telemetry, wall, wall_ms, web_fetch};

mod app;
mod dialogs;
mod exports;
mod kernel;
mod plugin_host;
mod session;
mod shell;
mod stories;
mod surfaces;
mod tick;
mod workspace;

use rune::AppSpec;

pub use app::{App, Source, TabStatus};
pub(crate) use app::{FIXTURE, arg, attended, auto_focus_composer, flag, harness_writes, headless, realtime};
pub use shell::nav::route::Route;
pub use shell::panels;
pub use surfaces::code_view;
pub(crate) use stories::scenes::open_scene;
pub(crate) use tick::{poll, setup, visibility};
#[allow(unused_imports)]
pub(crate) use exports::*;
#[cfg(not(target_arch = "wasm32"))]
pub use workspace::lifecycle::launch::last_exit::{OomGuard, alloc_failed};

/// Configures extracted feature crates once per process (native and wasm32).
pub(crate) fn init_features() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        fn doc_lookup(key: &str) -> opencode_platform::DocLookup {
            match storage::doc(key) {
                opencode_storage::Lookup::Ready(v) => opencode_platform::DocLookup::Ready(v),
                opencode_storage::Lookup::Pending => opencode_platform::DocLookup::Pending,
            }
        }
        opencode_platform::storage::configure(opencode_platform::StoragePort {
            is_ready: storage::is_ready,
            attended,
            read_json: storage::json,
            write_json: storage::set_json,
            delete: storage::delete,
            doc: doc_lookup,
            load_docs: storage::load_docs,
            set_doc: storage::set_doc,
            hand_off_doc: storage::hand_off_doc,
        });
        opencode_storage::configure_runner(|op| storage::with_storage(op));
        keymap::init();
        rune_mcp::init();
        updater::init();
        settings::init();
        #[cfg(feature = "office")]
        office::init();
    });
}

/// The app's fonts. Desktop sets every IBM Plex Mono weight below 600 in its Text cut and the rest
/// in its Bold cut (`index.css`), which a Markdown heading in a diff takes, and italic Plex in its
/// Text Italic cut (inline code in emphasis). JetBrains Mono is desktop's terminal face
/// (`terminalDefault`), Nerd Font glyphs included for prompts.
#[cfg(all(not(target_arch = "wasm32"), not(bundle_assets)))]
const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/InterVariable.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Text.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Bold.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-TextItalic.ttf"),
    include_bytes!("../assets/fonts/CommitMono-400-Regular.otf"),
    include_bytes!("../assets/fonts/CommitMono-700-Regular.otf"),
    include_bytes!("../assets/fonts/CommitMono-400-Italic.otf"),
    include_bytes!("../assets/fonts/CommitMono-700-Italic.otf"),
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Bold.ttf"),
];
/// A packaged macOS app's fonts are files in its bundle (`bundle`), mapped: 7.1 MiB the system does
/// not read before the exe's first run. A font whose file is missing is left out.
#[cfg(bundle_assets)]
const FONT_FILES: &[&str] = &[
    "InterVariable.ttf",
    "IBMPlexMono-Text.ttf",
    "IBMPlexMono-Bold.ttf",
    "IBMPlexMono-TextItalic.ttf",
    "CommitMono-400-Regular.otf",
    "CommitMono-700-Regular.otf",
    "CommitMono-400-Italic.otf",
    "CommitMono-700-Italic.otf",
    "JetBrainsMonoNerdFontMono-Regular.ttf",
    "JetBrainsMonoNerdFontMono-Bold.ttf",
];
#[cfg(bundle_assets)]
fn fonts() -> &'static [&'static [u8]] {
    static MAPPED: std::sync::OnceLock<Vec<&'static [u8]>> = std::sync::OnceLock::new();
    MAPPED.get_or_init(|| FONT_FILES.iter().filter_map(|file| crate::bundle::map(&format!("fonts/{file}"))).collect())
}
#[cfg(not(bundle_assets))]
fn fonts() -> &'static [&'static [u8]] {
    FONTS
}
/// In a browser, only the fonts the default appearance draws in: the page fetches Plex's italic and
/// Commit Mono once its first frame is up (`LATE_FONTS`), and JetBrains Mono, the terminal's (4.8 MB), when a
/// terminal first attaches (`TERMINAL_FONTS`).
#[cfg(target_arch = "wasm32")]
const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/InterVariable.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Text.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Bold.ttf"),
];
/// The fonts a browser fetches after its first frame, which the web build serves from `fonts/`
/// beside the page (`scripts/web.ps1`).
#[cfg(target_arch = "wasm32")]
pub(crate) const LATE_FONTS: &[&str] = &[
    "fonts/IBMPlexMono-TextItalic.ttf",
    "fonts/CommitMono-400-Regular.otf",
    "fonts/CommitMono-700-Regular.otf",
    "fonts/CommitMono-400-Italic.otf",
    "fonts/CommitMono-700-Italic.otf",
];
/// The terminal's face, which a page fetches when its first terminal attaches (`terminal::fetch_terminal_font`).
#[cfg(target_arch = "wasm32")]
pub(crate) const TERMINAL_FONTS: &[&str] = &["fonts/JetBrainsMonoNerdFontMono-Regular.ttf", "fonts/JetBrainsMonoNerdFontMono-Bold.ttf"];

/// The app's spec: `init` makes its state (`App::new`, or `App::empty`).
pub(crate) fn spec(init: fn() -> App) -> AppSpec<App> {
    init_features();
    rune::text::set_face_weights(appearance::FACE_WEIGHTS);
    // Desktop's base.css sets `text-wrap-style: pretty` on every element.
    rune::text::set_pretty_wrap(true);
    AppSpec {
        scenes: stories::scenes(),
        fonts: fonts(),
        families: (appearance::UI_FONTS[0].family, appearance::CODE_FONTS[0].family),
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(keys::on_key),
        ..AppSpec::new(launch::window_title(), (1280.0, 800.0), init, view::app)
    }
}

/// The window/CSD theme the app paints in: its saved appearance scheme, or `--scheme`.
pub(crate) fn window_color_scheme() -> rune::ColorScheme {
    let scheme = crate::app::arg("--scheme")
        .map(|value| crate::settings::Scheme::parse(&value))
        .unwrap_or_else(|| {
            if crate::app::arg("--fixture").is_some() {
                crate::settings::Scheme::default()
            } else {
                crate::settings::load().scheme
            }
        });
    color_scheme(scheme)
}

/// Maps the app's appearance scheme to Rune's window/CSD color scheme.
pub(crate) fn color_scheme(scheme: crate::settings::Scheme) -> rune::ColorScheme {
    match scheme {
        crate::settings::Scheme::Dark => rune::ColorScheme::Dark,
        crate::settings::Scheme::Light => rune::ColorScheme::Light,
        crate::settings::Scheme::System => rune::ColorScheme::System,
    }
}

/// The app as it runs: its history, its storybook (⌘⇧D), its window chrome, and Rune's native
/// Metal backend (`RUNE_GPU_BACKEND=wgpu` draws with wgpu again, no rebuild).
pub(crate) fn rune_app(spec: AppSpec<App>) -> rune::App<App> {
    // The CSD frame around the Linux window follows Rune's color scheme; match it to the
    // appearance scheme the app itself paints in, instead of the system's.
    let color_scheme = window_color_scheme();
    let app = rune::App::new(spec);
    // Office pages draw their Vello scenes into effect slots, and the new-session Easter egg its
    // pins and lodger; a window without one draws as Rune's default frame does.
    let app = app.compositor(compositor::AppCompositor::default());
    menubar::install(
        app
            .color_scheme(color_scheme)
            .gpu_backend(rune::GpuBackend::Metal)
            .playground(stories::playground())
            .on_ime(|s, ime, _| s.panels_ime(ime))
            .on_open_url(deep_links::open)
            // The page's server's copy of the session data moves into the window whose turn it is.
            .turn_scope(servers::runtime::enter, servers::runtime::leave)
            .on_modifiers(view::home_modifiers)
            .on_visibility(visibility)
            // Tab reaches every control on a Mac whatever its Keyboard navigation setting says,
            // as official's page does in Chromium: with the setting off (macOS's default), shipped
            // 2.2.7 Tabs through Home, Search sessions, New session, a reply's buttons and the
            // prompt, where AppKit's rule left only the prompt and the tab strip.
            .tab_visits_controls(true)
            // The timeline's jump-to-latest button and the composer's drop zone blur what is behind
            // them: the app's only effect. Rune compiles that pipeline on a low-priority thread once
            // the window rests (opt-in since rune#2006, per effect since rune#2011), so the button's
            // first showing in a scroll does not wait for the GPU driver (about 80 to 100 ms on a
            // cold shader cache). Nothing else is named: no layer blur, no blend is compiled ahead.
            .prewarm_effects(rune::PrewarmEffects::BACKDROP_BLUR)
            .titlebar(rune::Titlebar::new(view::TITLEBAR_H))
            .chrome(rune::Chrome {
                full_size_content: true,
                traffic_lights: Some((20.0, 25.0)),
                drag_regions: true,
            }),
    )
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    /// The launch's frame diagnostics, for `quit_last` to drain.
    static QUIT_DIAGNOSTICS: std::cell::RefCell<Option<diagnostics::Diagnostics>> = const { std::cell::RefCell::new(None) };
}

/// The windows' last step on the way out (`rune::fast_exit`), on the UI thread once every window's
/// quit hook ran: the database's last write, the diagnostics' queue, an install-on-quit's helper
/// (it must exist before this process ends), the launch trace, then a training build's profile.
/// Rune's fast exit raises every thread to normal priority before the process ends itself, so an
/// idle one can't hold it.
#[cfg(not(target_arch = "wasm32"))]
fn quit_last() {
    // Release any threads waiting on local server startup without stopping the detached daemon.
    servers::sidecar::dispose();
    // First, so its threads have left their writes by the time the process ends.
    browser_pane::installer::quit();
    rune::marks::mark("browser.engine.stopped");
    // Landed and closed: the file's lock goes before the update helper takes over.
    storage::close();
    rune::marks::mark("storage.flushed");
    // Nothing of the app's is left to lose: whatever the rest waits on, the process ends within
    // 2 s, saying what it waited for. A training build (`apps/desktop/scripts/pgo.ps1`) writes ~50 MB of counts
    // last; on a loaded machine that outlasted 2 s and left truncated, unmergeable files. A macOS
    // install-on-quit verifies and swaps the staged `.app` before exit.
    let deadline = if cfg!(pgo_generate) {
        120
    } else if updater::installs_on_quit() {
        60
    } else {
        2
    };
    rune::exit_deadline(std::time::Duration::from_secs(deadline));
    // A launch handed on while the app quit claims it now, unless an update replaces it.
    if !updater::installs_on_quit() {
        rune::release_instance();
    }
    QUIT_DIAGNOSTICS.with(|d| if let Some(d) = d.borrow_mut().take() { d.shutdown() });
    rune::marks::mark("diagnostics.stopped");
    // The engine's download had the storage's flush to stop in. What it writes is closed before
    // the process ends: left open, the process's own teardown closes it, with Defender's scan of a
    // half-written DLL inside that close (seconds before an update's helper sees the app gone).
    rune::marks::mark(if browser_pane::installer::quit_wait() { "browser.engine.closed" } else { "browser.engine.open" });
    updater::finish_quit();
    rune::marks::mark("quit.update_handed_off");
    launch::trace::finish(windows::opened());
    launch::write_profile();
}

/// Runs the native desktop application entrypoint.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_native() {
    // Before anything is started: what this process starts doesn't inherit the installer's layer.
    #[cfg(windows)]
    launch::compat_layer::shed();
    // And none of them holds the install folder as its working folder (an update renames it).
    #[cfg(windows)]
    launch::leave_install_folder();
    // The setup's run of a new install: the GPU's cache is made and nothing else starts.
    #[cfg(windows)]
    launch::warm_gpu_if_asked();
    commit_pages_on_demand();
    #[cfg(feature = "alloc-sites")]
    plugin_host::memory::register_reports();
    // A debug build COM starts for a proof's scratch toast activator (`LocalServer32`, whose
    // command line is fixed in the registry and whose environment is the user's) takes its
    // scratch root from the command line: it never runs without one. Release builds ignore it.
    #[cfg(debug_assertions)]
    if let Some(root) = std::env::args().find_map(|a| a.strip_prefix("--dirs-root=").map(str::to_owned)) {
        // SAFETY: the first thing `main` does, before any other thread exists.
        #[allow(unsafe_code)]
        unsafe { std::env::set_var("RUNE_DIRS_ROOT", root) };
    }
    launch::trace::begin();
    telemetry::launched();
    launch::attach_console();
    askpass::run_if_asked();
    init_features();
    #[cfg(windows)]
    updater::apply::run_if_asked();
    #[cfg(windows)]
    install::run_if_asked();
    launch::print_version_if_asked();
    #[cfg(feature = "office")]
    office::check_if_asked();
    launch::unwind_check_if_asked();
    if plugin_host::types::from_args() {
        return;
    }
    if genui::skill::maybe_cli() {
        return;
    }
    // The exe's own pages for this launch, asked for at once on a thread instead of one disk
    // read per page fault (a new build's first start, or memory pressure since the last one).
    #[cfg(any(windows, target_os = "linux"))]
    launch::image_pages::ask();
    // The app's state, read on its own thread beside the instance claim (`storage::start`). A
    // launch while the app runs hands its links (`deep_links`) to it and ends here.
    storage::start();
    // Only an argument on the app's own scheme is a link (`deep_links`): the value of
    // `--server http://…` isn't one, and a second launch with it still brings the window forward.
    rune::url_schemes([launch::product().scheme()]);
    if !headless() && !rune::single_instance(&launch::instance_id()) {
        return launch::trace::finish(0);
    }
    storage::claimed();
    // Start/discover the local OpenCode background service (`servers::sidecar`, official
    // `BackgroundService`) and ask for the first reads now (`launch::head_start`).
    if !headless() {
        servers::sidecar::start();
        launch::head_start();
    }
    launch::last_exit::init_hooks();
    notifications::take_activation(rune::extra_args());
    glyph_masks::start();
    // The first view's theme tokens (OpenCode's kit and the default theme's palette, both
    // appearances from one parse), built while the window is made rather than in its first frame.
    std::thread::Builder::new()
        .name("theme-warm".into())
        .spawn(|| {
            for dark in [true, false] {
                session_view::palette::kit(session_view::palette::pal(dark));
            }
        })
        .ok();
    opencode_kit::client::set_download_dir(dirs::downloads());
    // With `HARNESS_OPEN_EXTERNAL_LOG` set, a debug build or a headless run records every external
    // open instead of making it; a window does so only in a harness run rooted in its own folders,
    // to a log inside that root.
    harness_log::configure(
        cfg!(debug_assertions) || headless(),
        harness_writes()
            .then(|| std::env::var_os("RUNE_DIRS_ROOT"))
            .flatten()
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from)
            .as_deref(),
    );
    // Where the browser engine lives follows this app's data root (no I/O until a page opens).
    #[cfg(any(target_os = "macos", windows))]
    if let Some(engine) = dirs::engine() {
        browser_pane::installer::set_home(engine);
    }
    telemetry::startup("launch.platform", rune_mac::launch);
    telemetry::startup("launch.notifications", notifications::identify);
    telemetry::startup("launch.gpu", rune::prewarm_gpu);
    // Windows: no GPU before the first frame. Rune paints it on the CPU and makes the window's GPU
    // on a below-normal thread once it's on screen, then hands over. Warming the GPU here put its
    // driver loads beside the launch's: the first frame came 8 ms and the first input 15 ms later
    // (p50, visible desktop), for a hand-over ~50 ms sooner that nobody sees.
    telemetry::startup("launch.icon", launch::apply_app_icon);
    let diagnostics = diagnostics::Diagnostics::default();
    if !headless() {
        launch::defer();
        rune::marks::mark("windows.launch");
        // Quitting ends the process right after `quit_last`: no window, GPU device or allocation
        // is torn down (on Windows that was the graphics drivers' detach, hundreds of ms).
        QUIT_DIAGNOSTICS.with(|d| *d.borrow_mut() = Some(diagnostics.clone()));
        rune::fast_exit(quit_last);
        // Every window saved at the last quit, each with its own tabs (`windows`).
        windows::launch(diagnostics);
        quit_last();
        return;
    }
    // The harness's `--realtime` probes launch as a window does: the server's first reads
    // wait for `on_ready`, which only a realtime run calls (after its first frame).
    if harness_writes() && realtime() {
        launch::defer();
    }
    let app = rune_app(AppSpec { families: appearance::launch_families(), ..spec(App::new) });
    let handle = app.handle();
    let mut motion_log = motion_log::Log::default();
    let (encoded, drawn) = (diagnostics.clone(), diagnostics.clone());
    let app = app
        .on_ready(move |_| {
            storage::presented();
            let wake = launch::inbox_waker(handle.clone());
            let browser = launch::browser_waker(handle.clone());
            handle.update_if(move |s, tx| {
                s.launch.inbox_wake = Some(wake);
                s.workspace_panes.browser.host.set_message_waker(browser);
                launch::ready(s, tx)
            });
        })
        .on_frame_encoded(move |f| {
            encoded.encoded(f);
            telemetry::encoded(f);
            files::trace::frame(f);
        })
        .on_frame_drawn(move |rt| {
            drawn.drawn(rt);
            if let Some(frame) = telemetry::last_frame() { telemetry::drawn(rt.state.painted_session(), frame); }
            motion_log.frame(rt);
        })
        .on_close(|s, _| {
            let scheduler = s.kernel.scheduler.clone();
            scheduler.run_close(s);
            s.save_on_close();
        })
        .on_rest(App::rest);
    app.run();
    storage::close();
    diagnostics.shutdown();
    // A headless run's marks too (`--launch-trace`): the harness's realtime probes, such as the
    // browser engine's timing.
    launch::trace::finish(1);
}

/// Narrow host boundary for `apps/web` (`wasm32`): exposes only the bootstrap, poll, lifecycle,
/// storage, and browser-history hooks the browser entrypoint needs without leaking desktop internals.
#[cfg(target_arch = "wasm32")]
pub mod web_api {
    use rune::{AppHandle, AppSpec, Tx, Waker};

    use crate::{App, Route};

    pub const LATE_FONTS: &[&str] = crate::LATE_FONTS;

    pub fn arg(name: &str) -> Option<String> {
        crate::arg(name)
    }

    pub fn init_features() {
        crate::init_features();
    }

    pub fn new_app() -> App {
        App::new()
    }

    pub fn launch_families() -> (&'static str, &'static str) {
        crate::appearance::launch_families()
    }

    pub fn spec(init: fn() -> App) -> AppSpec<App> {
        crate::spec(init)
    }

    pub fn rune_app(spec: AppSpec<App>) -> rune::App<App> {
        crate::rune_app(spec)
    }

    pub fn poll(s: &mut App, now: f64, tx: &mut Tx) -> bool {
        crate::poll(s, now, tx)
    }

    pub fn storage_start() {
        crate::storage::start();
    }

    pub fn storage_presented() {
        crate::storage::presented();
    }

    pub fn storage_flush(reason: opencode_storage::FlushReason, wait: bool) {
        crate::storage::flush(reason, wait);
    }

    pub fn launch_defer() {
        crate::launch::defer();
    }

    pub fn launch_start_first_reads() {
        crate::launch::start_first_reads();
    }

    pub fn launch_inbox_waker(handle: AppHandle<App>) -> Waker {
        crate::launch::inbox_waker(handle)
    }

    pub fn launch_ready(s: &mut App, tx: &mut Tx) -> bool {
        crate::launch::ready(s, tx)
    }

    pub fn view_scale(s: &App) -> f32 {
        crate::windows::view_scale(s)
    }

    pub fn set_window_scale(s: &mut App, scale: f32) {
        s.window.scale = scale;
    }

    pub fn set_inbox_wake(s: &mut App, wake: Waker) {
        s.launch.inbox_wake = Some(wake);
    }

    pub fn dismiss_touch_keyboard(s: &mut App) -> bool {
        let changed = s.composer.focused || s.kernel.slots.get(&crate::suggestions::SUGGEST).search.focused;
        s.composer.focused = false;
        s.kernel.slots.get_mut(&crate::suggestions::SUGGEST).search.focused = false;
        changed
    }

    pub fn terminal_flush() {
        crate::terminal::persist::flush();
    }

    pub fn nav_index(s: &App) -> usize {
        s.nav.index()
    }

    pub fn navigate(s: &mut App, forward: bool, tx: &mut Tx) -> bool {
        s.navigate(forward, tx)
    }

    pub fn active_session_id(s: &App) -> Option<&str> {
        match &*s.route {
            Route::Session { id, .. } => Some(id.as_str()),
            _ => None,
        }
    }

    pub fn settings_open(s: &App) -> bool {
        s.settings_open()
    }
}
