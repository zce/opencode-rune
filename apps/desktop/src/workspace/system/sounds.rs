//! Desktop's sound effects (`shell/notifications/sound.ts`, `@opencode/ui/assets/audio`): the
//! sound Settings › Notifications › Sound effects picks for each event, played when the agent
//! finishes or fails in a session with an open tab, or asks a permission or a question, and as
//! a preview when picked. The clips ship as 22 kHz mono WAV (`assets/sounds`, converted from
//! Desktop's MP3s). Only a window with a person at it plays (`App::persists`). In a browser the
//! clips aren't in the module: the page plays them from `sounds/` beside it (`scripts/web.ps1`).
//! A packaged macOS app plays them from its bundle (`bundle`).


use crate::App;

#[cfg(all(not(target_arch = "wasm32"), not(bundle_assets)))]
macro_rules! clips {
    ($($id:literal),+ $(,)?) => {
        &[$(($id, include_bytes!(concat!("../../../assets/sounds/", $id, ".wav")))),+]
    };
}

#[cfg(any(target_arch = "wasm32", bundle_assets))]
macro_rules! clips {
    ($($id:literal),+ $(,)?) => {
        &[$(($id, &[])),+]
    };
}

static CLIPS: &[(&str, &[u8])] = clips![
    "alert-01", "alert-02", "alert-03", "alert-04", "alert-05", "alert-06", "alert-07", "alert-08", "alert-09", "alert-10",
    "bip-bop-01", "bip-bop-02", "bip-bop-03", "bip-bop-04", "bip-bop-05", "bip-bop-06", "bip-bop-07", "bip-bop-08", "bip-bop-09", "bip-bop-10",
    "staplebops-01", "staplebops-02", "staplebops-03", "staplebops-04", "staplebops-05", "staplebops-06", "staplebops-07",
    "nope-01", "nope-02", "nope-03", "nope-04", "nope-05", "nope-06", "nope-07", "nope-08", "nope-09", "nope-10", "nope-11", "nope-12",
    "yup-01", "yup-02", "yup-03", "yup-04", "yup-05", "yup-06",
];

/// Plays sound `id` now, stopping any sound still playing. Unknown ids play nothing.
pub fn play(id: &str) {
    if crate::headless() {
        return;
    }
    if let Some((_, wav)) = CLIPS.iter().find(|(c, _)| *c == id) {
        platform::play(id, wav);
    }
}

/// The sound event `e` calls for, when its event's sound is on (from `App::react`): each
/// window with the session's tab decides, and the sound plays once for the event.
pub fn event(app: &App, e: &crate::events::StreamEvent, sid: &str) {
    let (ty, data) = (e.ty.as_str(), &e.data);
    if !app.persists() {
        return;
    }
    let sounds = &app.settings.sounds;
    // A tab on the event's server: the window is checked out on it while it reacts.
    let server = crate::servers::runtime::active(app);
    let has_tab = |id: &str| app.tabs.iter().any(|t| t.id == id && t.server == *server);
    let root = |id: &str| app.session(id).is_none_or(|s| s.parent.is_none());
    // A muted session plays nothing, as it posts nothing (`notification.tsx`'s `muted`).
    let asker = data["sessionID"].as_str().unwrap_or(sid);
    if crate::session_menu::preferences::silenced(app, server, sid) || crate::session_menu::preferences::silenced(app, server, asker) {
        return;
    }
    let id = match ty {
        "session.execution.succeeded" if root(sid) && has_tab(sid) => sounds.agent.as_deref(),
        "session.execution.failed" if root(sid) && has_tab(sid) => sounds.errors.as_deref(),
        "permission.asked" if has_tab(app.root_of(data["sessionID"].as_str().unwrap_or(sid))) => sounds.permissions.as_deref(),
        "form.created" if has_tab(app.root_of(data["sessionID"].as_str().unwrap_or(sid))) => sounds.questions.as_deref(),
        _ => None,
    };
    if let Some(id) = id.filter(|_| crate::events::StreamEvent::take(&e.sound, true)) {
        play(id);
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod platform {
    use std::ffi::c_void;

    const SND_ASYNC: u32 = 0x0001;
    const SND_NODEFAULT: u32 = 0x0002;
    const SND_MEMORY: u32 = 0x0004;

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn PlaySoundW(sound: *const u16, module: *mut c_void, flags: u32) -> i32;
    }

    /// `PlaySound` straight from the clip in memory: asynchronous, replacing what plays.
    pub fn play(_: &str, wav: &'static [u8]) {
        // SAFETY: `wav` is a whole WAV file that lives for the program, as SND_MEMORY with
        // SND_ASYNC needs; a null module is allowed with SND_MEMORY.
        unsafe {
            PlaySoundW(wav.as_ptr().cast(), std::ptr::null_mut(), SND_ASYNC | SND_NODEFAULT | SND_MEMORY);
        }
    }
}

/// The page's own audio, from the clip served beside it.
#[cfg(target_arch = "wasm32")]
mod platform {
    use std::cell::RefCell;

    thread_local! {
        /// The clip playing, which the next one stops, as `PlaySound` replaces.
        static PLAYING: RefCell<Option<web_sys::HtmlAudioElement>> = const { RefCell::new(None) };
    }

    pub fn play(id: &str, _: &'static [u8]) {
        let Ok(audio) = web_sys::HtmlAudioElement::new_with_src(&format!("sounds/{id}.wav")) else { return };
        // A page may play sound only after the person has used it; until then the browser refuses,
        // and the refusal is no error of ours.
        if let Ok(playing) = audio.play() {
            wasm_bindgen_futures::spawn_local(async move { let _ = wasm_bindgen_futures::JsFuture::from(playing).await; });
        }
        if let Some(last) = PLAYING.replace(Some(audio)) {
            let _ = last.pause();
        }
    }
}

#[cfg(not(any(target_os = "windows", target_arch = "wasm32")))]
mod platform {
    /// The clip through the system's player: `afplay` on macOS, PulseAudio's `paplay`, PipeWire's
    /// `pw-play`, or ALSA's `aplay` elsewhere. It plays from the app's cache folder, written there
    /// on first use; a packaged macOS app plays the file in its bundle.
    #[cfg(not(bundle_assets))]
    pub fn play(id: &str, wav: &'static [u8]) {
        let Some(dir) = crate::dirs::data().map(|d| d.join("sounds")) else { return };
        let path = dir.join(format!("{id}.wav"));
        std::thread::spawn(move || {
            if !path.exists() {
                std::fs::create_dir_all(&dir).ok();
                if std::fs::write(&path, wav).is_err() {
                    return;
                }
            }
            run(&path);
        });
    }

    #[cfg(bundle_assets)]
    pub fn play(id: &str, _: &'static [u8]) {
        let Some(path) = crate::bundle::path(&format!("sounds/{id}.wav")) else { return };
        std::thread::spawn(move || run(&path));
    }

    fn run(path: &std::path::Path) {
        // PipeWire's own player sits beside PulseAudio's compatibility one: Fedora's Workstation
        // install has `pw-play` but not `paplay`, and ALSA's `aplay` may name another sink.
        let players: &[&str] = if cfg!(target_os = "macos") { &["afplay"] } else { &["paplay", "pw-play", "aplay"] };
        for player in players {
            if std::process::Command::new(player).arg(path).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success()) {
                return;
            }
        }
        eprintln!("sounds: no player could play {}", path.display());
    }
}
