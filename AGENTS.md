# OpenCode (Rune) (`opencode-rune`)

`anomalyco/opencode-rune` houses **OpenCode (Rune)** (`apps/desktop`, `apps/web`, `apps/installer`) and the shared Rust crates (`crates/*`) that any Rune app can embed by git dependency.

## Layout (Progressive Disclosure)

Start at the top and go deeper only when a feature needs lower-level control. See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the complete crate inventory, dependency graph, and "Which file do I touch?" wiring guide.

- **`plugins/`** (Tier 1 — default): `--!strict` Luau plugins targeting `opencode-plugin-spec`, hot-reloaded in place with zero Rust edits.
- **`features/`** (Tier 2): decoupled Rust feature crates (`opencode-feature-*`) with zero cross-feature dependencies and non-generic `&mut dyn *Host` boundaries. Scaffold new plugins or features with `scripts/new-feature.sh` (or `pwsh scripts/new-feature.ps1`).
- **`crates/`** (Tier 3): shell kernel (`opencode-kernel`), bottom-layer UI design-system foundation (`opencode-ui`, zero `session-view` dependency), headless runtime (`opencode-runtime`), platform/storage/search/i18n/plugin-runtime libraries, and embeddable crates (`session-view` built on `opencode-ui`, `opencode-code-view`, `opencode-kit`, `rune-parts`, `terminal`, `browser-pane`, `office-preview`, `project-icon`, `hex-client`, `otlp-lite`). Keep `session-view` free of `opencode-desktop`-only window/tab/sidebar assumptions and `opencode-kit` free of app-specific concepts so other Rune apps can embed them directly.
- **`apps/`** (Tier 4):
  - `apps/desktop` (`opencode-desktop`): shared desktop library (`opencode_desktop`) plus thin native binary (`main.rs`), organized into root entry/wiring files and domain folders (`shell/`, `session/`, `workspace/`, `surfaces/`, `dialogs/`, `plugin_host/`, `stories/`). Legacy `App` fields are clustered in `shell/app_state.rs`; new feature state goes in `opencode_kernel::StateSlots` and modal dialogs, Escape layers, commands, and per-frame polls register in `Feature::register`.
  - `apps/web` (`opencode-web`): thin `wasm32`/WebGPU browser entrypoint depending directly on `opencode-desktop` with zero duplicated `features/*` dependencies.
  - `apps/installer` (`opencode-rune-setup`): Windows per-user setup bootstrapper.
  - `apps/desktop-thin` + `apps/desktop-library`: a packaged Windows build is a thin `opencode-desktop.exe` that loads the app from `opencode-desktop.dll` beside it (`apps/installer/scripts/package.ps1`); `cargo run` and the other platforms build the one-file exe of `apps/desktop`.
  - `apps/browser-engine` (`opencode-browser-engine`): standalone out-of-process CEF sidecar bundle, excluded from the Cargo workspace in root `Cargo.toml`.

Rune comes from `anomalyco/rune` as a git dependency pinned in the root `Cargo.toml`. Keep every `anomalyco/rune` dependency and patch on the same rev (enforced by `scripts/arch-check.sh`); pin only revs on Rune's `dev` branch and land upstream fixes in `anomalyco/rune` first (convention).

## No tests

Application code only. This repo has no automated tests of any kind, and none get added back (Hona and Kit, Nov 10: "Application code is truth, tests are a poor replica"):

- No unit tests, `#[test]` or `#[cfg(test)]` code, `tests.rs` / `*_tests.rs` files, mocks, fuzzers, golden or snapshot files, motion audits, test benchmarks or CI test steps.
- Don't spend time writing, fixing or running tests. Verify a change by building and running the app (`--fixture`, a story, or live).
- Stories and `--fixture` are app features for looking at the app, not tests: don't turn them into assertions.

## Working here

- **Before pushing (cheap guardrails):** run `scripts/guardrails.sh` (macOS/Linux/Git Bash) or `pwsh scripts/guardrails.ps1` (Windows PowerShell). Requires Bun or Node.js 18+ (no Python) and exits non-zero on any failure. Full rule list and conventions live in [`ARCHITECTURE.md`](ARCHITECTURE.md):
  - `scripts/arch-check.sh`: enforces resolved transitive downward crate layers and zero cycles (`cargo metadata`), zero cross-feature deps, core-crate isolation (temporary exceptions in `scripts/arch-check-exceptions.txt`), core/feature `#![forbid(unsafe_code)]` (including `opencode-platform`), `<= 10` root `.rs` files in `apps/desktop/src/`, `struct App` `<= 94` effective fields (counting through `shell/app_state.rs` and any `Tracked` wrappers), `apps/web` depending directly on `opencode-desktop` with zero duplicated `features/*` deps, zero automated tests (`#[test]`, `#[cfg(...test...)]`, `tests.rs`, `*_tests.rs`, `*_test.rs`, `tests/`), hard `<= 800`-line cap in `apps/desktop/`, `features/`, and hand-written workspace `crates/`, zero `use crate::*;` wildcards in `apps/desktop/src/`, `opencode-feature-*` naming, workspace manifest completeness, a single `anomalyco/rune` rev pin, and `--!strict` built-in Luau plugins.
  - `scripts/file-size.sh`: repo-wide line-count monitor and 1,500-line ratchet across every tracked `.rs` file under `apps/`, `crates/`, and `features/` (including `apps/web/`, `apps/installer/`, `apps/browser-engine/`, `crates/browser-pane/`, and generated files outside `arch-check.sh` Rule 8); warns `> 800`, errors `> 1500` unless ratcheted in `scripts/file-size-exceptions.txt`.
  - `scripts/folder-shape.sh`: enforces prefix-family subfolders and folder breadth (`> 40` warn, `> 60` error).
  - `scripts/dialog-size.sh`: every dialog takes an `opencode_ui::dialog::DialogSize` preset (re-exported as `session_view::dialog::DialogSize`; `Normal` 480 × 368, `Large` 680 × 600, `XLarge` 880 × 680; `opencode_ui::widgets::{sized, sized_modal}`, re-exported as `crate::widgets` in `apps/desktop`), never its own width or height.
  - `scripts/tables-check.sh`: checks `apps/desktop/src/stories/order.rs` against `STORY_LIST`, `apps/desktop/src/shell/commands/order.rs` against registered built-in commands, `crates/opencode-i18n/locales/rune/*.json` coverage and unknown keys against `en.json`, and `cargo run -q -p opencode-plugin-spec -- --check` against `contract.json`.
  - `scripts/guardrails/cursor-allow.mjs`: the pointer's cursor follows the official app element by element (the arrow on controls; another cursor only where official's source gives one). Every `.cursor(...)` call, text link (`.link`) and selectable text (`.selectable`) under `apps/`, `crates/` and `features/` is listed in `scripts/cursor-allow.txt` (`path | kind | count | the official line that justifies it`); a call that is not listed, a count that differs and a stale line all fail.
- **Slow / compile checks (run separately):**
  - `scripts/clippy.sh`: zero clippy diagnostics (`-D warnings`) across the workspace and each feature set that compiles different code.
  - `cargo check -p session-view --examples`: keep `session-view`'s standalone examples building.
  - `pwsh scripts/web.ps1 -Check`: checks the `wasm32` browser build (`opencode-web`) and runs `scripts/web-clock.ts` to keep `std::time::Instant` and `SystemTime::now` out of the browser build.
- Fast local loop & profiles: `scripts/dev-fast.sh` / `pwsh scripts/dev-fast.ps1` with Luau `--plugin-hot` (see [`docs/fast-builds.md`](docs/fast-builds.md)); macOS `.app` install via `apps/desktop/scripts/dev.sh` (`apps/desktop/scripts/bundle.sh` / `scripts/mac-app`).
- **Linux builds run in Podman:** compile and package inside containers (`apps/desktop/scripts/package-fedora.sh` for the RPM, and a Fedora container for compile checks or the browser engine) instead of installing Rust toolchains, CEF downloads, or `*-devel` packages on the host. Keep caches in named volumes or the container filesystem; do not let `mise`/`rustup` auto-install a project toolchain on the host.
- Convention: keep personal paths (`/Users/<real-name>`), tailnet hostnames (`*.ts.net`, except `*.example.ts.net`), real session IDs, and credentials out of code, scripts, and commits.

## Official app

The official OpenCode desktop app (`https://github.com/anomalyco/opencode`, mostly `packages/app`, `packages/ui`, and `packages/desktop`) is the reference for behaviour.

- **Match it:** when behaviour here differs from it, or a change here would alter what a user sees, read the official source, match it, and cite the file in a comment (e.g. `packages/app/...`).
- **Don't copy bugs:** keep a difference only where the official behaviour is plainly a bug; use judgement and say so in a comment at the site.

## Explicit state machines

No implicit state machines. Any behaviour with modes or phases is an explicit state machine. Examples: lifecycle, connection and reconnect, streaming, send/queue/steer, drag, popovers and menus, focus, dialogs, quit, update, animation, and request/retry.

- **No encoded state:** don't encode state in combinations of `bool`s, `Option`s, timestamps, counters, `dismissed`/`pending`/`loading` flags or high-water marks.
- **Shape:**
  - One `enum State` whose variants carry only the data valid in that state, so illegal states can't be represented.
  - One transition function, `step(&mut State, Event) -> Effects`. Effects (I/O, timers, redraws, focus moves) are returned for the caller to run, not performed inside the transition.
  - Timers and async completions arrive as events.
- **Official machines:** when the official app has a `*-machine.ts` for the same behaviour, match its states and transitions following the rule above.
- **Touched code:** if you touch code that contains an implicit state machine, convert it in the same change.
