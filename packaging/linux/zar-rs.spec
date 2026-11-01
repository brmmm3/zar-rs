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
Recommends:     python3
Recommends:     shared-mime-info

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
/usr/bin/zarg-install-thunar-actions
/usr/share/applications/zarg.desktop
/usr/share/applications/zarg-error.desktop
/usr/share/icons/hicolor/256x256/apps/zarg.png
/usr/share/icons/hicolor/64x64/apps/zarg-error.png
/usr/share/icons/hicolor/64x64/mimetypes/application-x-zar.png
/usr/share/kio/servicemenus/zarg-files.desktop
/usr/share/kio/servicemenus/zarg-archives.desktop
/usr/share/mime/packages/zar-rs.xml
/usr/libexec/zar-rs/nautilus-script
/usr/share/nautilus/scripts/*
/usr/share/zar-rs/thunar-uca.xml

%post
if [ -x /usr/bin/update-mime-database ]; then
    /usr/bin/update-mime-database /usr/share/mime >/dev/null 2>&1 || :
fi
if [ -x /usr/bin/gtk-update-icon-cache ]; then
    /usr/bin/gtk-update-icon-cache -q -t /usr/share/icons/hicolor >/dev/null 2>&1 || :
fi

%postun
if [ -x /usr/bin/update-mime-database ]; then
    /usr/bin/update-mime-database /usr/share/mime >/dev/null 2>&1 || :
fi
if [ -x /usr/bin/gtk-update-icon-cache ]; then
    /usr/bin/gtk-update-icon-cache -q -t /usr/share/icons/hicolor >/dev/null 2>&1 || :
fi

%changelog
* Sat Oct 31 2026 The ZAR contributors - %{pkg_version}-1
- Packaged release %{pkg_version}
