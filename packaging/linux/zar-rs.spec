# RPM spec that packages prebuilt binaries; invoked by packaging/linux/build-packages.sh.
# Required defines: pkg_version, stage_dir (a directory laid out like the install root).
%global debug_package %{nil}
%global __strip /bin/true

Name:           zar-rs
Version:        %{pkg_version}
Release:        1%{?dist}
Summary:        ZAR archive command-line tool and archive manager
License:        MIT OR Apache-2.0
URL:            https://github.com/brmmm3/zar-rs
Recommends:     xdg-desktop-portal
Recommends:     libxkbcommon

%description
ZAR is a block-based, appendable archive format with Zstandard compression and
optional encryption. This package provides the zarc command-line tool and the
zarg graphical archive manager.

%install
mkdir -p %{buildroot}
cp -a %{stage_dir}/. %{buildroot}/

%files
%license /usr/share/doc/zar-rs/LICENSE-MIT
%license /usr/share/doc/zar-rs/LICENSE-APACHE
%doc /usr/share/doc/zar-rs/ReadMe.md
/usr/bin/zarc
/usr/bin/zarg
/usr/share/applications/zarg.desktop
/usr/share/icons/hicolor/256x256/apps/zarg.png

%changelog
* Sat Oct 31 2026 The ZAR contributors - %{pkg_version}-1
- Packaged release %{pkg_version}
