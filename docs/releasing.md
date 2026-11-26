# Releasing OpenCode (Rune)

`.github/workflows/release.yml` builds, signs and publishes the desktop app to `https://anoma.ly/release` (app `opencode`, artifact `desktop-rune`: `opencode/desktop-rune/<channel>`).

## Channels

**dev** is the only desktop channel. There is no stable channel.

- **dev**: checks `dev` every six hours (`17 */6 * * *` UTC). A scheduled run skips the builds when no app files changed; dispatching `dev` builds immediately. Each release publishes `0.0.0-dev-<n>` with all 14 targets. A failed build, signing step or package check leaves the previous complete release available. Once the release service has it, the last job tags `v0.0.0-dev-<n>` on the released commit and mirrors the same signed files, byte for byte, to that tag's GitHub release, marked latest, with notes generated since the previous dev tag. The release's `notes_url` points at that GitHub release.
- **check**: dispatch from `dev`. It builds, signs and verifies, and publishes nothing. Its last job pushes and deletes a throwaway `v0.0.0-test-<n>` tag.

```sh
gh workflow run release.yml -R anomalyco/opencode-rune --ref dev -f channel=dev
```

Tags start nothing, and the workflow has no ref input: a run always releases `github.sha` on `dev`.

PR builds (`opencode-rune-pr-build.yml`) are separate: each PR run keeps an ad-hoc signed macOS app and unsigned Windows and Linux x64 builds as workflow artifacts for 7 days, and publishes nothing.

To build Linux packages of any branch or commit without a PR or a publish (x64, and arm64 when asked), run `opencode-rune-linux-build.yml`; its artifact is kept for 7 days:

```sh
gh workflow run opencode-rune-linux-build.yml -R anomalyco/opencode-rune -f ref=<branch> [-f arm64=true]
```

## Latest downloads

Each link redirects to the dev channel's current file. Append `/info` to a link for its version, byte size and SHA-256. The same files are on the newest [GitHub release](https://github.com/anomalyco/opencode-rune/releases/latest).

| Platform | Dev |
| --- | --- |
| macOS Apple Silicon | [DMG](https://anoma.ly/release/opencode/desktop-rune/dev/macos-arm64-dmg) · [update ZIP](https://anoma.ly/release/opencode/desktop-rune/dev/macos-arm64) |
| macOS Intel | [DMG](https://anoma.ly/release/opencode/desktop-rune/dev/macos-x64-dmg) · [update ZIP](https://anoma.ly/release/opencode/desktop-rune/dev/macos-x64) |
| Windows x64 | [setup](https://anoma.ly/release/opencode/desktop-rune/dev/windows-x64-setup) · [update ZIP](https://anoma.ly/release/opencode/desktop-rune/dev/windows-x64) |
| Windows arm64 | [setup](https://anoma.ly/release/opencode/desktop-rune/dev/windows-arm64-setup) · [update ZIP](https://anoma.ly/release/opencode/desktop-rune/dev/windows-arm64) |
| Linux x64 | [AppImage](https://anoma.ly/release/opencode/desktop-rune/dev/linux-x64-appimage) · [DEB](https://anoma.ly/release/opencode/desktop-rune/dev/linux-x64-deb) · [RPM](https://anoma.ly/release/opencode/desktop-rune/dev/linux-x64-rpm) |
| Linux arm64 | [AppImage](https://anoma.ly/release/opencode/desktop-rune/dev/linux-arm64-appimage) · [DEB](https://anoma.ly/release/opencode/desktop-rune/dev/linux-arm64-deb) · [RPM](https://anoma.ly/release/opencode/desktop-rune/dev/linux-arm64-rpm) |

The Mac and Windows updaters read the bare `<os>-<arch>` ZIP targets. Linux uses manual downloads; its packages are built on Ubuntu 24.04. Set the AppImage's executable bit before launching it (`chmod +x`). The release registry authorizes the artifact and channels; adding a target requires no registry change.

## Local Fedora RPM

```sh
bash apps/desktop/scripts/package-fedora.sh 0.0.1 /path/to/output
```

This builds an unsigned RPM in a disposable Fedora 45 Podman container. No host toolchain, checkout mount, install, or publish is involved. The package uses the `opencode-rune` package name and app profile, so it replaces rather than sits alongside another `opencode-rune` RPM if installed. The build uses the `local` channel and keeps the published browser engine's SHA-256 pin.

On Wayland without `xdg-decoration` (including GNOME), the application draws its own 44 pt titlebar row with the window controls in it; it keeps normal decorations on X11 and compositors advertising the protocol. The Fedora build pins the Rune revision that carries that frame and the Linux `org.freedesktop.Notifications` backend (see `Cargo.toml`), so no Rune patches are applied. It does not affect the other Linux packages.

If Fedora's default mirrors are slow, set `OPENCODE_FEDORA_MIRROR` to the HTTPS root of a Fedora mirror, for example `https://mirrors.ustc.edu.cn/fedora/linux`.

## Comparison with the official desktop

Source: [official release workflow](https://github.com/anomalyco/opencode/blob/38bd73789a43bf043599d6196af96583228035d7/.github/workflows/release.yml) and [desktop publisher](https://github.com/anomalyco/opencode/blob/38bd73789a43bf043599d6196af96583228035d7/.github/release/publish-desktop.sh).

| | Official | Rune |
| --- | --- | --- |
| Cadence | Six-hour dev schedule; beta on push; dispatch from dev for latest | Six-hour dev schedule with app-change gate; dispatch for dev or check |
| macOS | arm64 + x64 DMG and ZIP; legacy `.app.tar.gz` on latest | arm64 + x64 DMG and ZIP |
| Windows | x64 + arm64 signed installer EXE | x64 + arm64 signed setup EXE and signed application ZIP |
| Linux | x64 + arm64 AppImage, DEB, RPM; per-architecture Electron update feeds | Same architectures and packages; manual updates |
| Signing | Apple Developer ID and notarization; Azure Authenticode; unsigned Linux packages | Same signing policy; every artifact checked against the producing job's SHA256SUMS digest |
| Publishing | `opencode.ai/files` (R2); `desktop` / distribution `opencode` at `opencode.ai/update`; `dev`, `beta`, `latest` | `anoma.ly/release/opencode/desktop-rune`, `dev` only, mirrored to GitHub releases; target names in the links above |

On a fully active day the schedule can build 24 OS/architecture combinations, up from seven (five Windows x64, one Windows arm64 and one Mac arm64). The change gate keeps idle runs to one Linux planning job, and concurrency lets the active release finish with only the newest pending run per channel. If runner spend exceeds budget, reduce the number of scheduled releases while retaining every target in each publish.

## Browser engine

The desktop starts without CEF and downloads the embedded browser engine on first use from `https://anoma.ly/release/opencode-rune/browser-engine/stable.json` (app `opencode-rune`, artifact `browser-engine`, channel `stable`). All six desktop targets ship an engine: `windows-x64`, `windows-arm64`, `macos-arm64`, `macos-x64`, `linux-x64`, and `linux-arm64`.

### Trust defaults (`crates/browser-pane/src/source.rs`)

- **Windows (`windows-x64`, `windows-arm64`) release builds:** `publisher:Anomaly Innovations, Inc https://anoma.ly/` — verifies the Authenticode signature of every `.exe` and `.dll` in the archive against `features/updater/src/windows.rs` (`EXPECTED_PUBLISHER`). Windows debug builds keep the local stand-in (`http://127.0.0.1:47981/index.json`, `local`).
- **macOS (`macos-arm64`, `macos-x64`):** `publisher:5NZ4Q7NXJ4` — verifies `OpenCode Browser Engine.app` with `codesign --verify --deep --strict` against Developer ID team `5NZ4Q7NXJ4`.
- **Linux (`linux-x64`, `linux-arm64`):** defaults to the index URL with an empty trust rule; the release workflow passes `OPENCODE_BROWSER_ENGINE_TRUST=sha256:<hex>` at build time for each architecture. A Linux build without it fails closed at startup as `Fault::NotPublished` (no download, no Retry button, and no local `browser_*` MCP tools).

### Versioning and installed-app updates

An engine release version is `<cef>-<ENGINE_REVISION>` (`crates/browser-pane/src/installer.rs`).

- An engine change under `apps/browser-engine/` that must reach users bumps `ENGINE_REVISION` in `crates/browser-pane/src/installer.rs`. The release workflow builds an engine only when the index lacks `<cef>-<revision>` (publishing every engine target together in one request, because the release service replaces the channel manifest whole) and refuses to republish a complete version.
- **Higher CEF build at the same revision (`installer.rs`, `prefetch.rs`, `sidecar.rs`):** installed Windows and macOS apps check the index once per run after the first tab opens, download and install the newer engine beside the running one for the next start, and retain the newest version per revision. Installed Linux apps pin a single archive SHA-256 (`source.rs`), so they ignore a different archive hash and keep running their installed engine until the desktop app updates (fresh installs of an older pinned Linux build also need the updated desktop build).
- **Bumped `ENGINE_REVISION` (for example, 2 to 3):** apps require exact revision equality (`version.revision == ENGINE_REVISION`), so an installed revision-2 app ignores revision 3 and continues running its installed revision-2 engine; a fresh install of an older app reports `Fault::NotPublished` until the desktop app updates.

### Linux preview dispatch

Dispatch `release.yml` from `dev` with `channel=engine-linux` to build the x86_64 CEF sidecar and publish a private GitHub prerelease. Its `index.json`, `.tar.zst` and `SHA256SUMS` can be downloaded together with `gh release download <tag>`. A debug desktop build accepts that local index with `OPENCODE_BROWSER_ENGINE_SOURCE=file:$PWD/index.json OPENCODE_BROWSER_ENGINE_TRUST=local`.

## Security boundary

- Build and verify jobs run repository code with no environment, secrets or `id-token`.
- Sign and publish jobs use the protected `release` environment, which can deploy only from `dev`. They sparse-check out only `.github/release/` at `github.sha` and treat artifacts as data, checked against the SHA256SUMS digest the job that built them reported.
- Signing credentials are stored outside the repository and fetched via GitHub OIDC only inside the step that uses them (`.github/release/with-secrets.sh`), never exported into `$GITHUB_ENV`.
- The `release` environment holds only non-secret identifiers (`AZURE_*` and Apple notary IDs); repository and environment secrets stay empty.
- The release service authorizes publishes via GitHub OIDC restricted to `anomalyco/opencode-rune`, `.github/workflows/release.yml`, `refs/heads/dev` and the `release` environment.
- Changes to `.github/` require code-owner approval (`.github/CODEOWNERS`).
- PR builds are never signed with release credentials. Their macOS app is ad-hoc signed.
