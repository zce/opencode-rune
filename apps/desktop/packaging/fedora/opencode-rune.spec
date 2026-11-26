Name: opencode-rune
Version: %{rune_version}
Release: 1%{?dist}
Summary: OpenCode (Rune) desktop client
License: MIT
Requires: libxkbcommon, fontconfig, libwayland-client, libX11, alsa-lib, pipewire-utils
BuildRequires: desktop-file-utils

%description
OpenCode (Rune) native desktop client with an integrated titlebar on GNOME Wayland.

%install
install -Dm755 %{rune_src}/target/release-quick/opencode-desktop %{buildroot}%{_bindir}/opencode-rune
install -Dm644 %{rune_src}/apps/desktop/assets/icon/icon-1024.png %{buildroot}%{_datadir}/icons/hicolor/1024x1024/apps/opencode-rune.png
install -Dm644 %{rune_src}/apps/desktop/packaging/fedora/opencode-rune.desktop %{buildroot}%{_datadir}/applications/opencode-rune.desktop
install -Dm644 %{rune_src}/LICENSE %{buildroot}%{_licensedir}/%{name}/LICENSE
install -Dm644 %{rune_src}/THIRD_PARTY_NOTICES.md %{buildroot}%{_docdir}/%{name}/THIRD_PARTY_NOTICES.md
desktop-file-validate %{buildroot}%{_datadir}/applications/opencode-rune.desktop

%files
%{_bindir}/opencode-rune
%{_datadir}/applications/opencode-rune.desktop
%{_datadir}/icons/hicolor/1024x1024/apps/opencode-rune.png
%dir %{_licensedir}/%{name}
%license %{_licensedir}/%{name}/LICENSE
%dir %{_docdir}/%{name}
%doc %{_docdir}/%{name}/THIRD_PARTY_NOTICES.md
