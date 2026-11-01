#!/usr/bin/env bash
# Builds Linux packages for zarc and zarg: deb, rpm, AppImage and Flatpak.
#
# Usage: packaging/linux/build-packages.sh [deb|rpm|appimage|flatpak|all]...
#
# The deb, rpm and AppImage packages are assembled from release binaries, which
# are built with `cargo build --release --bins` unless SKIP_BUILD=1. Output goes
# to dist/ in the repository root.
#
# Environment:
#   BIN_DIR        directory holding zarc and zarg (default: target/release)
#   SKIP_BUILD=1   do not run cargo
#   APPIMAGETOOL   path to appimagetool (default: appimagetool from PATH)
#
# Required tools per format: dpkg-deb (deb), rpmbuild (rpm), appimagetool
# (appimage), flatpak-builder and flatpak (flatpak).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HERE="$ROOT/packaging/linux"
DIST="$ROOT/dist"
BIN_DIR="${BIN_DIR:-$ROOT/target/release}"
VERSION="$(sed -n '/^\[workspace.package\]/,/^\[/{s/^version = "\(.*\)"/\1/p}' "$ROOT/Cargo.toml")"
APP_ID="io.github.brmmm3.Zarg"

case "$(uname -m)" in
    x86_64) DEB_ARCH=amd64 RPM_ARCH=x86_64 ;;
    aarch64 | arm64) DEB_ARCH=arm64 RPM_ARCH=aarch64 ;;
    *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

need() {
    command -v "$1" > /dev/null 2>&1 || { echo "error: '$1' is required for this format" >&2; exit 1; }
}

build_binaries() {
    if [ "${SKIP_BUILD:-0}" != 1 ]; then
        (cd "$ROOT" && cargo build --release --locked --bins -p zarc -p zarg)
    fi
    [ -x "$BIN_DIR/zarc" ] && [ -x "$BIN_DIR/zarg" ] \
        || { echo "error: zarc and zarg not found in $BIN_DIR" >&2; exit 1; }
}

# Lays out an install root shared by deb, rpm and AppImage.
stage_files() {
    local stage="$1"
    rm -rf "$stage"
    install -Dm755 "$BIN_DIR/zarc" "$stage/usr/bin/zarc"
    install -Dm755 "$BIN_DIR/zarg" "$stage/usr/bin/zarg"
    install -Dm644 "$ROOT/zarg/data/zarg.desktop" "$stage/usr/share/applications/zarg.desktop"
    install -Dm644 "$ROOT/zarg/data/zarg-error.desktop" \
        "$stage/usr/share/applications/zarg-error.desktop"
    install -Dm644 "$ROOT/zarg/assets/zarg-icon.png" \
        "$stage/usr/share/icons/hicolor/256x256/apps/zarg.png"
    install -Dm644 "$ROOT/zarg/assets/zarg-error.png" \
        "$stage/usr/share/icons/hicolor/64x64/apps/zarg-error.png"
    install -Dm644 "$ROOT/zarg/assets/application-x-zar.png" \
        "$stage/usr/share/icons/hicolor/64x64/mimetypes/application-x-zar.png"
    install -Dm644 "$HERE/zarg-mime.xml" \
        "$stage/usr/share/mime/packages/zar-rs.xml"
    if [ "${2:-}" != portable ]; then
        install -Dm644 "$HERE/dolphin-files.desktop" \
            "$stage/usr/share/kio/servicemenus/zarg-files.desktop"
        install -Dm644 "$HERE/dolphin-archives.desktop" \
            "$stage/usr/share/kio/servicemenus/zarg-archives.desktop"
        install -Dm755 "$HERE/nautilus-script.sh" \
            "$stage/usr/libexec/zar-rs/nautilus-script"
        mkdir -p "$stage/usr/share/nautilus/scripts"
        local script_name
        for script_name in \
            "ZAR - Create Archive..." \
            "ZAR - Add to Archive..." \
            "ZAR - Create Archive (named)" \
            "ZAR - Open Archive" \
            "ZAR - Extract here" \
            "ZAR - Extract to..." \
            "ZAR - Extract to archive folder" \
            "ZAR - Test Archive"; do
            ln -s /usr/libexec/zar-rs/nautilus-script \
                "$stage/usr/share/nautilus/scripts/$script_name"
        done
        install -Dm644 "$HERE/thunar-uca.xml" \
            "$stage/usr/share/zar-rs/thunar-uca.xml"
        install -Dm755 "$HERE/install-thunar-actions.py" \
            "$stage/usr/bin/zarg-install-thunar-actions"
    fi
    install -Dm644 "$ROOT/LICENSE-MIT" "$stage/usr/share/doc/zar-rs/LICENSE-MIT"
    install -Dm644 "$ROOT/LICENSE-APACHE" "$stage/usr/share/doc/zar-rs/LICENSE-APACHE"
    install -Dm644 "$ROOT/ReadMe.md" "$stage/usr/share/doc/zar-rs/ReadMe.md"
}

build_deb() {
    need dpkg-deb
    local stage="$DIST/work/deb"
    stage_files "$stage"
    mkdir -p "$stage/DEBIAN"
    local size
    size="$(du -sk --exclude=DEBIAN "$stage" | cut -f1)"
    cat > "$stage/DEBIAN/control" << EOF
Package: zar-rs
Version: $VERSION
Section: utils
Priority: optional
Architecture: $DEB_ARCH
Installed-Size: $size
Depends: libc6, libgcc-s1
Recommends: xdg-desktop-portal, libgl1, libxkbcommon0, libwayland-client0, libx11-6,
 python3, shared-mime-info
Maintainer: Martin Bammer <mrbm74@gmail.com>
Homepage: https://github.com/brmmm3/zar-rs
Description: ZAR archive command-line tool and archive manager
 ZAR is a block-based, appendable archive format with Zstandard compression
 and optional encryption. This package provides the zarc command-line tool
 and the zarg graphical archive manager.
EOF
    cat > "$stage/DEBIAN/postinst" << 'EOF'
#!/bin/sh
set -e
if [ "$1" = configure ] && command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database /usr/share/mime
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t /usr/share/icons/hicolor
fi
EOF
    chmod 755 "$stage/DEBIAN/postinst"
    cat > "$stage/DEBIAN/postrm" << 'EOF'
#!/bin/sh
set -e
if [ "$1" = remove ] || [ "$1" = purge ]; then
    if command -v update-mime-database >/dev/null 2>&1; then
        update-mime-database /usr/share/mime
    fi
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t /usr/share/icons/hicolor
fi
EOF
    chmod 755 "$stage/DEBIAN/postrm"
    mkdir -p "$stage/usr/share/doc/zar-rs"
    cat > "$stage/usr/share/doc/zar-rs/copyright" << EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: zar-rs
Source: https://github.com/brmmm3/zar-rs

Files: *
Copyright: 2026 The ZAR contributors
License: MIT or Apache-2.0
 The full texts are in LICENSE-MIT and LICENSE-APACHE in this directory.
EOF
    chmod 644 "$stage/usr/share/doc/zar-rs/copyright"
    mkdir -p "$DIST"
    dpkg-deb --root-owner-group -Zxz --build "$stage" "$DIST/zar-rs_${VERSION}_${DEB_ARCH}.deb"
    echo "Created $DIST/zar-rs_${VERSION}_${DEB_ARCH}.deb"
}

build_rpm() {
    need rpmbuild
    local stage="$DIST/work/rpm-stage" top="$DIST/work/rpmbuild"
    stage_files "$stage"
    rm -rf "$top"
    mkdir -p "$top"/{BUILD,RPMS,SRPMS,SPECS} "$DIST"
    rpmbuild -bb "$HERE/zar-rs.spec" \
        --target "$RPM_ARCH" \
        --define "_topdir $top" \
        --define "pkg_version $VERSION" \
        --define "stage_dir $stage"
    cp "$top"/RPMS/*/zar-rs-*.rpm "$DIST/"
    echo "Created RPM in $DIST"
}

build_appimage() {
    local tool="${APPIMAGETOOL:-appimagetool}"
    need "$tool"
    local appdir="$DIST/work/ZAR.AppDir"
    stage_files "$appdir" portable
    install -m755 "$HERE/AppRun" "$appdir/AppRun"
    install -m644 "$ROOT/zarg/data/zarg.desktop" "$appdir/zarg.desktop"
    install -m644 "$ROOT/zarg/assets/zarg-icon.png" "$appdir/zarg.png"
    mkdir -p "$DIST"
    ARCH="$(uname -m)" "$tool" "$appdir" "$DIST/ZAR-${VERSION}-$(uname -m).AppImage"
    echo "Created $DIST/ZAR-${VERSION}-$(uname -m).AppImage"
}

build_flatpak() {
    need flatpak-builder
    need flatpak
    local work="$DIST/work/flatpak"
    rm -rf "$work"
    mkdir -p "$work" "$DIST"
    flatpak-builder --force-clean --user --install-deps-from=flathub \
        --repo="$work/repo" "$work/build" "$HERE/$APP_ID.yml"
    flatpak build-bundle "$work/repo" "$DIST/zar-rs-${VERSION}-$(uname -m).flatpak" "$APP_ID"
    echo "Created $DIST/zar-rs-${VERSION}-$(uname -m).flatpak"
}

formats=("$@")
[ ${#formats[@]} -eq 0 ] && formats=(all)
if [ "${formats[0]}" = all ]; then
    formats=(deb rpm appimage flatpak)
fi

for format in "${formats[@]}"; do
    case "$format" in
        deb | rpm | appimage) build_binaries ;;
    esac
done

for format in "${formats[@]}"; do
    case "$format" in
        deb) build_deb ;;
        rpm) build_rpm ;;
        appimage) build_appimage ;;
        flatpak) build_flatpak ;;
        *) echo "unknown format: $format" >&2; exit 1 ;;
    esac
done
rm -rf "$DIST/work"
