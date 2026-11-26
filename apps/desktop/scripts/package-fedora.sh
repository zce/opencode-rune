#!/usr/bin/env bash
# Build and export one Fedora RPM without mounting the checkout into the container.
set -euo pipefail

if [[ $# != 2 || ! $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9]+([.-][A-Za-z0-9]+)*)?$ ]]; then
  echo "usage: package-fedora.sh VERSION OUTPUT-DIRECTORY (VERSION is X.Y.Z or X.Y.Z-prerelease)" >&2
  exit 2
fi
version=$1
mkdir -p -- "$2"
out=$(realpath -- "$2")
rpm="$out/OpenCode-Rune-$version-fedora45-$(uname -m).rpm"
if [[ -e $rpm || -L $rpm ]]; then
  echo "Refusing to overwrite $rpm" >&2
  exit 1
fi
partial=$(mktemp "$out/.rune-rpm.XXXXXXXX")
trap 'rm -f -- "$partial"' EXIT

cd -- "$(dirname -- "$0")/../../.."
fedora=apps/desktop/packaging/fedora
# Include the recipe even before it's tracked, but not unrelated untracked files.
{
  git ls-files -z -- . ":(exclude)$fedora"
  printf '%s\0' "$fedora"/{build.sh,opencode-rune.desktop,opencode-rune.spec}
} | tar --null -T - -cf - |
  podman run --rm --init --pull=missing -i \
    -e OPENCODE_RUNE_VERSION="$version" -e OPENCODE_FEDORA_MIRROR \
    registry.fedoraproject.org/fedora:45 \
    bash -euc 'mkdir /build; cd /build; tar -xf -; exec bash "$1/build.sh"' -- "$fedora" > "$partial"

# Publish only a finished RPM, and never clobber an existing path.
ln -T -- "$partial" "$rpm"
printf '%s\n' "$rpm"
