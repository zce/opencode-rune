#!/usr/bin/env bash
# Runs inside the disposable build container; stdout is reserved for the RPM.
set -euo pipefail
exec 3>&1 >&2

fedora=apps/desktop/packaging/fedora
rev=$(sed -nE 's/^rune = .*rev = "([0-9a-f]{40})".*/\1/p' Cargo.toml)

# Allow a faster mirror without changing the container image's package signature checks.
repo_args=()
if [[ -n ${OPENCODE_FEDORA_MIRROR:-} ]]; then
  mirror=${OPENCODE_FEDORA_MIRROR%/}
  [[ $mirror == https://* ]] || { echo "OPENCODE_FEDORA_MIRROR must use HTTPS." >&2; exit 2; }
  repo_args=(
    --setopt=fedora.metalink= "--setopt=fedora.baseurl=$mirror/releases/45/Everything/\$basearch/os/"
    --setopt=updates.metalink= "--setopt=updates.baseurl=$mirror/updates/45/Everything/\$basearch/"
  )
fi
dnf -qy --disablerepo=fedora-cisco-openh264 --setopt=install_weak_deps=False \
  --setopt=timeout=30 --setopt=retries=2 "${repo_args[@]}" install \
  git-core gcc-c++ clang lld make cmake pkgconf-pkg-config nasm xz jq \
  fontconfig-devel libxkbcommon-devel wayland-devel libX11-devel libxcb-devel \
  dbus-devel systemd-devel openssl-devel alsa-lib-devel rpm-build desktop-file-utils

machine=$(uname -m)
case $machine in
  x86_64) arch=x64 ;;
  aarch64) arch=arm64 ;;
  *) echo "Unsupported architecture: $machine" >&2; exit 1 ;;
esac
work=$(mktemp -d)
fetch() { curl -fsSL --retry 2 --connect-timeout 15 --max-time 120 "$@"; }

toolchain=$(sed -nE 's/^channel = "([^"]+)"/\1/p' rust-toolchain.toml)
fetch "https://static.rust-lang.org/rustup/dist/$machine-unknown-linux-gnu/rustup-init" -o "$work/rustup-init"
chmod +x "$work/rustup-init"
(cd "$work"; ./rustup-init -y --no-modify-path --profile minimal --default-toolchain "$toolchain")
export PATH="$HOME/.cargo/bin:$PATH" RUSTUP_TOOLCHAIN="$toolchain"

rune=$work/rune
mkdir "$rune"
fetch "https://codeload.github.com/zce/rune/tar.gz/$rev" | tar -xz --strip-components=1 -C "$rune"
# The pinned Rune revision already carries the Linux titlebar frame and the
# org.freedesktop.Notifications backend; the RPM applies no Rune patches.
# Redirect all Rune crates and vendored patches to the same tree in this container.
# The locked versions are unchanged; only the Rune package source identifiers change.
while read -r name; do
  case $name in
    rune-theme-opencode) path=themes/opencode ;;
    naga|winit|wgpu-hal|sctk-adwaita|accesskit_*|parley_engine) path=vendor/$name ;;
    *) path=crates/$name ;;
  esac
  test -f "$rune/$path/Cargo.toml"
  sed -i "/^$name = /s@git = \"https://github.com/anomalyco/rune\", rev = \"$rev\"@path = \"$rune/$path\"@" Cargo.toml
done < <(sed -n '/git = "https:\/\/github.com\/anomalyco\/rune"/s/ .*//p' Cargo.toml)
sed -i '/^source = "git+https:\/\/github.com\/anomalyco\/rune?/d' Cargo.lock

# Use the same pinned browser engine as the official Linux build of this source.
cef=$(grep -A1 '^name = "cef-dll-sys"$' apps/browser-engine/Cargo.lock |
  sed -n 's/^version = "[^"]*+\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)"$/\1/p')
revision=$(sed -n 's/^pub const ENGINE_REVISION: u32 = \([0-9][0-9]*\);$/\1/p' crates/browser-pane/src/installer.rs)
engine=$(bash .github/release/opencode-rune-check-engine.sh --plan-state --version "$cef-$revision")
pin=$(sed -n "s/^linux_${arch}_sha256=//p" <<< "$engine")
if [[ ! $pin =~ ^[0-9a-f]{64}$ ]]; then
  echo "No published browser engine $cef-$revision for linux-$arch; refusing to build an RPM with the browser disabled." >&2
  exit 1
fi
export OPENCODE_BROWSER_ENGINE_TRUST="sha256:$pin" OPENCODE_RUNE_BUILD_CHANNEL=local

RUSTFLAGS="-C link-arg=-fuse-ld=lld" cargo build --locked -p opencode-desktop --profile release-quick
pkg_version=${OPENCODE_RUNE_VERSION/-/\~}
pkg_version=${pkg_version//-/.}
rpmbuild -bb --define "_topdir $work/rpm" --define "rune_src $PWD" \
  --define "rune_version $pkg_version" --define 'debug_package %{nil}' "$fedora/opencode-rune.spec"

shopt -s nullglob
packages=("$work/rpm/RPMS/$machine/"*.rpm)
[[ ${#packages[@]} == 1 ]]
rpm -qp --list "${packages[0]}" | grep -Fx /usr/bin/opencode-rune
# This local package has no signing key; still verify its header and payload digests.
rpmkeys --define '_pkgverify_level digest' --checksig "${packages[0]}"
mkdir "$work/extracted"
rpm2archive - < "${packages[0]}" | tar -xz -C "$work/extracted"
[[ $("$work/extracted/usr/bin/opencode-rune" --version) == "opencode-rune $OPENCODE_RUNE_VERSION (local)" ]]
cat "${packages[0]}" >&3
